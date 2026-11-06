use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::{
    sync::{mpsc, oneshot, watch},
    task::{JoinHandle, JoinSet},
    time::{Instant, MissedTickBehavior},
};
use uuid::Uuid;

use crate::{
    audio::{
        PcmSpec,
        graph::AudioGraph,
        packetizer::AudioPacketizer,
        platform::inventory::{AudioInventory, EndpointRole, RuntimeEndpoint},
        platform::{AudioDirection, DriverStatus},
        runtime::{AudioBackend, AudioOpenRequest, AudioPorts},
    },
    config::ProviderConfig,
    domain::{
        audio::{AudioSource, PLAYBACK_SAMPLE_RATE_HZ, SAMPLE_RATE_HZ},
        launch::{
            DeviceSelection, DirectionPhase, DirectionSnapshot, RuntimeOutcome, RuntimePhase,
            RuntimeSnapshot, SessionContext, SessionLaunch,
        },
        session::{SessionDirection, SessionMode},
        transcript::{
            DiagnosticPayload, GapCause, PlaybackBoundary, RuntimeDiagnostic, RuntimeFailureCode,
        },
    },
    error::{AppError, AppResult},
    events::{EventBus, SessionEvent},
    providers::{
        ProviderConnection, ProviderError, ProviderEvent,
        factory::{DirectedProviderStream, PreparedProviderSession, ProviderFactory},
    },
    storage::LocalStore,
};

use super::{
    direction::{DiagnosticAccumulator, provider_failure, router_direction, split_provider_audio},
    transcript_sink::TranscriptSink,
};

const AUDIO_OPEN_TIMEOUT: Duration = Duration::from_secs(3);
const PROVIDER_READY_TIMEOUT: Duration = Duration::from_secs(10);
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
#[cfg(not(test))]
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const CLOSE_TIMEOUT: Duration = Duration::from_millis(400);
const LATE_AUDIO_CLEANUP_TIMEOUT: Duration = Duration::from_secs(3);
const PENDING_PACKET_CAPACITY: usize = 16;

pub struct TranslationRuntime {
    factory: Arc<ProviderFactory>,
    audio: Arc<dyn AudioBackend>,
    store: Arc<LocalStore>,
    bus: EventBus,
}

pub struct PreparedRuntime {
    session_id: Uuid,
    context: SessionContext,
    provider: PreparedProviderSession,
    directions: Vec<DirectionPlan>,
}

#[derive(Clone)]
struct DirectionPlan {
    direction: SessionDirection,
    request: AudioOpenRequest,
}

#[derive(Clone)]
pub struct RunningSession {
    stop: watch::Sender<bool>,
    resume: mpsc::Sender<ResumeRequest>,
    status: watch::Receiver<RuntimeSnapshot>,
    completion: watch::Receiver<Option<RuntimeOutcome>>,
}

struct ResumeRequest {
    direction: SessionDirection,
    result: oneshot::Sender<AppResult<()>>,
}

enum DirectionControl {
    Resume(oneshot::Sender<AppResult<()>>),
}

enum ActorUpdate {
    Snapshot(DirectionSnapshot),
}

struct ActorExit {
    direction: SessionDirection,
    failure: Option<RuntimeFailureCode>,
}

struct DirectionActor {
    session_id: Uuid,
    plan: DirectionPlan,
    prepared: PreparedProviderSession,
    audio: Arc<dyn AudioBackend>,
    store: Arc<LocalStore>,
    bus: EventBus,
    stop: watch::Receiver<bool>,
    controls: mpsc::Receiver<DirectionControl>,
    updates: mpsc::Sender<ActorUpdate>,
    sink: TranscriptSink,
    epoch: u64,
    failures: Vec<RuntimeFailureCode>,
    diagnostics: DiagnosticAccumulator,
}

impl TranslationRuntime {
    pub fn new(
        factory: Arc<ProviderFactory>,
        audio: Arc<dyn AudioBackend>,
        store: Arc<LocalStore>,
        bus: EventBus,
    ) -> Self {
        Self {
            factory,
            audio,
            store,
            bus,
        }
    }

    pub async fn prepare(
        &self,
        session_id: Uuid,
        launch: &SessionLaunch,
        saved: &ProviderConfig,
    ) -> AppResult<PreparedRuntime> {
        if saved.id() != launch.provider_config_id {
            return Err(AppError::InvalidRequest(
                "provider configuration does not match launch".into(),
            ));
        }
        let inventory = self.audio.inventory().await?;
        if inventory.driver != DriverStatus::Ready {
            return Err(AppError::Audio {
                code: RuntimeFailureCode::AudioBackendUnavailable,
                message: "audio driver is not ready",
            });
        }
        let (endpoints, directions) = prepare_audio(&inventory, launch)?;
        let provider = self
            .factory
            .prepare(
                saved,
                session_id,
                &launch.config,
                PcmSpec::pcm16_mono(SAMPLE_RATE_HZ),
                PcmSpec::pcm16_mono(PLAYBACK_SAMPLE_RATE_HZ),
            )
            .map_err(provider_app_error)?;
        let provider_id = provider.provider_id();
        Ok(PreparedRuntime {
            session_id,
            context: SessionContext {
                launch: launch.clone(),
                provider: provider_id.clone(),
                provider_name: provider_id.as_str().to_owned(),
                endpoints,
            },
            provider,
            directions,
        })
    }

    pub fn start(&self, prepared: PreparedRuntime) -> RunningSession {
        let directions = prepared
            .directions
            .iter()
            .map(|plan| DirectionSnapshot {
                direction: plan.direction,
                phase: DirectionPhase::Starting,
                stream_epoch: 0,
                failure: None,
            })
            .collect::<Vec<_>>();
        let (status_tx, status) = watch::channel(RuntimeSnapshot {
            phase: RuntimePhase::Starting,
            directions,
            outcome: None,
        });
        let (completion_tx, completion) = watch::channel(None);
        let (stop, stop_rx) = watch::channel(false);
        let (resume, resume_rx) = mpsc::channel(4);
        let audio = self.audio.clone();
        let store = self.store.clone();
        let bus = self.bus.clone();
        tokio::spawn(supervise(
            prepared,
            audio,
            store,
            bus,
            stop.clone(),
            stop_rx,
            resume_rx,
            status_tx,
            completion_tx,
        ));
        RunningSession {
            stop,
            resume,
            status,
            completion,
        }
    }
}

impl PreparedRuntime {
    pub fn context(&self) -> &SessionContext {
        &self.context
    }
}

impl RunningSession {
    pub fn status(&self) -> watch::Receiver<RuntimeSnapshot> {
        self.status.clone()
    }

    pub async fn stop(&self) -> RuntimeOutcome {
        self.stop.send_replace(true);
        let mut completion = self.completion.clone();
        loop {
            if let Some(outcome) = completion.borrow_and_update().clone() {
                return outcome;
            }
            if completion.changed().await.is_err() {
                return RuntimeOutcome::Failed {
                    codes: vec![RuntimeFailureCode::CleanupTimeout],
                };
            }
        }
    }

    pub async fn resume_direction(&self, direction: SessionDirection) -> AppResult<()> {
        if direction == SessionDirection::Bidirectional {
            return Err(AppError::InvalidRequest(
                "resume requires one direction".into(),
            ));
        }
        let (result, receive) = oneshot::channel();
        self.resume
            .send(ResumeRequest { direction, result })
            .await
            .map_err(|_| AppError::InvalidRequest("runtime is stopping".into()))?;
        receive
            .await
            .map_err(|_| AppError::Internal("resume response was lost".into()))?
    }
}

#[allow(clippy::too_many_arguments)]
async fn supervise(
    prepared: PreparedRuntime,
    audio: Arc<dyn AudioBackend>,
    store: Arc<LocalStore>,
    bus: EventBus,
    stop_tx: watch::Sender<bool>,
    mut stop: watch::Receiver<bool>,
    mut resumes: mpsc::Receiver<ResumeRequest>,
    status: watch::Sender<RuntimeSnapshot>,
    completion: watch::Sender<Option<RuntimeOutcome>>,
) {
    let (updates_tx, mut updates) = mpsc::channel(32);
    let mut controls = BTreeMap::new();
    let mut tasks = JoinSet::new();
    let provider_id = prepared.provider.provider_id();
    for plan in prepared.directions {
        let (control_tx, controls_rx) = mpsc::channel(1);
        controls.insert(direction_key(plan.direction), control_tx);
        let config = prepared
            .provider
            .configs()
            .iter()
            .find(|config| config.direction == plan.direction)
            .expect("prepared direction config")
            .clone();
        let sink =
            match TranscriptSink::new(store.clone(), bus.clone(), config, provider_id.clone()) {
                Ok(sink) => sink,
                Err(_) => {
                    stop_tx.send_replace(true);
                    continue;
                }
            };
        let actor = DirectionActor {
            session_id: prepared.session_id,
            plan,
            prepared: prepared.provider.clone(),
            audio: audio.clone(),
            store: store.clone(),
            bus: bus.clone(),
            stop: stop.clone(),
            controls: controls_rx,
            updates: updates_tx.clone(),
            sink,
            epoch: 0,
            failures: Vec::new(),
            diagnostics: DiagnosticAccumulator::new(Instant::now()),
        };
        tasks.spawn(actor.run());
    }
    drop(updates_tx);
    let mut remaining = tasks.len();
    let mut failures = Vec::new();
    let mut stopping = false;
    while remaining > 0 {
        tokio::select! {
            _ = stop.changed(), if !stopping => {
                if *stop.borrow() {
                    stopping = true;
                    status.send_modify(|snapshot| snapshot.phase = RuntimePhase::Stopping);
                }
            }
            Some(request) = resumes.recv(), if !stopping => {
                match controls.get(&direction_key(request.direction)) {
                    Some(control) => {
                        if control.send(DirectionControl::Resume(request.result)).await.is_err() {
                            // The actor has already terminated.
                        }
                    }
                    None => { let _ = request.result.send(Err(AppError::InvalidRequest("direction is not part of this runtime".into()))); }
                }
            }
            Some(update) = updates.recv() => {
                match update {
                    ActorUpdate::Snapshot(direction) => {
                        status.send_modify(|snapshot| {
                            if let Some(current) = snapshot.directions.iter_mut().find(|current| current.direction == direction.direction) {
                                *current = direction;
                            }
                            if snapshot.phase != RuntimePhase::Stopping {
                                snapshot.phase = if snapshot.phase == RuntimePhase::Starting
                                    && !snapshot.directions.iter().all(|value| value.phase == DirectionPhase::Running)
                                {
                                    RuntimePhase::Starting
                                } else {
                                    aggregate_phase(&snapshot.directions)
                                };
                            }
                        });
                    }
                }
            }
            joined = tasks.join_next() => {
                remaining = remaining.saturating_sub(1);
                match joined {
                    Some(Ok(exit)) => {
                        if let Some(code) = exit.failure {
                            if !failures.contains(&code) { failures.push(code); }
                        }
                        status.send_modify(|snapshot| {
                            if let Some(direction) = snapshot.directions.iter_mut().find(|value| value.direction == exit.direction) {
                                direction.phase = DirectionPhase::Stopped;
                            }
                        });
                    }
                    Some(Err(_)) => {
                        if !failures.contains(&RuntimeFailureCode::CleanupTimeout) {
                            failures.push(RuntimeFailureCode::CleanupTimeout);
                        }
                    }
                    None => break,
                }
                if !stopping && remaining > 0 && status.borrow().phase == RuntimePhase::Starting {
                    stopping = true;
                    stop_tx.send_replace(true);
                    status.send_modify(|snapshot| snapshot.phase = RuntimePhase::Stopping);
                }
            }
        }
    }
    let outcome = if failures.is_empty() {
        RuntimeOutcome::Completed
    } else {
        RuntimeOutcome::Failed { codes: failures }
    };
    status.send_modify(|snapshot| {
        snapshot.phase = RuntimePhase::Stopped;
        snapshot.outcome = Some(outcome.clone());
    });
    completion.send_replace(Some(outcome));
}

impl DirectionActor {
    async fn run(mut self) -> ActorExit {
        let mut resources = match self.open_resources().await {
            Ok(resources) => resources,
            Err(RuntimeFailureCode::Interrupted) if *self.stop.borrow() => {
                self.publish(DirectionPhase::Stopped, None).await;
                return ActorExit {
                    direction: self.plan.direction,
                    failure: None,
                };
            }
            Err(code) => {
                self.fail(code.clone());
                self.publish(DirectionPhase::Stopped, Some(code.clone()))
                    .await;
                return ActorExit {
                    direction: self.plan.direction,
                    failure: Some(code),
                };
            }
        };
        if let Err(code) = self.await_started(&mut resources).await {
            self.abort_resources(resources).await;
            if code == RuntimeFailureCode::Interrupted && *self.stop.borrow() {
                self.publish(DirectionPhase::Stopped, None).await;
                return ActorExit {
                    direction: self.plan.direction,
                    failure: None,
                };
            }
            self.fail(code.clone());
            self.publish(DirectionPhase::Stopped, Some(code.clone()))
                .await;
            return ActorExit {
                direction: self.plan.direction,
                failure: Some(code),
            };
        }
        resources.ports.capture.clear();
        self.publish(DirectionPhase::Running, None).await;
        loop {
            match self.run_connected(resources).await {
                ConnectedExit::Stopped(failure) => {
                    if let Some(code) = &failure {
                        self.fail(code.clone());
                    }
                    self.publish(DirectionPhase::Stopped, failure.clone()).await;
                    return ActorExit {
                        direction: self.plan.direction,
                        failure,
                    };
                }
                ConnectedExit::Paused(code) => {
                    self.fail(code.clone());
                    self.publish(DirectionPhase::Paused, Some(code.clone()))
                        .await;
                    loop {
                        tokio::select! {
                            _ = self.stop.changed() => if *self.stop.borrow() {
                                self.publish(DirectionPhase::Stopped, Some(code.clone())).await;
                                return ActorExit { direction: self.plan.direction, failure: Some(code) };
                            },
                            control = self.controls.recv() => match control {
                                Some(DirectionControl::Resume(result)) => {
                                    let response = self.resume().await;
                                    match response {
                                        Ok(next) => {
                                            let _ = result.send(Ok(()));
                                            resources = next;
                                            self.publish(DirectionPhase::Running, None).await;
                                            break;
                                        }
                                        Err(error) => { let _ = result.send(Err(error)); }
                                    }
                                }
                                None => return ActorExit { direction: self.plan.direction, failure: Some(code) },
                            }
                        }
                    }
                }
            }
        }
    }

    async fn open_resources(&mut self) -> Result<Resources, RuntimeFailureCode> {
        let audio = self.audio.clone();
        let request = self.plan.request.clone();
        let mut audio_task = tokio::spawn(async move { audio.open(request).await });
        let provider = self.prepared.clone();
        let direction = self.plan.direction;
        let mut provider_task =
            tokio::spawn(async move { provider.connect_direction(direction).await });
        let audio_deadline = tokio::time::sleep(AUDIO_OPEN_TIMEOUT);
        let provider_deadline = tokio::time::sleep(PROVIDER_READY_TIMEOUT);
        tokio::pin!(audio_deadline, provider_deadline);
        let mut ports = None;
        let mut directed: Option<DirectedProviderStream> = None;
        loop {
            if ports.is_some() && directed.is_some() {
                let ports = ports.take().expect("checked audio readiness");
                let directed = directed.take().expect("checked provider readiness");
                return Ok(Resources {
                    stream: Arc::from(directed.stream),
                    ports,
                    output_sequence: 0,
                });
            }
            tokio::select! {
                _ = self.stop.changed() => if *self.stop.borrow() {
                    match ports.take() {
                        Some(ports) => close_ports(ports).await,
                        None => cleanup_late_audio(audio_task).await,
                    }
                    match directed.take() {
                        Some(stream) => cleanup_directed(stream).await,
                        None => cleanup_late_provider(provider_task).await,
                    }
                    return Err(RuntimeFailureCode::Interrupted);
                },
                result = &mut audio_task, if ports.is_none() => match result {
                    Ok(Ok(opened)) => ports = Some(opened),
                    Ok(Err(error)) => {
                        match directed.take() {
                            Some(stream) => cleanup_directed(stream).await,
                            None => cleanup_late_provider(provider_task).await,
                        }
                        return Err(audio_failure(&error));
                    }
                    Err(_) => {
                        match directed.take() {
                            Some(stream) => cleanup_directed(stream).await,
                            None => cleanup_late_provider(provider_task).await,
                        }
                        return Err(RuntimeFailureCode::CleanupTimeout);
                    }
                },
                result = &mut provider_task, if directed.is_none() => match result {
                    Ok(Ok(opened)) => directed = Some(opened),
                    Ok(Err(error)) => {
                        match ports.take() {
                            Some(ports) => close_ports(ports).await,
                            None => cleanup_late_audio(audio_task).await,
                        }
                        return Err(provider_failure(&error));
                    }
                    Err(_) => {
                        match ports.take() {
                            Some(ports) => close_ports(ports).await,
                            None => cleanup_late_audio(audio_task).await,
                        }
                        return Err(RuntimeFailureCode::CleanupTimeout);
                    }
                },
                _ = &mut audio_deadline, if ports.is_none() => {
                    match directed.take() {
                        Some(stream) => cleanup_directed(stream).await,
                        None => cleanup_late_provider(provider_task).await,
                    }
                    cleanup_late_audio(audio_task).await;
                    return Err(RuntimeFailureCode::StartupTimeout);
                }
                _ = &mut provider_deadline, if directed.is_none() => {
                    match ports.take() {
                        Some(ports) => close_ports(ports).await,
                        None => cleanup_late_audio(audio_task).await,
                    }
                    provider_task.abort();
                    return Err(RuntimeFailureCode::StartupTimeout);
                }
            }
        }
    }

    async fn await_started(&mut self, resources: &mut Resources) -> Result<(), RuntimeFailureCode> {
        let deadline = tokio::time::sleep(PROVIDER_READY_TIMEOUT);
        tokio::pin!(deadline);
        loop {
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
        let mut capture_baseline = resources.ports.control.statistics().capture;
        let mut diagnostic_tick = tokio::time::interval(Duration::from_secs(1));
        diagnostic_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        diagnostic_tick.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = self.stop.changed() => if *self.stop.borrow() {
                    return ConnectedExit::Stopped(self.graceful_stop(resources, packetizer, pending).await.err());
                },
                control = self.controls.recv() => if let Some(DirectionControl::Resume(result)) = control {
                    let _ = result.send(Err(AppError::InvalidRequest("direction is already running".into())));
                },
                changed = resources.ports.health.changed() => {
                    if changed.is_err() || *resources.ports.health.borrow() != crate::audio::runtime::AudioHealth::Ready {
                        self.diagnostics.observe_gap(GapCause::DeviceLoss, 0);
                        let _ = self.flush_diagnostics(&resources, true, None);
                        self.abort_resources(resources).await;
                        return ConnectedExit::Paused(RuntimeFailureCode::DeviceUnavailable);
                    }
                }
                _ = diagnostic_tick.tick() => {
                    if self.flush_diagnostics(&resources, false, None).is_err() {
                        self.abort_resources(resources).await;
                        return ConnectedExit::Paused(RuntimeFailureCode::Storage);
                    }
                }
                _ = wait_for_pacing(next_send_at), if !pending.is_empty() => {
                    let packet = pending.pop_front().expect("pacing branch requires a packet");
                    let duration = packet.duration();
                    let captured_at = packet.captured_at();
                    let result = resources.stream.send_audio(packet).await;
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
                            match self.recover_same_stream(resources, false, false).await {
                                Ok(next) => {
                                    resources = next;
                                    capture_baseline = resources.ports.control.statistics().capture;
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
                            self.diagnostics.observe_gap(GapCause::ProviderReconnect, 0);
                            if self.sink.advance_epoch().is_err() {
                                self.abort_resources(resources).await;
                                return ConnectedExit::Paused(RuntimeFailureCode::Storage);
                            }
                            self.epoch = self.sink.epoch();
                            self.publish(DirectionPhase::Reconnecting, None).await;
                            match self.recover_same_stream(resources, true, false).await {
                                Ok(next) => {
                                    resources = next;
                                    packetizer.reset();
                                    pending.clear();
                                    next_send_at = None;
                                    capture_baseline = resources.ports.control.statistics().capture;
                                    self.publish(DirectionPhase::Running, None).await;
                                }
                                Err(code) => return ConnectedExit::Paused(code),
                            }
                        }
                        Ok(Some(ProviderEvent::Audio { frame, .. })) => {
                            if let Err(code) = route_audio(&resources.ports, &frame, &mut resources.output_sequence) {
                                if code == RuntimeFailureCode::DeviceUnavailable {
                                    self.diagnostics.observe_gap(GapCause::OutputOverflow, 1);
                                    self.publish(DirectionPhase::Reconnecting, None).await;
                                    match self.recover_same_stream(resources, false, true).await {
                                        Ok(next) => {
                                            resources = next;
                                            packetizer.reset();
                                            pending.clear();
                                            next_send_at = None;
                                            capture_baseline = resources.ports.control.statistics().capture;
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
                        Ok(Some(event)) => if self.handle_non_audio_event(&event).is_err() {
                            self.abort_resources(resources).await;
                            return ConnectedExit::Paused(RuntimeFailureCode::Storage);
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
                    let lost = capture
                        .dropped_native_blocks
                        .saturating_sub(capture_baseline.dropped_native_blocks)
                        .saturating_add(
                            capture
                                .dropped_normalized_frames
                                .saturating_sub(capture_baseline.dropped_normalized_frames),
                        );
                    capture_baseline = capture;
                    if lost != 0 {
                        self.diagnostics.observe_gap(GapCause::CaptureOverflow, lost);
                        pending.clear();
                        packetizer.reset();
                        next_send_at = None;
                        self.publish(DirectionPhase::Reconnecting, None).await;
                        match self.recover_same_stream(resources, false, true).await {
                            Ok(next) => {
                                resources = next;
                                capture_baseline = resources.ports.control.statistics().capture;
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
                    for packet in packets {
                        if pending.len() == PENDING_PACKET_CAPACITY {
                            pending.pop_front();
                            self.diagnostics.observe_gap(GapCause::CaptureOverflow, 1);
                        }
                        pending.push_back(packet);
                    }
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
        resources.ports.control.suppress_now();
        let stream = resources.stream.clone();
        let output_sequence = resources.output_sequence;
        let audio = self.audio.clone();
        let request = self.plan.request.clone();
        let mut replacement = tokio::spawn(async move {
            let _ = resources.ports.control.discard_pending().await;
            let _ = resources.ports.control.close().await;
            audio.open(request).await
        });
        let mut provider_ready = provider_was_ready;
        let mut actual_gap_seen = gap_seen;
        let mut opened: Option<AudioPorts> = None;
        let deadline = tokio::time::sleep(PROVIDER_READY_TIMEOUT + AUDIO_OPEN_TIMEOUT);
        tokio::pin!(deadline);
        loop {
            if provider_ready && opened.is_some() {
                let ports = opened.take().unwrap();
                ports.capture.clear();
                return Ok(Resources {
                    stream,
                    ports,
                    output_sequence,
                });
            }
            tokio::select! {
                _ = &mut deadline => {
                    replacement.abort();
                    let _ = stream.cancel().await;
                    return Err(RuntimeFailureCode::StartupTimeout);
                }
                _ = self.stop.changed() => if *self.stop.borrow() {
                    replacement.abort();
                    let _ = stream.cancel().await;
                    return Err(RuntimeFailureCode::Interrupted);
                },
                result = &mut replacement, if opened.is_none() => match result {
                    Ok(Ok(ports)) => opened = Some(ports),
                    Ok(Err(error)) => return Err(audio_failure(&error)),
                    Err(_) => return Err(RuntimeFailureCode::CleanupTimeout),
                },
                event = stream.next_event() => match event {
                    Ok(Some(ProviderEvent::Gap { .. })) => {
                        if !actual_gap_seen {
                            self.sink.advance_epoch().map_err(|_| RuntimeFailureCode::Storage)?;
                            self.epoch = self.sink.epoch();
                            actual_gap_seen = true;
                        }
                        provider_ready = false;
                    }
                    Ok(Some(ProviderEvent::SessionStarted { .. })) => {
                        if !actual_gap_seen { return Err(RuntimeFailureCode::InvalidEvent); }
                        provider_ready = true;
                    }
                    Ok(Some(ProviderEvent::Audio { .. })) => {}
                    Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                        self.fail_provider(&error, metadata.log_id.clone());
                        return Err(provider_failure(&error));
                    }
                    Ok(Some(ProviderEvent::Finished { .. })) | Ok(None) => return Err(RuntimeFailureCode::Provider),
                    Ok(Some(event)) => self.handle_non_audio_event(&event)?,
                    Err(error) => {
                        self.fail_provider(&error, None);
                        return Err(provider_failure(&error));
                    }
                }
            }
        }
    }

    async fn graceful_stop(
        &mut self,
        mut resources: Resources,
        mut packetizer: AudioPacketizer,
        mut pending: VecDeque<crate::audio::AudioFrame>,
    ) -> Result<(), RuntimeFailureCode> {
        let stop_result =
            tokio::time::timeout(CAPTURE_STOP_TIMEOUT, resources.ports.control.stop_capture())
                .await;
        let stop_failure = match stop_result {
            Ok(Ok(())) => None,
            Ok(Err(_)) => Some(RuntimeFailureCode::DeviceUnavailable),
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
            while let Some(packet) = pending.front().cloned() {
                self.admit_stop_packet(&mut resources, packet).await?;
                pending.pop_front();
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
                let dropped = pending
                    .len()
                    .saturating_add(resources.ports.capture.clear());
                self.diagnostics.observe_gap(
                    GapCause::CaptureOverflow,
                    u64::try_from(dropped.max(1)).unwrap_or(u64::MAX),
                );
                self.cancel_and_close(&resources).await;
                return Err(RuntimeFailureCode::Interrupted);
            }
        }

        if let Err(error) = resources.stream.finish().await {
            let code = provider_failure(&error);
            self.fail_provider(&error, None);
            self.cancel_and_close(&resources).await;
            return Err(code);
        }
        let terminal_result = tokio::time::timeout(PROVIDER_FINISH_TIMEOUT, async {
            loop {
                match resources.stream.next_event().await {
                    Ok(Some(ProviderEvent::Audio { frame, .. })) => {
                        route_audio(&resources.ports, &frame, &mut resources.output_sequence)?;
                    }
                    Ok(Some(ProviderEvent::Finished { .. })) => return Ok(()),
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
        })
        .await;
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
            Ok(Err(_)) | Err(_) => {
                self.cancel_and_close(&resources).await;
                return Err(RuntimeFailureCode::PlaybackTimeout);
            }
        };

        match tokio::time::timeout(CLOSE_TIMEOUT, resources.ports.control.close()).await {
            Ok(Ok(())) => {
                let statistics = resources.ports.control.statistics();
                self.flush_diagnostics(
                    &resources,
                    true,
                    Some((Some(playback.boundary), statistics.playback)),
                )
            }
            Ok(Err(_)) | Err(_) => {
                resources.ports.control.suppress_now();
                let statistics = resources.ports.control.statistics();
                let _ = self.flush_diagnostics(&resources, true, Some((None, statistics.playback)));
                Err(RuntimeFailureCode::CleanupTimeout)
            }
        }
    }

    async fn abort_resources(&mut self, resources: Resources) {
        self.cancel_and_close(&resources).await;
    }

    async fn admit_stop_packet(
        &mut self,
        resources: &mut Resources,
        packet: crate::audio::AudioFrame,
    ) -> Result<(), RuntimeFailureCode> {
        let captured_at = packet.captured_at();
        loop {
            tokio::select! {
                result = resources.stream.send_audio(packet.clone()) => {
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
                        Err(error) => {
                            self.fail_provider(&error, None);
                            return Err(provider_failure(&error));
                        }
                    }
                    return Ok(());
                }
                event = resources.stream.next_event() => match event {
                    Ok(Some(ProviderEvent::Audio { frame, .. })) => {
                        route_audio(&resources.ports, &frame, &mut resources.output_sequence)?;
                    }
                    Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                        self.fail_provider(&error, metadata.log_id.clone());
                        return Err(provider_failure(&error));
                    }
                    Ok(Some(ProviderEvent::Finished { .. })) | Ok(None) => {
                        return Err(RuntimeFailureCode::Provider);
                    }
                    Ok(Some(event)) => self.handle_non_audio_event(&event)?,
                    Err(error) => {
                        self.fail_provider(&error, None);
                        return Err(provider_failure(&error));
                    }
                }
            }
        }
    }

    async fn cancel_and_close(&mut self, resources: &Resources) {
        resources.ports.control.suppress_now();
        let cancel_ack = tokio::time::timeout(CLOSE_TIMEOUT, resources.stream.cancel()).await;
        let control = resources.ports.control.clone();
        let audio_cleanup = async move {
            control.discard_pending().await?;
            control.close().await
        };
        tokio::pin!(audio_cleanup);
        let cleanup_ack = tokio::time::timeout(CLOSE_TIMEOUT, async {
            let mut audio_done = false;
            let mut terminal_done = false;
            loop {
                if audio_done && terminal_done {
                    return true;
                }
                tokio::select! {
                    result = &mut audio_cleanup, if !audio_done => {
                        audio_done = true;
                        if result.is_err() {
                            return false;
                        }
                    }
                    event = resources.stream.next_event(), if !terminal_done => match event {
                        Ok(Some(event @ (ProviderEvent::SourceInterim { .. }
                            | ProviderEvent::SourceFinal { .. }
                            | ProviderEvent::TranslationInterim { .. }
                            | ProviderEvent::TranslationFinal { .. }
                            | ProviderEvent::Muted { .. }))) => {
                            let _ = self.handle_non_audio_event(&event);
                        }
                        Ok(Some(ProviderEvent::Failed { metadata, error })) => {
                            if error != ProviderError::Cancelled {
                                self.fail_provider(&error, metadata.log_id.clone());
                            }
                            terminal_done = true;
                        }
                        Ok(Some(ProviderEvent::Finished { .. })) | Ok(None) | Err(_) => {
                            terminal_done = true;
                        }
                        Ok(Some(_)) => {}
                    }
                }
            }
        })
        .await
        .unwrap_or(false);
        let statistics = resources.ports.control.statistics();
        let _ = self.flush_diagnostics(resources, true, Some((None, statistics.playback)));
        let _ = cancel_ack;
        let _ = cleanup_ack;
    }

    async fn resume(&mut self) -> AppResult<Resources> {
        let inventory = self.audio.inventory().await?;
        validate_request(&inventory, &self.plan.request)?;
        self.sink.advance_epoch()?;
        self.epoch = self.sink.epoch();
        self.diagnostics.observe_gap(GapCause::ManualResume, 0);
        self.publish(DirectionPhase::Starting, None).await;
        let mut resources = self.open_resources().await.map_err(runtime_error)?;
        self.await_started(&mut resources)
            .await
            .map_err(runtime_error)?;
        resources.ports.capture.clear();
        Ok(resources)
    }

    fn handle_non_audio_event(&mut self, event: &ProviderEvent) -> Result<(), RuntimeFailureCode> {
        match event {
            ProviderEvent::SourceInterim { .. }
            | ProviderEvent::SourceFinal { .. }
            | ProviderEvent::TranslationInterim { .. }
            | ProviderEvent::TranslationFinal { .. } => {
                self.sink
                    .accept(event)
                    .map_err(|_| RuntimeFailureCode::Storage)?;
                self.diagnostics
                    .observe_transcript_overflow(self.sink.take_dropped_interims());
                Ok(())
            }
            ProviderEvent::Muted { .. } => Ok(()),
            _ => Err(RuntimeFailureCode::InvalidEvent),
        }
    }

    fn fail(&mut self, code: RuntimeFailureCode) {
        if self.failures.contains(&code) {
            return;
        }
        self.failures.push(code.clone());
        self.persist_failure(code, None, None);
    }

    fn fail_provider(&mut self, error: &ProviderError, log_id: Option<String>) {
        let code = provider_failure(error);
        if self.failures.contains(&code) {
            return;
        }
        self.failures.push(code.clone());
        let provider_code = match error {
            ProviderError::RemoteFailure { code } => Some(*code),
            _ => None,
        };
        self.persist_failure(code, provider_code, log_id);
    }

    fn persist_failure(
        &self,
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
        let _ = self
            .updates
            .send(ActorUpdate::Snapshot(DirectionSnapshot {
                direction: self.plan.direction,
                phase,
                stream_epoch: self.epoch,
                failure,
            }))
            .await;
    }
}

struct Resources {
    stream: Arc<dyn ProviderConnection>,
    ports: AudioPorts,
    output_sequence: u64,
}

enum ConnectedExit {
    Stopped(Option<RuntimeFailureCode>),
    Paused(RuntimeFailureCode),
}

fn prepare_audio(
    inventory: &AudioInventory,
    launch: &SessionLaunch,
) -> AppResult<(Vec<RuntimeEndpoint>, Vec<DirectionPlan>)> {
    let find = |id: &str| {
        inventory
            .endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint.id == id)
            .cloned()
            .ok_or_else(|| AppError::InvalidRequest(format!("audio endpoint not found: {id}")))
    };
    match (&launch.config.mode(), &launch.devices) {
        (
            SessionMode::Live,
            DeviceSelection::Live {
                input_id,
                m1_output_id,
            },
        ) => {
            let input = find(input_id)?;
            let output = find(m1_output_id)?;
            if !matches!(input.role, EndpointRole::Input | EndpointRole::M2Capture)
                || input.endpoint.direction != AudioDirection::Input
                || output.role != EndpointRole::M1Render
                || output.endpoint.direction != AudioDirection::Output
            {
                return Err(AppError::InvalidRequest("invalid live audio roles".into()));
            }
            AudioGraph::for_live(input.endpoint.clone(), output.endpoint.clone())?;
            router_direction(SessionMode::Live, launch.config.direction())?;
            Ok((
                vec![input, output],
                vec![DirectionPlan {
                    direction: launch.config.direction(),
                    request: AudioOpenRequest {
                        inventory_generation: inventory.generation,
                        input_id: input_id.clone(),
                        output_id: m1_output_id.clone(),
                        source: AudioSource::Microphone,
                    },
                }],
            ))
        }
        (
            SessionMode::Call,
            DeviceSelection::Call {
                a_input_id,
                m2_input_id,
                m1_output_id,
                physical_output_id,
            },
        ) => {
            let a = find(a_input_id)?;
            let m2 = find(m2_input_id)?;
            let m1 = find(m1_output_id)?;
            let physical = find(physical_output_id)?;
            if a.role != EndpointRole::Input
                || m2.role != EndpointRole::M2Capture
                || m1.role != EndpointRole::M1Render
                || physical.role != EndpointRole::PhysicalOutput
                || a.endpoint.direction != AudioDirection::Input
                || m2.endpoint.direction != AudioDirection::Input
                || m1.endpoint.direction != AudioDirection::Output
                || physical.endpoint.direction != AudioDirection::Output
            {
                return Err(AppError::InvalidRequest("invalid call audio roles".into()));
            }
            AudioGraph::for_call(
                a.endpoint.clone(),
                m2.endpoint.clone(),
                m1.endpoint.clone(),
                physical.endpoint.clone(),
            )?;
            Ok((
                vec![a, m2, m1, physical],
                vec![
                    DirectionPlan {
                        direction: SessionDirection::SourceToTarget,
                        request: AudioOpenRequest {
                            inventory_generation: inventory.generation,
                            input_id: a_input_id.clone(),
                            output_id: m1_output_id.clone(),
                            source: AudioSource::Microphone,
                        },
                    },
                    DirectionPlan {
                        direction: SessionDirection::TargetToSource,
                        request: AudioOpenRequest {
                            inventory_generation: inventory.generation,
                            input_id: m2_input_id.clone(),
                            output_id: physical_output_id.clone(),
                            source: AudioSource::System,
                        },
                    },
                ],
            ))
        }
        _ => Err(AppError::InvalidRequest(
            "session mode and device selection do not match".into(),
        )),
    }
}

fn validate_request(inventory: &AudioInventory, request: &AudioOpenRequest) -> AppResult<()> {
    if inventory.driver != DriverStatus::Ready {
        return Err(AppError::Audio {
            code: RuntimeFailureCode::AudioBackendUnavailable,
            message: "audio driver is not ready",
        });
    }
    for (id, direction) in [
        (&request.input_id, AudioDirection::Input),
        (&request.output_id, AudioDirection::Output),
    ] {
        let endpoint = inventory
            .endpoints
            .iter()
            .find(|endpoint| &endpoint.endpoint.id == id)
            .ok_or_else(|| AppError::InvalidRequest("selected endpoint is missing".into()))?;
        if endpoint.endpoint.direction != direction || !endpoint.endpoint.ready {
            return Err(AppError::InvalidRequest(
                "selected endpoint is unavailable".into(),
            ));
        }
    }
    Ok(())
}

fn route_audio(
    ports: &AudioPorts,
    frame: &crate::audio::AudioFrame,
    output_sequence: &mut u64,
) -> Result<(), RuntimeFailureCode> {
    for frame in split_provider_audio(frame, output_sequence)
        .map_err(|_| RuntimeFailureCode::InvalidEvent)?
    {
        match ports.playback.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                return Err(RuntimeFailureCode::DeviceUnavailable);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err(RuntimeFailureCode::DeviceUnavailable);
            }
        }
    }
    Ok(())
}

async fn close_ports(ports: AudioPorts) {
    ports.control.suppress_now();
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, ports.control.discard_pending()).await;
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, ports.control.close()).await;
}

async fn cleanup_late_audio(mut task: JoinHandle<AppResult<AudioPorts>>) {
    if let Ok(Ok(Ok(ports))) = tokio::time::timeout(LATE_AUDIO_CLEANUP_TIMEOUT, &mut task).await {
        close_ports(ports).await;
    } else {
        task.abort();
    }
}

async fn cleanup_late_provider(
    mut task: JoinHandle<crate::providers::ProviderResult<DirectedProviderStream>>,
) {
    match tokio::time::timeout(CLOSE_TIMEOUT, &mut task).await {
        Ok(Ok(Ok(stream))) => cleanup_directed(stream).await,
        Ok(_) => {}
        Err(_) => task.abort(),
    }
}

async fn cleanup_directed(stream: DirectedProviderStream) {
    let stream: Arc<dyn ProviderConnection> = Arc::from(stream.stream);
    let _ = stream.cancel().await;
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, async {
        loop {
            match stream.next_event().await {
                Ok(Some(ProviderEvent::Finished { .. } | ProviderEvent::Failed { .. }))
                | Ok(None)
                | Err(_) => break,
                Ok(Some(_)) => {}
            }
        }
    })
    .await;
}

async fn wait_for_pacing(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(deadline).await;
    }
}

fn aggregate_phase(directions: &[DirectionSnapshot]) -> RuntimePhase {
    if directions
        .iter()
        .all(|direction| direction.phase == DirectionPhase::Running)
    {
        return RuntimePhase::Running;
    }
    if directions.iter().any(|direction| {
        matches!(
            direction.phase,
            DirectionPhase::Running | DirectionPhase::Reconnecting
        )
    }) {
        return RuntimePhase::Running;
    }
    if directions
        .iter()
        .all(|direction| direction.phase == DirectionPhase::Paused)
    {
        return RuntimePhase::Paused;
    }
    RuntimePhase::Starting
}

fn provider_app_error(error: ProviderError) -> AppError {
    AppError::InvalidRequest(format!("provider preflight failed: {error}"))
}

fn audio_failure(error: &AppError) -> RuntimeFailureCode {
    match error {
        AppError::Audio { code, .. } => code.clone(),
        AppError::InvalidRequest(_) => RuntimeFailureCode::InvalidEvent,
        _ => RuntimeFailureCode::DeviceUnavailable,
    }
}

fn runtime_error(code: RuntimeFailureCode) -> AppError {
    AppError::Audio {
        code,
        message: "runtime direction could not resume",
    }
}

fn direction_key(direction: SessionDirection) -> u8 {
    match direction {
        SessionDirection::SourceToTarget => 0,
        SessionDirection::TargetToSource => 1,
        SessionDirection::Bidirectional => 2,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}
