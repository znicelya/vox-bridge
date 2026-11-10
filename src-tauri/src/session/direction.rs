use super::{
    resources::*,
    runtime::{
        ActorExit, ActorUpdate, DirectionControl, DirectionPlan, audio_failure, now_ms,
        runtime_error, validate_request,
    },
    transcript_sink::TranscriptSink,
};
use crate::{
    audio::{
        AudioFrame, PcmSpec,
        packetizer::AudioPacketizer,
        router::{AudioRouter, Direction as RouterDirection, EndpointHealthCheck, RouteAdmission},
        runtime::AudioBackend,
    },
    domain::{
        audio::PLAYBACK_SAMPLE_RATE_HZ,
        launch::{DirectionPhase, DirectionSnapshot},
        session::{SessionDirection, SessionMode},
        transcript::{
            DiagnosticPayload, GapCause, PlaybackBoundary, RuntimeDiagnostic, RuntimeFailureCode,
        },
    },
    error::{AppError, AppResult},
    events::{EventBus, SessionEvent},
    providers::{
        ProviderConnection, ProviderError, ProviderEvent, factory::PreparedProviderSession,
    },
    storage::LocalStore,
};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::{
    sync::{mpsc, watch},
    time::{Instant, MissedTickBehavior},
};
use uuid::Uuid;

const CAPTURE_STOP_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(not(test))]
const CAPTURE_DRAIN_TIMEOUT: Duration = Duration::from_secs(4);
#[cfg(test)]
const CAPTURE_DRAIN_TIMEOUT: Duration = Duration::from_millis(400);
#[cfg(not(test))]
const PROVIDER_FINISH_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
const PROVIDER_FINISH_TIMEOUT: Duration = Duration::from_millis(400);
#[cfg(not(test))]
const PLAYBACK_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const PLAYBACK_DRAIN_TIMEOUT: Duration = Duration::from_millis(400);
const PENDING_PACKET_CAPACITY: usize = 16;

#[derive(Clone, Default)]
pub(super) struct RuntimeRouteHealth(
    Arc<Mutex<BTreeMap<String, watch::Receiver<crate::audio::runtime::AudioHealth>>>>,
);
impl EndpointHealthCheck for RuntimeRouteHealth {
    fn is_ready(&self, endpoint: &crate::audio::platform::AudioEndpoint) -> bool {
        self.0
            .lock()
            .unwrap()
            .get(&endpoint.id)
            .is_some_and(|health| *health.borrow() == crate::audio::runtime::AudioHealth::Ready)
    }
}

type Admission = Pin<
    Box<dyn Future<Output = (crate::providers::ProviderResult<()>, Duration, SystemTime)> + Send>,
>;
fn admission(stream: Arc<dyn ProviderConnection>, packet: crate::audio::AudioFrame) -> Admission {
    Box::pin(async move {
        let duration = packet.duration();
        let captured_at = packet.captured_at();
        (stream.send_audio(packet).await, duration, captured_at)
    })
}

pub(super) struct DirectionActor {
    pub(super) session_id: Uuid,
    pub(super) plan: DirectionPlan,
    pub(super) prepared: PreparedProviderSession,
    pub(super) audio: Arc<dyn AudioBackend>,
    pub(super) store: Arc<LocalStore>,
    pub(super) bus: EventBus,
    pub(super) stop: watch::Receiver<bool>,
    pub(super) controls: mpsc::Receiver<DirectionControl>,
    pub(super) updates: mpsc::Sender<ActorUpdate>,
    pub(super) sink: TranscriptSink,
    pub(super) epoch: u64,
    pub(super) failures: Vec<RuntimeFailureCode>,
    pub(super) diagnostics: DiagnosticAccumulator,
    pub(super) transition_failures: Vec<RuntimeFailureCode>,
    pub(super) router: Arc<AudioRouter<RuntimeRouteHealth>>,
    pub(super) route: RouterDirection,
    pub(super) health: RuntimeRouteHealth,
}

impl DirectionActor {
    pub(super) async fn run(mut self) -> ActorExit {
        let mut resources = match self.open_resources().await {
            Ok(resources) => resources,
            Err(code) => return self.finish_exit(Some(code)).await,
        };
        if let Err(code) = self.await_started(&mut resources).await {
            self.abort_resources(resources).await;
            return self.finish_exit(Some(code)).await;
        }
        if let Err(code) = self.activate(&resources) {
            self.abort_resources(resources).await;
            return self.finish_exit(Some(code)).await;
        }
        self.publish(DirectionPhase::Running, None).await;
        loop {
            match self.run_connected(resources).await {
                ConnectedExit::Stopped(failure) => return self.finish_exit(failure).await,
                ConnectedExit::Paused(code) => {
                    if code != RuntimeFailureCode::Interrupted || !*self.stop.borrow() {
                        self.fail(code.clone());
                    }
                    if *self.stop.borrow() {
                        return self.finish_exit(None).await;
                    }
                    self.publish(DirectionPhase::Paused, Some(code)).await;
                    loop {
                        if *self.stop.borrow() {
                            return self.finish_exit(None).await;
                        }
                        tokio::select! {
                            _ = self.stop.changed() => {},
                            control = self.controls.recv() => match control {
                                Some(DirectionControl::Resume(result)) => {
                                    let response = self.resume().await;
                                    match response {
                                        Ok(next) => {
                                            resources = next;
                                            self.publish(DirectionPhase::Running, None).await;
                                            let _ = self.updates.send(ActorUpdate::ResumeComplete { direction: self.plan.direction, result, response: Ok(()) }).await;
                                            break;
                                        }
                                        Err(error) => {
                                            let code = audio_failure(&error);
                                            if code != RuntimeFailureCode::Interrupted || !*self.stop.borrow() { self.fail(code.clone()); }
                                            self.publish(DirectionPhase::Paused, Some(code)).await;
                                            let _ = self.updates.send(ActorUpdate::ResumeComplete { direction: self.plan.direction, result, response: Err(error) }).await;
                                        }
                                    }
                                }
                                None => return self.finish_exit(None).await,
                            }
                        }
                    }
                }
            }
        }
    }

    async fn finish_exit(&mut self, code: Option<RuntimeFailureCode>) -> ActorExit {
        if let Some(code) = code {
            if code != RuntimeFailureCode::Interrupted || !*self.stop.borrow() {
                self.fail(code);
            }
        }
        self.pause_route();
        self.publish(DirectionPhase::Stopped, self.failures.last().cloned())
            .await;
        ActorExit {
            direction: self.plan.direction,
            failures: self.failures.clone(),
        }
    }

    fn cleanup_event(&mut self, event: &ProviderEvent) -> Result<(), RuntimeFailureCode> {
        if let ProviderEvent::Failed { error, metadata } = event {
            self.fail_provider(error, metadata.log_id.clone());
            Ok(())
        } else {
            self.handle_non_audio_event(event)
        }
    }

    fn record_cleanup(&mut self, report: CleanupReport) -> Result<(), RuntimeFailureCode> {
        self.record_cleanup_with_boundary(report, None)
    }

    fn record_cleanup_with_boundary(
        &mut self,
        report: CleanupReport,
        boundary: Option<PlaybackBoundary>,
    ) -> Result<(), RuntimeFailureCode> {
        for code in report.failures {
            if code != RuntimeFailureCode::Interrupted || !*self.stop.borrow() {
                self.fail(code);
            }
        }
        let mut storage_failed = false;
        for statistics in report.statistics {
            let capture = self.persist_diagnostic(DiagnosticPayload::CaptureAccounting {
                statistics: statistics.capture,
            });
            let playback = self.persist_diagnostic(DiagnosticPayload::PlaybackAccounting {
                boundary: boundary.clone(),
                statistics: statistics.playback,
            });
            storage_failed |= capture.is_err() || playback.is_err();
        }
        if storage_failed {
            self.fail(RuntimeFailureCode::Storage);
            Err(RuntimeFailureCode::Storage)
        } else {
            Ok(())
        }
    }

    async fn open_resources(&mut self) -> Result<Resources, RuntimeFailureCode> {
        let result = super::resources::open_resources(
            self.audio.clone(),
            self.plan.request.clone(),
            self.prepared.clone(),
            self.plan.direction,
            self.stop.clone(),
            |event| self.cleanup_event(event),
        )
        .await;
        match result {
            Ok(resources) => Ok(resources),
            Err(report) => {
                let code = report
                    .failures
                    .first()
                    .cloned()
                    .unwrap_or(RuntimeFailureCode::CleanupTimeout);
                let _ = self.record_cleanup(report);
                Err(code)
            }
        }
    }

    fn pause_route(&self) {
        let _ = self
            .router
            .pause_direction(self.route, "direction is not ready".into());
    }

    fn activate(&mut self, resources: &Resources) -> Result<(), RuntimeFailureCode> {
        if *self.stop.borrow() {
            return Err(RuntimeFailureCode::Interrupted);
        }
        if *resources.ports.health.borrow() != crate::audio::runtime::AudioHealth::Ready {
            return Err(RuntimeFailureCode::DeviceUnavailable);
        }
        resources.ports.capture.clear();
        for endpoint in &self.plan.endpoints {
            self.health
                .0
                .lock()
                .unwrap()
                .insert(endpoint.endpoint.id.clone(), resources.ports.health.clone());
        }
        self.router
            .replace_direction_output(self.route, resources.ports.playback.clone())
            .map_err(|_| RuntimeFailureCode::DeviceUnavailable)?;
        self.router
            .resume_direction(self.route)
            .map_err(|_| RuntimeFailureCode::DeviceUnavailable)?;
        self.transition_failures.clear();
        Ok(())
    }

    fn route_audio(
        &mut self,
        resources: &mut Resources,
        frame: &crate::audio::AudioFrame,
    ) -> Result<(), RuntimeFailureCode> {
        let frames = split_provider_audio(frame, &mut resources.output_sequence)
            .map_err(|_| RuntimeFailureCode::InvalidEvent)?;
        for frame in frames {
            if self
                .router
                .route_direction_admission(frame, self.route)
                .map_err(|_| RuntimeFailureCode::DeviceUnavailable)?
                == RouteAdmission::Dropped
            {
                return Err(RuntimeFailureCode::DeviceUnavailable);
            }
        }
        Ok(())
    }

    async fn await_started(&mut self, resources: &mut Resources) -> Result<(), RuntimeFailureCode> {
        let deadline = tokio::time::sleep(PROVIDER_READY_TIMEOUT);
        tokio::pin!(deadline);
        loop {
            if *self.stop.borrow() {
                return Err(RuntimeFailureCode::Interrupted);
            }
            tokio::select! {
                _ = &mut deadline => return Err(RuntimeFailureCode::StartupTimeout),
                _ = self.stop.changed() => if *self.stop.borrow() { return Err(RuntimeFailureCode::Interrupted); },
                changed = resources.ports.health.changed() => {
                    if changed.is_err() || *resources.ports.health.borrow() != crate::audio::runtime::AudioHealth::Ready {
                        return Err(RuntimeFailureCode::DeviceUnavailable);
                    }
                }
                event = resources.stream.next_event() => match event {
                    Ok(Some(ProviderEvent::SessionStarted { .. })) => return Ok(()),
                    Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                        self.fail_provider(&error, metadata.log_id.clone());
                        return Err(provider_failure(&error));
                    }
                    Ok(Some(event)) => self.handle_non_audio_event(&event)?,
                    Ok(None) => return Err(RuntimeFailureCode::Provider),
                    Err(error) => {
                        self.fail_provider(&error, None);
                        return Err(provider_failure(&error));
                    }
                }
            }
        }
    }

    async fn run_connected(&mut self, mut resources: Resources) -> ConnectedExit {
        let mut packetizer = AudioPacketizer::new(self.plan.request.source);
        let mut pending = VecDeque::with_capacity(PENDING_PACKET_CAPACITY);
        let mut next_send_at = None;
        let mut in_flight: Option<Admission> = None;
        let mut capture_baseline = resources.ports.control.statistics().capture;
        let mut playback_baseline = resources.ports.control.playback_overflow_generation();
        let mut diagnostic_tick = tokio::time::interval(Duration::from_secs(1));
        diagnostic_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        diagnostic_tick.tick().await;
        loop {
            if *self.stop.borrow() {
                return ConnectedExit::Stopped(
                    self.graceful_stop(resources, packetizer, pending, next_send_at, in_flight)
                        .await
                        .err(),
                );
            }
            tokio::select! {
                biased;
                _ = self.stop.changed() => if *self.stop.borrow() {
                    return ConnectedExit::Stopped(self.graceful_stop(resources, packetizer, pending, next_send_at, in_flight).await.err());
                },
                control = self.controls.recv() => if let Some(DirectionControl::Resume(result)) = control {
                    let _ = self.updates.send(ActorUpdate::ResumeComplete { direction: self.plan.direction, result, response: Err(AppError::InvalidRequest("direction is already running".into())) }).await;
                },
                changed = resources.ports.health.changed() => {
                    if changed.is_err() || *resources.ports.health.borrow() != crate::audio::runtime::AudioHealth::Ready {
                        self.diagnostics.observe_gap(GapCause::DeviceLoss, 0);
                        let _ = self.flush_diagnostics(&resources, true, None);
                        self.abort_resources(resources).await;
                        return ConnectedExit::Paused(RuntimeFailureCode::DeviceUnavailable);
                    }
                }
                _ = resources.ports.control.playback_overflow_changed(playback_baseline) => {
                    // Native samples are not normalized frames: preserve their
                    // units in accounting, and report the discontinuity with 0 frames.
                    self.diagnostics.observe_gap(GapCause::OutputOverflow, 0);
                    pending.clear(); packetizer.reset(); next_send_at = None; in_flight = None;
                    self.publish(DirectionPhase::Reconnecting, None).await;
                    match self.recover_same_stream(resources, false, true).await {
                        Ok(next) => {
                            resources = next;
                            capture_baseline = resources.ports.control.statistics().capture;
                            playback_baseline = resources.ports.control.playback_overflow_generation();
                            self.publish(DirectionPhase::Running, None).await;
                        }
                        Err(code) => return ConnectedExit::Paused(code),
                    }
                }
                _ = diagnostic_tick.tick() => {
                    let playback = resources.ports.control.statistics().playback;
                    if self.flush_diagnostics(&resources, false, Some((None, playback))).is_err() {
                        self.abort_resources(resources).await;
                        return ConnectedExit::Paused(RuntimeFailureCode::Storage);
                    }
                }
                _ = wait_for_pacing(next_send_at), if !pending.is_empty() && in_flight.is_none() => {
                    let packet = pending.pop_front().expect("pacing branch requires a packet");
                    in_flight = Some(admission(resources.stream.clone(), packet));
                }
                (result, duration, captured_at) = async { in_flight.as_mut().unwrap().await }, if in_flight.is_some() => {
                    in_flight = None;
                    let base = next_send_at.take().unwrap_or_else(Instant::now);
                    next_send_at = Some(std::cmp::max(base, Instant::now()) + duration);
                    match result {
                        Ok(()) => {
                            let capture_to_admission = SystemTime::now()
                                .duration_since(captured_at)
                                .ok()
                                .and_then(|duration| u64::try_from(duration.as_millis()).ok());
                            self.diagnostics.observe_latency(capture_to_admission, None);
                        }
                        Err(ProviderError::QueueFull) => {
                            self.diagnostics.observe_gap(GapCause::CaptureOverflow, 1);
                        }
                        Err(ProviderError::NotStarted | ProviderError::Closed) => {
                            pending.clear();
                            packetizer.reset();
                            next_send_at = None;
                            self.publish(DirectionPhase::Reconnecting, None).await;
                            in_flight = None;
                            match self.recover_same_stream(resources, false, false).await {
                                Ok(next) => {
                                    resources = next;
                                    capture_baseline = resources.ports.control.statistics().capture;
                                    playback_baseline = resources.ports.control.playback_overflow_generation();
                                    self.publish(DirectionPhase::Running, None).await;
                                }
                                Err(code) => return ConnectedExit::Paused(code),
                            }
                        }
                        Err(error) => {
                            let code = provider_failure(&error);
                            self.fail_provider(&error, None);
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(code);
                        }
                    }
                }
                event = resources.stream.next_event() => {
                    let received_at = Instant::now();
                    match event {
                        Ok(Some(ProviderEvent::SessionStarted { .. })) => {
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(RuntimeFailureCode::InvalidEvent);
                        }
                        Ok(Some(ProviderEvent::Gap { .. })) => {
                            if let Err(code) = self.advance_provider_epoch() {
                                self.abort_resources(resources).await;
                                return ConnectedExit::Paused(code);
                            }
                            self.publish(DirectionPhase::Reconnecting, None).await;
                            in_flight = None;
                            match self.recover_same_stream(resources, true, false).await {
                                Ok(next) => {
                                    resources = next;
                                    packetizer.reset();
                                    pending.clear();
                                    next_send_at = None;
                                    capture_baseline = resources.ports.control.statistics().capture;
                                    playback_baseline = resources.ports.control.playback_overflow_generation();
                                    self.publish(DirectionPhase::Running, None).await;
                                }
                                Err(code) => return ConnectedExit::Paused(code),
                            }
                        }
                        Ok(Some(ProviderEvent::Audio { frame, .. })) => {
                            if let Err(code) = self.route_audio(&mut resources, &frame) {
                                if code == RuntimeFailureCode::DeviceUnavailable {
                                    self.diagnostics.observe_gap(GapCause::OutputOverflow, 1);
                                    self.publish(DirectionPhase::Reconnecting, None).await;
                                    in_flight = None;
                                    match self.recover_same_stream(resources, false, true).await {
                                        Ok(next) => {
                                            resources = next;
                                            packetizer.reset();
                                            pending.clear();
                                            next_send_at = None;
                                            capture_baseline = resources.ports.control.statistics().capture;
                                            playback_baseline = resources.ports.control.playback_overflow_generation();
                                            self.publish(DirectionPhase::Running, None).await;
                                        }
                                        Err(code) => return ConnectedExit::Paused(code),
                                    }
                                } else {
                                    self.abort_resources(resources).await;
                                    return ConnectedExit::Paused(code);
                                }
                            } else {
                                self.diagnostics.observe_latency(
                                    None,
                                    Some(u64::try_from(received_at.elapsed().as_millis()).unwrap_or(u64::MAX)),
                                );
                            }
                        }
                        Ok(Some(ProviderEvent::Finished { .. })) => {
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(RuntimeFailureCode::Interrupted);
                        }
                        Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                            let code = provider_failure(&error);
                            self.fail_provider(&error, metadata.log_id.clone());
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(code);
                        }
                        Ok(Some(event)) => if let Err(code) = self.handle_non_audio_event(&event) {
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(code);
                        },
                        Ok(None) => {
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(RuntimeFailureCode::Provider);
                        }
                        Err(error) => {
                            let code = provider_failure(&error);
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(code);
                        }
                    }
                }
                frame = resources.ports.capture.recv() => {
                    let Some(frame) = frame else { continue; };
                    let capture = resources.ports.control.statistics().capture;
                    let raw_lost = capture.dropped_native_blocks.saturating_sub(capture_baseline.dropped_native_blocks);
                    let frames_lost = capture.dropped_normalized_frames.saturating_sub(capture_baseline.dropped_normalized_frames);
                    capture_baseline = capture;
                    if raw_lost != 0 || frames_lost != 0 {
                        self.diagnostics.observe_gap(GapCause::CaptureOverflow, frames_lost);
                        pending.clear();
                        packetizer.reset();
                        next_send_at = None;
                        self.publish(DirectionPhase::Reconnecting, None).await;
                        in_flight = None;
                        match self.recover_same_stream(resources, false, true).await {
                            Ok(next) => {
                                resources = next;
                                capture_baseline = resources.ports.control.statistics().capture;
                                playback_baseline = resources.ports.control.playback_overflow_generation();
                                self.publish(DirectionPhase::Running, None).await;
                                continue;
                            }
                            Err(code) => return ConnectedExit::Paused(code),
                        }
                    }
                    let packets = match packetizer.push(frame) {
                        Ok(packets) => packets,
                        Err(_) => {
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(RuntimeFailureCode::InvalidEvent);
                        }
                    };
                    if pending.len().saturating_add(packets.len()) > PENDING_PACKET_CAPACITY {
                        self.diagnostics.observe_gap(GapCause::CaptureOverflow, (pending.len() + packets.len()) as u64);
                        pending.clear(); packetizer.reset(); next_send_at = None; in_flight = None;
                        self.publish(DirectionPhase::Reconnecting, None).await;
                        match self.recover_same_stream(resources, false, true).await {
                            Ok(next) => {
                                resources = next;
                                capture_baseline = resources.ports.control.statistics().capture;
                                playback_baseline = resources.ports.control.playback_overflow_generation();
                                self.publish(DirectionPhase::Running, None).await;
                            }
                            Err(code) => return ConnectedExit::Paused(code),
                        }
                        continue;
                    }
                    pending.extend(packets);
                    if next_send_at.is_none() && !pending.is_empty() {
                        next_send_at = Some(Instant::now());
                    }
                }
            }
        }
    }

    async fn recover_same_stream(
        &mut self,
        resources: Resources,
        gap_seen: bool,
        provider_was_ready: bool,
    ) -> Result<Resources, RuntimeFailureCode> {
        self.pause_route();
        let stream = resources.stream;
        let output_sequence = resources.output_sequence;
        let mut operation = Some(AudioOperation::replace(
            self.audio.clone(),
            self.plan.request.clone(),
            resources.ports,
        ));
        let audio_deadline = operation.as_ref().unwrap().admission_deadline;
        let deadline = Instant::now() + PROVIDER_READY_TIMEOUT;
        let mut provider_ready = provider_was_ready;
        let mut awaiting_started = gap_seen;
        let mut opened = None;
        let failure = loop {
            if *self.stop.borrow() {
                break RuntimeFailureCode::Interrupted;
            }
            if provider_ready && opened.is_some() {
                let resources = Resources {
                    stream: stream.clone(),
                    ports: opened.take().unwrap(),
                    output_sequence,
                };
                match self.activate(&resources) {
                    Ok(()) => return Ok(resources),
                    Err(code) => {
                        opened = Some(resources.ports);
                        break code;
                    }
                }
            }
            tokio::select! {
                _ = self.stop.changed() => {},
                _ = tokio::time::sleep_until(deadline) => break RuntimeFailureCode::StartupTimeout,
                _ = tokio::time::sleep_until(audio_deadline), if operation.is_some() => break RuntimeFailureCode::StartupTimeout,
                acquired = async { operation.as_mut().unwrap().join().await }, if operation.is_some() => {
                    operation.take();
                    let failure = acquired.report.failures.first().cloned();
                    let persisted = self.record_cleanup(acquired.report);
                    opened = acquired.ports;
                    if let Some(code) = failure.or_else(|| persisted.err()) { break code; }
                    if opened.is_none() { break RuntimeFailureCode::CleanupTimeout; }
                }
                event = stream.next_event() => match event {
                    Ok(Some(ProviderEvent::Gap { .. })) => {
                        if let Err(code) = self.advance_provider_epoch() { break code; }
                        awaiting_started = true;
                        provider_ready = false;
                    }
                    Ok(Some(ProviderEvent::SessionStarted { .. })) => {
                        if !awaiting_started { break RuntimeFailureCode::InvalidEvent; }
                        awaiting_started = false;
                        provider_ready = true;
                    }
                    Ok(Some(ProviderEvent::Audio { .. })) => {},
                    Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                        self.fail_provider(&error, metadata.log_id.clone()); break provider_failure(&error);
                    }
                    Ok(Some(ProviderEvent::Finished { .. })) | Ok(None) => break RuntimeFailureCode::Provider,
                    Ok(Some(event)) => if let Err(code) = self.handle_non_audio_event(&event) { break code; },
                    Err(error) => { self.fail_provider(&error, None); break provider_failure(&error); }
                }
            }
        };
        let audio_cleanup = async {
            if let Some(operation) = operation {
                operation.cancel_and_settle().await
            } else if let Some(ports) = opened {
                close_audio(ports.control.clone(), true).await
            } else {
                CleanupReport::default()
            }
        };
        let (audio, provider) = tokio::join!(
            audio_cleanup,
            cancel_provider(stream, |event| self.cleanup_event(event))
        );
        let _ = self.record_cleanup(audio);
        let _ = self.record_cleanup(provider);
        for payload in self.diagnostics.take_due(Instant::now(), true) {
            if self.persist_diagnostic(payload).is_err() {
                self.fail(RuntimeFailureCode::Storage);
            }
        }
        Err(failure)
    }

    fn advance_provider_epoch(&mut self) -> Result<(), RuntimeFailureCode> {
        self.sink
            .advance_epoch()
            .map_err(|_| RuntimeFailureCode::Storage)?;
        self.epoch = self.sink.epoch();
        self.transition_failures.clear();
        self.diagnostics.observe_gap(GapCause::ProviderReconnect, 0);
        Ok(())
    }

    async fn graceful_stop(
        &mut self,
        mut resources: Resources,
        mut packetizer: AudioPacketizer,
        mut pending: VecDeque<crate::audio::AudioFrame>,
        mut next_send_at: Option<Instant>,
        mut in_flight: Option<Admission>,
    ) -> Result<(), RuntimeFailureCode> {
        let stop_result =
            tokio::time::timeout(CAPTURE_STOP_TIMEOUT, resources.ports.control.stop_capture())
                .await;
        let stop_failure = match stop_result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(audio_failure(&error)),
            Err(_) => Some(RuntimeFailureCode::CleanupTimeout),
        };
        if let Some(code) = stop_failure {
            self.cancel_and_close(&resources).await;
            return Err(code);
        }
        let drain_result = tokio::time::timeout(CAPTURE_DRAIN_TIMEOUT, async {
            while let Some(frame) = resources.ports.capture.pop().await {
                pending.extend(
                    packetizer
                        .push(frame)
                        .map_err(|_| RuntimeFailureCode::InvalidEvent)?,
                );
            }
            if let Some(tail) = packetizer
                .flush()
                .map_err(|_| RuntimeFailureCode::InvalidEvent)?
            {
                pending.push_back(tail);
            }
            loop {
                if in_flight.is_none() {
                    let Some(packet) = pending.pop_front() else {
                        break;
                    };
                    let stream = resources.stream.clone();
                    let send_at = next_send_at;
                    // One future owns both its pacing wait and its admission; event
                    // delivery never recreates or replays it.
                    in_flight = Some(Box::pin(async move {
                        wait_for_pacing(send_at).await;
                        admission(stream, packet).await
                    }));
                }
                let duration = self
                    .await_stop_admission(&mut resources, in_flight.as_mut().unwrap())
                    .await?;
                in_flight = None;
                next_send_at = Some(Instant::now() + duration);
            }
            Ok::<(), RuntimeFailureCode>(())
        })
        .await;
        match drain_result {
            Ok(Ok(())) => {}
            Ok(Err(code)) => {
                self.cancel_and_close(&resources).await;
                return Err(code);
            }
            Err(_) => {
                let dropped = pending.len()
                    + usize::from(in_flight.is_some())
                    + resources.ports.capture.clear();
                self.diagnostics
                    .observe_gap(GapCause::CaptureOverflow, dropped.max(1) as u64);
                // Drop the owned admission before cancellation; it is never retried.
                drop(in_flight);
                self.fail(RuntimeFailureCode::Interrupted);
                self.cancel_and_close(&resources).await;
                return Err(RuntimeFailureCode::Interrupted);
            }
        }
        let stream = resources.stream.clone();
        let finish = stream.finish();
        tokio::pin!(finish);
        let terminal_result = tokio::time::timeout(PROVIDER_FINISH_TIMEOUT, async {
            let mut finish_done = false;
            let mut terminal_done = false;
            loop {
                if finish_done && terminal_done { return Ok(()); }
                tokio::select! {
                    result = &mut finish, if !finish_done => {
                        finish_done = true;
                        if let Err(error) = result { self.fail_provider(&error, None); return Err(provider_failure(&error)); }
                    }
                    event = resources.stream.next_event(), if !terminal_done => {
                        terminal_done = self.stop_event(&mut resources, event)?;
                    }
                }
            }
        }).await;
        match terminal_result {
            Ok(Ok(())) => {}
            Ok(Err(code)) => {
                self.cancel_and_close(&resources).await;
                return Err(code);
            }
            Err(_) => {
                self.cancel_and_close(&resources).await;
                return Err(RuntimeFailureCode::FinishTimeout);
            }
        }
        let playback_result = tokio::time::timeout(
            PLAYBACK_DRAIN_TIMEOUT,
            resources.ports.control.drain_playback(),
        )
        .await;
        let playback = match playback_result {
            Ok(Ok(playback)) => playback,
            Ok(Err(error)) => {
                self.fail(audio_failure(&error));
                self.cancel_and_close(&resources).await;
                return Err(RuntimeFailureCode::PlaybackTimeout);
            }
            Err(_) => {
                self.cancel_and_close(&resources).await;
                return Err(RuntimeFailureCode::PlaybackTimeout);
            }
        };
        let report = close_audio(resources.ports.control.clone(), false).await;
        self.pause_route();
        let failure = report.failures.first().cloned();
        let boundary = failure.is_none().then_some(playback.boundary);
        let accounting = self.record_cleanup_with_boundary(report, boundary);
        // Attempt pending measured diagnostics even when finalized accounting fails.
        let diagnostics = self.flush_diagnostics(&resources, true, None);
        accounting?;
        diagnostics?;
        match failure {
            Some(code) => Err(code),
            None => Ok(()),
        }
    }

    async fn abort_resources(&mut self, resources: Resources) {
        self.cancel_and_close(&resources).await;
    }

    async fn await_stop_admission(
        &mut self,
        resources: &mut Resources,
        operation: &mut Admission,
    ) -> Result<Duration, RuntimeFailureCode> {
        loop {
            tokio::select! {
                (result, duration, captured_at) = &mut *operation => {
                    match result {
                        Ok(()) => self.diagnostics.observe_latency(SystemTime::now().duration_since(captured_at).ok().and_then(|d| u64::try_from(d.as_millis()).ok()), None),
                        Err(ProviderError::QueueFull) => self.diagnostics.observe_gap(GapCause::CaptureOverflow, 1),
                        Err(error) => { self.fail_provider(&error, None); return Err(provider_failure(&error)); }
                    }
                    return Ok(duration);
                }
                event = resources.stream.next_event() => {
                    if self.stop_event(resources, event)? { return Err(RuntimeFailureCode::Provider); }
                }
            }
        }
    }

    fn stop_event(
        &mut self,
        resources: &mut Resources,
        event: crate::providers::ProviderResult<Option<ProviderEvent>>,
    ) -> Result<bool, RuntimeFailureCode> {
        match event {
            Ok(Some(ProviderEvent::Audio { frame, .. })) => self.route_audio(resources, &frame)?,
            Ok(Some(ProviderEvent::Finished { .. })) => return Ok(true),
            Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                self.fail_provider(&error, metadata.log_id.clone());
                return Err(provider_failure(&error));
            }
            Ok(Some(event)) => self.handle_non_audio_event(&event)?,
            Ok(None) => return Err(RuntimeFailureCode::Provider),
            Err(error) => {
                self.fail_provider(&error, None);
                return Err(provider_failure(&error));
            }
        }
        Ok(false)
    }

    async fn cancel_and_close(&mut self, resources: &Resources) {
        self.pause_route();
        resources.ports.control.suppress_now();
        let (audio, provider) = tokio::join!(
            close_audio(resources.ports.control.clone(), true),
            cancel_provider(resources.stream.clone(), |event| self.cleanup_event(event))
        );
        let _ = self.record_cleanup(audio);
        let _ = self.record_cleanup(provider);
        if self.flush_diagnostics(resources, true, None).is_err() {
            self.fail(RuntimeFailureCode::Storage);
        }
    }

    async fn resume(&mut self) -> AppResult<Resources> {
        if *self.stop.borrow() {
            return Err(runtime_error(RuntimeFailureCode::Interrupted));
        }
        let inventory = self.audio.inventory().await?;
        validate_request(&inventory, &self.plan)?;
        self.prepared
            .revalidate()
            .map_err(|error| runtime_error(provider_failure(&error)))?;
        self.plan.request.inventory_generation = inventory.generation;
        self.sink.advance_epoch()?;
        self.epoch = self.sink.epoch();
        self.transition_failures.clear();
        self.diagnostics.observe_gap(GapCause::ManualResume, 0);
        self.publish(DirectionPhase::Starting, None).await;
        let mut resources = self.open_resources().await.map_err(runtime_error)?;
        let result = self
            .await_started(&mut resources)
            .await
            .and_then(|()| self.activate(&resources));
        if let Err(code) = result {
            self.abort_resources(resources).await;
            return Err(runtime_error(code));
        }
        Ok(resources)
    }

    fn handle_non_audio_event(&mut self, event: &ProviderEvent) -> Result<(), RuntimeFailureCode> {
        match event {
            ProviderEvent::SourceInterim { .. }
            | ProviderEvent::SourceFinal { .. }
            | ProviderEvent::TranslationInterim { .. }
            | ProviderEvent::TranslationFinal { .. } => {
                self.sink.accept(event).map_err(|error| match error {
                    AppError::InvalidRequest(_) => RuntimeFailureCode::InvalidEvent,
                    _ => RuntimeFailureCode::Storage,
                })?;
                self.diagnostics
                    .observe_transcript_overflow(self.sink.take_dropped_interims());
                Ok(())
            }
            ProviderEvent::Muted { .. } => Ok(()),
            _ => Err(RuntimeFailureCode::InvalidEvent),
        }
    }

    fn fail(&mut self, code: RuntimeFailureCode) {
        if self.transition_failures.contains(&code) {
            return;
        }
        self.transition_failures.push(code.clone());
        if !self.failures.contains(&code) {
            self.failures.push(code.clone());
        }
        self.persist_failure(code, None, None);
    }

    fn fail_provider(&mut self, error: &ProviderError, log_id: Option<String>) {
        let code = provider_failure(error);
        if self.transition_failures.contains(&code) {
            return;
        }
        self.transition_failures.push(code.clone());
        if !self.failures.contains(&code) {
            self.failures.push(code.clone());
        }
        let provider_code = match error {
            ProviderError::RemoteFailure { code } => Some(*code),
            _ => None,
        };
        self.persist_failure(code, provider_code, log_id);
    }

    fn persist_failure(
        &mut self,
        code: RuntimeFailureCode,
        provider_code: Option<i32>,
        log_id: Option<String>,
    ) {
        let diagnostic = RuntimeDiagnostic {
            id: Uuid::new_v4(),
            session_id: self.session_id,
            direction: Some(self.plan.direction),
            stream_epoch: Some(self.epoch),
            occurred_at_ms: now_ms(),
            payload: DiagnosticPayload::Failure {
                code: code.clone(),
                provider_code,
                log_id,
            },
        };
        if self.store.append_diagnostic(&diagnostic).is_ok() {
            let _ = self
                .bus
                .publish(self.session_id, SessionEvent::Diagnostic { diagnostic });
        } else {
            if !self.failures.contains(&RuntimeFailureCode::Storage) {
                self.failures.push(RuntimeFailureCode::Storage);
            }
            let local = RuntimeDiagnostic {
                id: Uuid::new_v4(),
                session_id: self.session_id,
                direction: Some(self.plan.direction),
                stream_epoch: Some(self.epoch),
                occurred_at_ms: now_ms(),
                payload: DiagnosticPayload::Failure {
                    code: RuntimeFailureCode::Storage,
                    provider_code: None,
                    log_id: None,
                },
            };
            let _ = self.bus.publish(
                self.session_id,
                SessionEvent::Diagnostic { diagnostic: local },
            );
        }
    }

    fn flush_diagnostics(
        &mut self,
        resources: &Resources,
        force: bool,
        playback: Option<(
            Option<PlaybackBoundary>,
            crate::domain::transcript::PlaybackStatistics,
        )>,
    ) -> Result<(), RuntimeFailureCode> {
        for payload in self.diagnostics.take_due(Instant::now(), force) {
            self.persist_diagnostic(payload)?;
        }
        let statistics = resources.ports.control.statistics();
        self.persist_diagnostic(DiagnosticPayload::CaptureAccounting {
            statistics: statistics.capture,
        })?;
        if let Some((boundary, statistics)) = playback {
            self.persist_diagnostic(DiagnosticPayload::PlaybackAccounting {
                boundary,
                statistics,
            })?;
        }
        Ok(())
    }

    fn persist_diagnostic(&self, payload: DiagnosticPayload) -> Result<(), RuntimeFailureCode> {
        let diagnostic = RuntimeDiagnostic {
            id: Uuid::new_v4(),
            session_id: self.session_id,
            direction: Some(self.plan.direction),
            stream_epoch: Some(self.epoch),
            occurred_at_ms: now_ms(),
            payload,
        };
        self.store
            .append_diagnostic(&diagnostic)
            .map_err(|_| RuntimeFailureCode::Storage)?;
        let _ = self
            .bus
            .publish(self.session_id, SessionEvent::Diagnostic { diagnostic });
        Ok(())
    }

    async fn publish(&self, phase: DirectionPhase, failure: Option<RuntimeFailureCode>) {
        let (applied, observed) = tokio::sync::oneshot::channel();
        if self
            .updates
            .send(ActorUpdate::Snapshot {
                direction: DirectionSnapshot {
                    direction: self.plan.direction,
                    phase,
                    stream_epoch: self.epoch,
                    failure,
                },
                applied,
            })
            .await
            .is_ok()
        {
            // Apply the snapshot before a following resource transition or
            // transcript write can become visible from the blocking actor.
            let _ = observed.await;
        }
    }
}

enum ConnectedExit {
    Stopped(Option<RuntimeFailureCode>),
    Paused(RuntimeFailureCode),
}
pub(crate) const MAX_PROVIDER_OUTPUT_SAMPLES: usize = 48_000;
pub(crate) const PLAYBACK_FRAME_SAMPLES: usize = 1_920;

pub(crate) struct DiagnosticAccumulator {
    last_flush: Instant,
    latency: Option<(Option<u64>, Option<u64>)>,
    gaps: Vec<(GapCause, u64)>,
    transcript_overflow: u64,
}

impl DiagnosticAccumulator {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_flush: now,
            latency: None,
            gaps: Vec::new(),
            transcript_overflow: 0,
        }
    }

    pub(crate) fn observe_latency(
        &mut self,
        capture_to_admission_ms: Option<u64>,
        response_to_enqueue_ms: Option<u64>,
    ) {
        let (capture, response) = self.latency.unwrap_or((None, None));
        self.latency = Some((
            capture_to_admission_ms.or(capture),
            response_to_enqueue_ms.or(response),
        ));
    }

    pub(crate) fn observe_gap(&mut self, cause: GapCause, dropped_frames: u64) {
        if let Some((_, count)) = self.gaps.iter_mut().find(|(current, _)| *current == cause) {
            *count = count.saturating_add(dropped_frames);
        } else {
            self.gaps.push((cause, dropped_frames));
        }
    }

    pub(crate) fn observe_transcript_overflow(&mut self, dropped_interims: u64) {
        self.transcript_overflow = self.transcript_overflow.saturating_add(dropped_interims);
    }

    pub(crate) fn take_due(&mut self, now: Instant, force: bool) -> Vec<DiagnosticPayload> {
        if !force && now.duration_since(self.last_flush) < Duration::from_secs(1) {
            return Vec::new();
        }
        self.last_flush = now;
        let mut payloads = Vec::new();
        if let Some((capture_to_send_ms, response_to_enqueue_ms)) = self.latency.take() {
            payloads.push(DiagnosticPayload::Latency {
                // This field ends at provider queue admission, not a wire send.
                capture_to_send_ms,
                response_to_enqueue_ms,
                enqueue_to_callback_ms: None,
                provider_processing_ms: None,
            });
        }
        payloads.extend(self.gaps.drain(..).map(|(cause, dropped_frames)| {
            DiagnosticPayload::Gap {
                cause,
                dropped_frames,
            }
        }));
        if self.transcript_overflow != 0 {
            payloads.push(DiagnosticPayload::TranscriptOverflow {
                dropped_interims: std::mem::take(&mut self.transcript_overflow),
            });
        }
        payloads
    }
}

pub(crate) fn router_direction(
    mode: SessionMode,
    direction: SessionDirection,
) -> AppResult<RouterDirection> {
    match (mode, direction) {
        (SessionMode::Live, SessionDirection::SourceToTarget)
        | (SessionMode::Live, SessionDirection::TargetToSource) => Ok(RouterDirection::Live),
        (SessionMode::Call, SessionDirection::SourceToTarget) => Ok(RouterDirection::AtoB),
        (SessionMode::Call, SessionDirection::TargetToSource) => Ok(RouterDirection::BtoA),
        (_, SessionDirection::Bidirectional) => Err(AppError::InvalidRequest(
            "runtime directions must be unidirectional".into(),
        )),
    }
}

pub(crate) fn provider_failure(error: &ProviderError) -> RuntimeFailureCode {
    match error {
        ProviderError::Timeout => RuntimeFailureCode::StartupTimeout,
        ProviderError::UnsupportedAudioFormat
        | ProviderError::InvalidAudioFrame
        | ProviderError::InvalidDirection
        | ProviderError::InvalidConfiguration
        | ProviderError::Protocol => RuntimeFailureCode::InvalidEvent,
        ProviderError::Cancelled => RuntimeFailureCode::Interrupted,
        _ => RuntimeFailureCode::Provider,
    }
}

pub(crate) fn split_provider_audio(
    frame: &AudioFrame,
    next_sequence: &mut u64,
) -> AppResult<Vec<AudioFrame>> {
    if frame.spec() != PcmSpec::pcm16_mono(PLAYBACK_SAMPLE_RATE_HZ)
        || frame.samples().len() > MAX_PROVIDER_OUTPUT_SAMPLES
    {
        return Err(AppError::InvalidRequest(
            "invalid provider playback event".into(),
        ));
    }
    let mut result = Vec::with_capacity(frame.samples().len().div_ceil(PLAYBACK_FRAME_SAMPLES));
    for (index, samples) in frame.samples().chunks(PLAYBACK_FRAME_SAMPLES).enumerate() {
        let sequence = *next_sequence;
        *next_sequence = next_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::InvalidRequest("playback sequence overflow".into()))?;
        let sample_offset = index
            .checked_mul(PLAYBACK_FRAME_SAMPLES)
            .ok_or_else(|| AppError::InvalidRequest("playback offset overflow".into()))?;
        let captured_at = frame
            .captured_at()
            .checked_add(Duration::from_nanos(
                u64::try_from(sample_offset)
                    .map_err(|_| AppError::InvalidRequest("playback offset overflow".into()))?
                    .saturating_mul(1_000_000_000)
                    / u64::from(PLAYBACK_SAMPLE_RATE_HZ),
            ))
            .ok_or_else(|| AppError::InvalidRequest("playback timestamp overflow".into()))?;
        result.push(AudioFrame::at(
            frame.source(),
            sequence,
            captured_at,
            frame.spec(),
            samples.to_vec(),
        )?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        audio::AudioSource,
        transcript::{DiagnosticPayload, GapCause},
    };
    use tokio::time::Instant;

    #[test]
    fn splits_maximum_provider_audio_without_truncation() {
        let samples = (0..MAX_PROVIDER_OUTPUT_SAMPLES)
            .map(|sample| sample as i16)
            .collect::<Vec<_>>();
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            99,
            PcmSpec::pcm16_mono(24_000),
            samples.clone(),
        )
        .unwrap();
        let mut sequence = 7;
        let split = split_provider_audio(&frame, &mut sequence).unwrap();
        assert!(split.iter().all(|frame| frame.samples().len() <= 1_920));
        assert_eq!(
            split
                .iter()
                .flat_map(|frame| frame.samples().iter().copied())
                .collect::<Vec<_>>(),
            samples
        );
        assert_eq!(split.first().unwrap().sequence(), 7);
        assert_eq!(sequence, 32);
    }

    #[test]
    fn rejects_audio_larger_than_the_provider_envelope() {
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![0; MAX_PROVIDER_OUTPUT_SAMPLES + 1],
        )
        .unwrap();
        assert!(split_provider_audio(&frame, &mut 0).is_err());
    }

    #[test]
    fn diagnostics_coalesce_by_category_and_force_flush_pending_values() {
        let start = Instant::now();
        let mut diagnostics = DiagnosticAccumulator::new(start);
        diagnostics.observe_latency(Some(10), Some(20));
        diagnostics.observe_latency(Some(30), Some(40));
        diagnostics.observe_gap(GapCause::CaptureOverflow, 2);
        diagnostics.observe_gap(GapCause::CaptureOverflow, 3);
        diagnostics.observe_transcript_overflow(4);

        assert!(
            diagnostics
                .take_due(start + Duration::from_millis(999), false)
                .is_empty()
        );
        let due = diagnostics.take_due(start + Duration::from_secs(1), false);
        assert_eq!(due.len(), 3);
        assert!(due.iter().any(|payload| matches!(
            payload,
            DiagnosticPayload::Latency {
                capture_to_send_ms: Some(30),
                response_to_enqueue_ms: Some(40),
                ..
            }
        )));
        assert!(due.iter().any(|payload| matches!(
            payload,
            DiagnosticPayload::Gap {
                cause: GapCause::CaptureOverflow,
                dropped_frames: 5
            }
        )));
        assert!(due.iter().any(|payload| matches!(
            payload,
            DiagnosticPayload::TranscriptOverflow {
                dropped_interims: 4
            }
        )));

        diagnostics.observe_gap(GapCause::OutputOverflow, 1);
        assert!(
            diagnostics
                .take_due(start + Duration::from_millis(1_500), false)
                .is_empty()
        );
        assert!(matches!(
            diagnostics
                .take_due(start + Duration::from_millis(1_500), true)
                .as_slice(),
            [DiagnosticPayload::Gap {
                cause: GapCause::OutputOverflow,
                dropped_frames: 1
            }]
        ));
    }
}

async fn wait_for_pacing(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    }
}

#[cfg(test)]
mod rereview_regressions {
    use super::*;
    use crate::{
        audio::graph::AudioGraph,
        config::SecretStore,
        domain::audio::AudioSource,
        providers::{
            factory::ProviderFactory,
            fixture::{FixtureScript, FixtureStep},
        },
        session::test_support::{RuntimeHarness, metadata},
    };

    #[tokio::test]
    async fn rereview_lagged_router_events_must_not_hide_current_overflow() {
        let script = FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap();
        let h = RuntimeHarness::call(script.clone(), script).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let factory = ProviderFactory::new(Arc::new(SecretStore::for_tests(dir.path()).unwrap()))
            .with_test_provider(h.provider.clone());
        let prepared = factory
            .prepare(
                &h.saved,
                h.id,
                &h.launch.config,
                PcmSpec::pcm16_mono(16_000),
                PcmSpec::pcm16_mono(24_000),
            )
            .unwrap();
        let config = prepared.configs()[0].clone();
        let inventory = h.audio.inventory().await.unwrap();
        let find = |id: &str| {
            inventory
                .endpoints
                .iter()
                .find(|e| e.endpoint.id == id)
                .unwrap()
                .clone()
        };
        let a = find("a-input");
        let m2 = find("m2-capture");
        let m1 = find("m1-render");
        let physical = find("physical-output");
        let graph = AudioGraph::for_call(
            a.endpoint.clone(),
            m2.endpoint.clone(),
            m1.endpoint.clone(),
            physical.endpoint.clone(),
        )
        .unwrap();
        let health = RuntimeRouteHealth::default();
        let router = Arc::new(AudioRouter::paused_unbound(graph, health.clone()));
        let mut notifications = router.subscribe();
        let (_stop_tx, stop) = watch::channel(false);
        let (_control_tx, controls) = mpsc::channel(1);
        let (updates, _update_rx) = mpsc::channel(32);
        let mut actor = DirectionActor {
            session_id: h.id,
            plan: DirectionPlan {
                direction: SessionDirection::SourceToTarget,
                request: crate::audio::runtime::AudioOpenRequest {
                    inventory_generation: inventory.generation,
                    input_id: "a-input".into(),
                    output_id: "m1-render".into(),
                    source: AudioSource::Microphone,
                },
                endpoints: vec![a, m1],
            },
            sink: TranscriptSink::new(
                h.store.clone(),
                h.bus.clone(),
                config,
                prepared.provider_id(),
            )
            .unwrap(),
            prepared,
            audio: h.audio.clone(),
            store: h.store.clone(),
            bus: h.bus.clone(),
            stop,
            controls,
            updates,
            epoch: 0,
            failures: vec![],
            transition_failures: vec![],
            diagnostics: DiagnosticAccumulator::new(Instant::now()),
            router: router.clone(),
            route: RouterDirection::AtoB,
            health,
        };
        let peer_ports = h
            .audio
            .open(crate::audio::runtime::AudioOpenRequest {
                inventory_generation: inventory.generation,
                input_id: "m2-capture".into(),
                output_id: "physical-output".into(),
                source: AudioSource::System,
            })
            .await
            .unwrap();
        for id in ["m2-capture", "physical-output"] {
            actor
                .health
                .0
                .lock()
                .unwrap()
                .insert(id.into(), peer_ports.health.clone());
        }
        router
            .replace_direction_output(RouterDirection::BtoA, peer_ports.playback.clone())
            .unwrap();
        router.resume_direction(RouterDirection::BtoA).unwrap();
        let mut resources = actor.open_resources().await.unwrap();
        actor.await_started(&mut resources).await.unwrap();
        // A deterministic bounded destination remains full until this test consumes it.
        let (output, mut receiver) = mpsc::channel(1);
        resources.ports.playback = output;
        actor.activate(&resources).unwrap();
        let frame = AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![7; 1920],
        )
        .unwrap();
        actor.route_audio(&mut resources, &frame).unwrap();
        for index in 0..40 {
            router
                .pause_direction(RouterDirection::BtoA, format!("peer transition {index}"))
                .unwrap();
            router.resume_direction(RouterDirection::BtoA).unwrap();
        }
        assert!(matches!(
            notifications.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
        ));
        let result = actor.route_audio(&mut resources, &frame);
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err());
        router
            .route_direction(frame.clone(), RouterDirection::BtoA)
            .unwrap();
        let resources = actor
            .recover_same_stream(resources, false, true)
            .await
            .unwrap();
        assert_eq!(
            h.audio.total_opens(),
            3,
            "only the overflowing direction may reopen"
        );
        assert_eq!(h.provider.connect_counts(), [1, 0]);
        assert_eq!(
            router.direction_status(RouterDirection::BtoA).unwrap(),
            crate::audio::router::AudioRouterStatus::Running
        );
        assert_eq!(
            h.audio
                .next_playback("physical-output")
                .await
                .unwrap()
                .samples(),
            &[7; 1920]
        );
        actor.abort_resources(resources).await;
        let peer_closed = close_audio(peer_ports.control.clone(), true).await;
        assert!(peer_closed.failures.is_empty());
        assert_eq!(h.audio.open_ports(), 0);
        assert_eq!(
            result,
            Err(RuntimeFailureCode::DeviceUnavailable),
            "lagged broadcast cursor hid the current Full result, so the actor skipped replacement"
        );
    }
}
