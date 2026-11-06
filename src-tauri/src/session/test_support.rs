use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, mpsc, oneshot, watch},
    time::Instant,
};
use uuid::Uuid;

use crate::{
    audio::{
        AudioFrame, PcmSpec,
        platform::inventory::{AudioBackendKind, AudioInventory, EndpointRole, RuntimeEndpoint},
        platform::{AudioDirection, AudioEndpoint, DriverStatus},
        queue::BoundedAudioQueue,
        runtime::{
            AudioBackend, AudioControl, AudioFailure, AudioHealth, AudioOpenRequest, AudioPorts,
            AudioShared, PlaybackReport, add, audio_error, validate_playback,
        },
    },
    config::{ProviderConfig, SecretStore},
    domain::{
        launch::{DeviceSelection, RuntimePhase, SessionLaunch},
        session::{LanguagePair, ProviderId, SessionConfig, SessionDirection},
        transcript::{AudioStatistics, PlaybackBoundary, PlaybackStatistics, RuntimeFailureCode},
        translation::{TimedTranslation, Translation},
    },
    error::{AppError, AppResult},
    events::EventBus,
    providers::{
        ProviderCapabilities, ProviderConnection, ProviderEvent, ProviderEventMetadata,
        ProviderResult, ProviderSessionConfig, ProviderStream, TranslationProvider,
        factory::ProviderFactory,
        fixture::{FixtureProvider, FixtureScript, FixtureStep},
    },
    session::{RunningSession, TranslationRuntime},
    storage::{LocalStore, StoredSession},
};

#[path = "fixture_audio_tests.rs"]
mod fixture_audio_tests;

#[path = "fixture_provider_tests.rs"]
mod fixture_provider_tests;

pub(crate) fn metadata() -> ProviderEventMetadata {
    ProviderEventMetadata {
        session_id: Uuid::nil(),
        direction: SessionDirection::SourceToTarget,
        provider_event_type: None,
        timestamp: None,
        sentence_id: None,
        local_correlation_id: None,
        log_id: None,
    }
}

pub(crate) fn final_text(language: &str, text: &str) -> TimedTranslation {
    TimedTranslation {
        translation: Translation {
            language: language.into(),
            text: text.into(),
        },
        start: Some(Duration::ZERO),
        end: Some(Duration::from_millis(80)),
    }
}

pub(crate) async fn wait_phase(run: &RunningSession, phase: RuntimePhase) {
    let mut status = run.status();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if status.borrow_and_update().phase == phase {
                break;
            }
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

struct FixtureAudioState {
    inventory: AudioInventory,
    captures: HashMap<String, BoundedAudioQueue>,
    outputs: HashMap<String, Arc<PlaybackProbe>>,
    input_outputs: HashMap<String, String>,
    playback_observations: HashMap<String, AudioShared>,
    health: HashMap<String, watch::Sender<AudioHealth>>,
    leases: HashSet<String>,
    quarantined: HashSet<String>,
    delayed_opens: HashMap<String, oneshot::Receiver<Result<(), RuntimeFailureCode>>>,
    delayed_closes: HashMap<String, CloseAcknowledgement>,
}

struct PlaybackProbe {
    // The worker owns ingress; callback waits never hold its receiver lock.
    pending: Mutex<VecDeque<AudioFrame>>,
    shared: AudioShared,
    injected_normalized_loss: AtomicU64,
    sealed: AtomicBool,
    worker_done: AtomicBool,
    failure: Mutex<Option<AudioFailure>>,
    changed: Notify,
}

impl PlaybackProbe {
    fn suppress(&self) {
        let mut pending = self.pending.lock().unwrap();
        self.shared.suppress();
        for frame in pending.drain(..) {
            add(
                &self.shared.counters.native_discarded,
                frame.samples().len() as u64,
            );
        }
        self.changed.notify_waiters();
    }

    async fn wait_worker_done(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.worker_done.load(Ordering::Acquire) {
                return;
            }
            changed.await;
        }
    }
}

async fn playback_worker(probe: Arc<PlaybackProbe>, mut receiver: mpsc::Receiver<AudioFrame>) {
    loop {
        let changed = probe.shared.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if probe
            .shared
            .gates
            .playback_suppressed
            .load(Ordering::Acquire)
        {
            receiver.close();
            while let Ok(frame) = receiver.try_recv() {
                if validate_playback(&frame).is_ok() {
                    let samples = frame.samples().len() as u64;
                    add(&probe.shared.counters.accepted, samples);
                    add(&probe.shared.counters.pre_conversion_discarded, samples);
                }
            }
            break;
        }
        if probe.sealed.load(Ordering::Acquire) {
            receiver.close();
        }
        let frame = tokio::select! {
            biased;
            _ = &mut changed => continue,
            frame = receiver.recv() => frame,
        };
        let Some(frame) = frame else {
            break;
        };
        if let Err(error) = validate_playback(&frame) {
            *probe.failure.lock().unwrap() = Some(AudioFailure::from_error(error));
            probe.suppress();
            continue;
        }
        let samples = frame.samples().len() as u64;
        add(&probe.shared.counters.accepted, samples);
        let mut pending = probe.pending.lock().unwrap();
        if probe
            .shared
            .gates
            .playback_suppressed
            .load(Ordering::Acquire)
        {
            add(&probe.shared.counters.pre_conversion_discarded, samples);
        } else {
            // Synthetic native playback is identity conversion at 24 kHz.
            add(&probe.shared.counters.native_generated, samples);
            if pending.len() == 8 {
                let discarded = pending.pop_front().unwrap();
                add(
                    &probe.shared.counters.native_discarded,
                    discarded.samples().len() as u64,
                );
            }
            pending.push_back(frame);
        }
        drop(pending);
        probe.changed.notify_waiters();
    }
    probe.worker_done.store(true, Ordering::Release);
    probe.changed.notify_waiters();
}

enum CloseAcknowledgement {
    Success(oneshot::Receiver<()>),
    Result(oneshot::Receiver<Result<(), RuntimeFailureCode>>),
}

impl CloseAcknowledgement {
    async fn wait(self) -> Result<(), AudioFailure> {
        let result = match self {
            Self::Success(wait) => wait.await.map(|()| Ok(())),
            Self::Result(wait) => wait.await,
        };
        result
            .map_err(|_| {
                AudioFailure::new(
                    RuntimeFailureCode::CleanupTimeout,
                    "fixture close acknowledgement was dropped",
                )
            })?
            .map_err(|code| AudioFailure::new(code, "injected fixture close failure"))
    }
}

#[derive(Clone)]
pub(crate) struct FixtureAudioBackend {
    state: Arc<Mutex<FixtureAudioState>>,
    open_ports: Arc<AtomicUsize>,
    total_opens: Arc<AtomicUsize>,
    open_calls: Arc<AtomicUsize>,
}

impl FixtureAudioBackend {
    fn new() -> Self {
        let endpoint = |id: &str, direction: AudioDirection, role: EndpointRole| RuntimeEndpoint {
            endpoint: AudioEndpoint {
                id: id.into(),
                display_name: id.into(),
                direction,
                sample_rates: vec![16_000, 24_000],
                channels: 1,
                ready: true,
            },
            role,
            backend: AudioBackendKind::Cpal,
        };
        let endpoints = vec![
            endpoint("live-input", AudioDirection::Input, EndpointRole::Input),
            endpoint("a-input", AudioDirection::Input, EndpointRole::Input),
            endpoint("m2-capture", AudioDirection::Input, EndpointRole::M2Capture),
            endpoint("m1-render", AudioDirection::Output, EndpointRole::M1Render),
            endpoint(
                "physical-output",
                AudioDirection::Output,
                EndpointRole::PhysicalOutput,
            ),
        ];
        Self {
            state: Arc::new(Mutex::new(FixtureAudioState {
                inventory: AudioInventory {
                    generation: Uuid::new_v4(),
                    driver: DriverStatus::Ready,
                    endpoints,
                },
                captures: HashMap::new(),
                outputs: HashMap::new(),
                input_outputs: HashMap::new(),
                playback_observations: HashMap::new(),
                health: HashMap::new(),
                leases: HashSet::new(),
                quarantined: HashSet::new(),
                delayed_opens: HashMap::new(),
                delayed_closes: HashMap::new(),
            })),
            open_ports: Arc::new(AtomicUsize::new(0)),
            total_opens: Arc::new(AtomicUsize::new(0)),
            open_calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(crate) async fn send_capture(&self, input_id: &str, frame: AudioFrame) -> AppResult<()> {
        if frame.spec() != PcmSpec::pcm16_mono(16_000) {
            return Err(AppError::InvalidRequest(
                "invalid fixture capture frame".into(),
            ));
        }
        let queue = self
            .state
            .lock()
            .unwrap()
            .captures
            .get(input_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidRequest("capture port is not open".into()))?;
        queue
            .push(frame)
            .await
            .map_err(|_| AppError::InvalidRequest("capture port is closed".into()))
    }

    pub(crate) async fn next_playback(&self, output_id: &str) -> Option<AudioFrame> {
        let probe = self.state.lock().unwrap().outputs.get(output_id).cloned()?;
        loop {
            let changed = probe.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut pending = probe.pending.lock().unwrap();
                if probe
                    .shared
                    .gates
                    .playback_suppressed
                    .load(Ordering::Acquire)
                {
                    return None;
                }
                if let Some(frame) = pending.pop_front() {
                    add(
                        &probe.shared.counters.native_consumed,
                        frame.samples().len() as u64,
                    );
                    probe.changed.notify_waiters();
                    return Some(frame);
                }
                if probe.worker_done.load(Ordering::Acquire) {
                    return None;
                }
            }
            changed.await;
        }
    }

    pub(crate) fn lose(&self, id: &str) {
        self.set_ready(id, false);
        let state = self.state.lock().unwrap();
        if let Some(health) = state.health.get(id) {
            crate::audio::runtime::mark_lost(health);
        }
        let output_id = state.input_outputs.get(id).map_or(id, String::as_str);
        let probe = state.outputs.get(output_id).cloned();
        if let Some(capture) = state.captures.get(id) {
            capture.close();
        }
        drop(state);
        if let Some(probe) = probe {
            probe.suppress();
        }
    }

    pub(crate) fn restore(&self, id: &str) {
        self.set_ready(id, true);
    }

    pub(crate) fn set_ready(&self, id: &str, ready: bool) {
        let mut state = self.state.lock().unwrap();
        let endpoint = state
            .inventory
            .endpoints
            .iter_mut()
            .find(|e| e.endpoint.id == id)
            .expect("known fixture endpoint");
        if endpoint.endpoint.ready != ready {
            endpoint.endpoint.ready = ready;
            state.inventory.generation = Uuid::new_v4();
        }
    }

    pub(crate) fn set_role(&self, id: &str, role: EndpointRole) {
        let mut state = self.state.lock().unwrap();
        let endpoint = state
            .inventory
            .endpoints
            .iter_mut()
            .find(|e| e.endpoint.id == id)
            .expect("known fixture endpoint");
        if endpoint.role != role {
            endpoint.role = role;
            state.inventory.generation = Uuid::new_v4();
        }
    }

    pub(crate) fn set_driver_status(&self, driver: DriverStatus) {
        let mut state = self.state.lock().unwrap();
        if state.inventory.driver != driver {
            state.inventory.driver = driver;
            state.inventory.generation = Uuid::new_v4();
        }
    }

    pub(crate) fn open_ports(&self) -> usize {
        self.open_ports.load(Ordering::SeqCst)
    }

    pub(crate) fn open_calls(&self) -> usize {
        self.open_calls.load(Ordering::SeqCst)
    }

    pub(crate) fn total_opens(&self) -> usize {
        self.total_opens.load(Ordering::SeqCst)
    }

    pub(crate) fn playback_statistics(&self, output_id: &str) -> Option<PlaybackStatistics> {
        self.state
            .lock()
            .unwrap()
            .playback_observations
            .get(output_id)
            .map(|shared| shared.statistics().playback)
    }

    pub(crate) fn inject_capture_loss(
        &self,
        input_id: &str,
        raw_blocks: u64,
        normalized_frames: u64,
    ) {
        let state = self.state.lock().unwrap();
        let output_id = state
            .input_outputs
            .get(input_id)
            .expect("open fixture input");
        let probe = state.outputs.get(output_id).unwrap();
        add(&probe.shared.counters.native_blocks, raw_blocks);
        add(&probe.injected_normalized_loss, normalized_frames);
    }

    pub(crate) fn delay_open(
        &self,
        input_id: &str,
    ) -> oneshot::Sender<Result<(), RuntimeFailureCode>> {
        let (release, wait) = oneshot::channel();
        self.state
            .lock()
            .unwrap()
            .delayed_opens
            .insert(input_id.into(), wait);
        release
    }

    pub(crate) fn delay_close(&self, input_id: &str) -> oneshot::Sender<()> {
        let (release, wait) = oneshot::channel();
        self.state
            .lock()
            .unwrap()
            .delayed_closes
            .insert(input_id.into(), CloseAcknowledgement::Success(wait));
        release
    }

    pub(crate) fn delay_close_result(
        &self,
        input_id: &str,
    ) -> oneshot::Sender<Result<(), RuntimeFailureCode>> {
        let (release, wait) = oneshot::channel();
        self.state
            .lock()
            .unwrap()
            .delayed_closes
            .insert(input_id.into(), CloseAcknowledgement::Result(wait));
        release
    }
}

struct FixtureControl {
    input_id: String,
    output_id: String,
    capture: BoundedAudioQueue,
    probe: Arc<PlaybackProbe>,
    backend: FixtureAudioBackend,
    health: watch::Sender<AudioHealth>,
    delayed_close: Mutex<Option<CloseAcknowledgement>>,
    close_result: Mutex<Option<watch::Receiver<Option<Result<(), AudioFailure>>>>>,
}

impl FixtureControl {
    fn begin_close(&self) -> watch::Receiver<Option<Result<(), AudioFailure>>> {
        self.suppress_now();
        let mut entry = self.close_result.lock().unwrap();
        if let Some(result) = &*entry {
            return result.clone();
        }
        let (publish, result) = watch::channel(None);
        *entry = Some(result.clone());
        let backend = self.backend.clone();
        let ids = [self.input_id.clone(), self.output_id.clone()];
        let capture = self.capture.clone();
        let probe = self.probe.clone();
        let health = self.health.clone();
        let delayed = self.delayed_close.lock().unwrap().take();
        tokio::spawn(async move {
            let acknowledgement = async {
                probe.wait_worker_done().await;
                if let Some(delayed) = delayed {
                    delayed.wait().await?;
                }
                Ok(())
            };
            tokio::pin!(acknowledgement);
            let value = tokio::select! {
                value = &mut acknowledgement => value,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {
                    backend.state.lock().unwrap().quarantined.extend(ids.clone());
                    health.send_replace(AudioHealth::NeedsRestart);
                    publish.send_replace(Some(Err(AudioFailure::new(RuntimeFailureCode::CleanupTimeout, "fixture close acknowledgement timed out"))));
                    // The owner can still acknowledge later; the timeout result and quarantine persist.
                    acknowledgement.await
                }
            };
            match &value {
                Ok(()) => {
                    capture.close();
                    probe
                        .shared
                        .counters
                        .capture_finalized
                        .store(true, Ordering::Release);
                    probe
                        .shared
                        .counters
                        .playback_finalized
                        .store(true, Ordering::Release);
                    probe
                        .shared
                        .counters
                        .tail_known
                        .store(true, Ordering::Release);
                    let mut state = backend.state.lock().unwrap();
                    state.captures.remove(&ids[0]);
                    state.outputs.remove(&ids[1]);
                    state.input_outputs.remove(&ids[0]);
                    for id in &ids {
                        state.health.remove(id);
                        if !state.quarantined.contains(id) {
                            state.leases.remove(id);
                        }
                    }
                    backend.open_ports.fetch_sub(1, Ordering::SeqCst);
                }
                Err(_) => {
                    backend.state.lock().unwrap().quarantined.extend(ids);
                    health.send_replace(AudioHealth::NeedsRestart);
                }
            }
            if publish.borrow().is_none() {
                publish.send_replace(Some(value));
            }
        });
        result
    }
}

#[async_trait]
impl AudioControl for FixtureControl {
    fn suppress_now(&self) {
        self.probe.suppress();
        self.capture.close();
        self.capture.clear();
    }

    fn statistics(&self) -> AudioStatistics {
        let mut statistics = self.probe.shared.statistics();
        statistics.capture.dropped_normalized_frames = statistics
            .capture
            .dropped_normalized_frames
            .saturating_add(self.probe.injected_normalized_loss.load(Ordering::Acquire));
        statistics
    }

    async fn stop_capture(&self) -> AppResult<()> {
        self.capture.close();
        self.probe.shared.stop_capture();
        self.probe
            .shared
            .counters
            .capture_finalized
            .store(true, Ordering::Release);
        Ok(())
    }

    async fn discard_pending(&self) -> AppResult<()> {
        self.suppress_now();
        self.probe.wait_worker_done().await;
        Ok(())
    }

    async fn drain_playback(&self) -> AppResult<PlaybackReport> {
        self.probe.sealed.store(true, Ordering::SeqCst);
        self.probe.shared.changed.notify_waiters();
        let drained = async {
            loop {
                let changed = self.probe.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if let Some(error) = self.probe.failure.lock().unwrap().clone() {
                    return Err(error.into());
                }
                if self
                    .probe
                    .shared
                    .gates
                    .playback_suppressed
                    .load(Ordering::Acquire)
                {
                    return Err(audio_error("fixture playback was suppressed"));
                }
                if self.probe.worker_done.load(Ordering::Acquire)
                    && self.probe.pending.lock().unwrap().is_empty()
                {
                    self.probe
                        .shared
                        .counters
                        .playback_finalized
                        .store(true, Ordering::Release);
                    self.probe
                        .shared
                        .counters
                        .tail_known
                        .store(true, Ordering::Release);
                    return Ok(PlaybackReport {
                        boundary: PlaybackBoundary::CallbackConsumed,
                        statistics: self.statistics().playback,
                    });
                }
                changed.await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), drained)
            .await
            .map_err(|_| {
                self.suppress_now();
                self.backend
                    .state
                    .lock()
                    .unwrap()
                    .quarantined
                    .extend([self.input_id.clone(), self.output_id.clone()]);
                self.health.send_replace(AudioHealth::NeedsRestart);
                AppError::from(AudioFailure::new(
                    RuntimeFailureCode::PlaybackTimeout,
                    "fixture playback drain timed out",
                ))
            })?
    }

    async fn close(&self) -> AppResult<()> {
        let mut result = self.begin_close();
        loop {
            if let Some(value) = result.borrow_and_update().clone() {
                return value.map_err(Into::into);
            }
            result
                .changed()
                .await
                .map_err(|_| audio_error("fixture close acknowledgement closed"))?;
        }
    }
}

impl Drop for FixtureControl {
    fn drop(&mut self) {
        // Dropping the client requests owner cleanup, but is never its acknowledgement.
        self.begin_close();
    }
}

#[async_trait]
impl AudioBackend for FixtureAudioBackend {
    async fn inventory(&self) -> AppResult<AudioInventory> {
        Ok(self.state.lock().unwrap().inventory.clone())
    }

    async fn open(&self, request: AudioOpenRequest) -> AppResult<AudioPorts> {
        self.open_calls.fetch_add(1, Ordering::SeqCst);
        let delayed = {
            let mut state = self.state.lock().unwrap();
            validate_fixture_open(&state, &request)?;
            state.delayed_opens.remove(&request.input_id)
        };
        if let Some(delayed) = delayed {
            match delayed.await {
                Ok(Ok(())) => {}
                Ok(Err(code)) => {
                    return Err(AppError::Audio {
                        code,
                        message: "injected fixture open failure",
                    });
                }
                Err(_) => {
                    return Err(AppError::Audio {
                        code: RuntimeFailureCode::CleanupTimeout,
                        message: "fixture open release was dropped",
                    });
                }
            }
        }
        let mut state = self.state.lock().unwrap();
        validate_fixture_open(&state, &request)?;
        let capture = BoundedAudioQueue::new(8);
        let (playback, receiver) = mpsc::channel(8);
        let shared = AudioShared::new(capture.clone());
        shared.counters.native_rate.store(24_000, Ordering::Release);
        let probe = Arc::new(PlaybackProbe {
            pending: Mutex::new(VecDeque::new()),
            shared,
            injected_normalized_loss: AtomicU64::new(0),
            sealed: AtomicBool::new(false),
            worker_done: AtomicBool::new(false),
            failure: Mutex::new(None),
            changed: Notify::new(),
        });
        tokio::spawn(playback_worker(probe.clone(), receiver));
        let (health, receiver) = watch::channel(AudioHealth::Ready);
        state
            .captures
            .insert(request.input_id.clone(), capture.clone());
        state
            .outputs
            .insert(request.output_id.clone(), probe.clone());
        state
            .input_outputs
            .insert(request.input_id.clone(), request.output_id.clone());
        state
            .playback_observations
            .insert(request.output_id.clone(), probe.shared.clone());
        state
            .health
            .insert(request.input_id.clone(), health.clone());
        state
            .health
            .insert(request.output_id.clone(), health.clone());
        state
            .leases
            .extend([request.input_id.clone(), request.output_id.clone()]);
        self.open_ports.fetch_add(1, Ordering::SeqCst);
        self.total_opens.fetch_add(1, Ordering::SeqCst);
        let delayed_close = state.delayed_closes.remove(&request.input_id);
        let control = Arc::new(FixtureControl {
            input_id: request.input_id,
            output_id: request.output_id,
            capture: capture.clone(),
            probe,
            backend: self.clone(),
            health,
            delayed_close: Mutex::new(delayed_close),
            close_result: Mutex::new(None),
        });
        Ok(AudioPorts {
            capture,
            playback,
            health: receiver,
            control,
        })
    }
}

fn validate_fixture_open(state: &FixtureAudioState, request: &AudioOpenRequest) -> AppResult<()> {
    if request.inventory_generation != state.inventory.generation
        || state.inventory.driver != DriverStatus::Ready
    {
        return Err(audio_error(
            "fixture inventory is stale or driver is not ready",
        ));
    }
    let endpoint = |id: &str| {
        state
            .inventory
            .endpoints
            .iter()
            .find(|e| e.endpoint.id == id && e.endpoint.ready)
    };
    let input =
        endpoint(&request.input_id).ok_or_else(|| audio_error("fixture input is unavailable"))?;
    let output =
        endpoint(&request.output_id).ok_or_else(|| audio_error("fixture output is unavailable"))?;
    if input.endpoint.direction != AudioDirection::Input
        || !matches!(input.role, EndpointRole::Input | EndpointRole::M2Capture)
        || output.endpoint.direction != AudioDirection::Output
        || !matches!(
            output.role,
            EndpointRole::M1Render | EndpointRole::PhysicalOutput
        )
        || request.input_id == request.output_id
    {
        return Err(audio_error("invalid fixture endpoint directions or roles"));
    }
    if state.leases.len() >= 4
        || state.leases.contains(&request.input_id)
        || state.leases.contains(&request.output_id)
    {
        return Err(audio_error("fixture endpoint is busy or requires restart"));
    }
    Ok(())
}

pub(crate) struct ObservedFixtureProvider {
    scripts: [FixtureScript; 2],
    connects: Arc<[AtomicUsize; 2]>,
    finishes: Arc<[AtomicUsize; 2]>,
    cancels: Arc<[AtomicUsize; 2]>,
    send_calls: Arc<[AtomicUsize; 2]>,
    sent: Arc<[Mutex<Vec<u64>>; 2]>,
    samples: Arc<[Mutex<VecDeque<Vec<i16>>>; 2]>,
    sent_at: Arc<[Mutex<Vec<Instant>>; 2]>,
    queue_full_next: Arc<[AtomicBool; 2]>,
    queue_full_count: Arc<[AtomicUsize; 2]>,
    not_started_count: Arc<[AtomicUsize; 2]>,
    send_gates: Arc<[GapGate; 2]>,
    gap_gates: Arc<[GapGate; 2]>,
    terminal_gates: Arc<[GapGate; 2]>,
    finish_gates: Arc<[GapGate; 2]>,
    cancel_gates: Arc<[GapGate; 2]>,
    started_gates: Arc<[GapGate; 2]>,
    final_gates: Arc<[GapGate; 2]>,
}

#[derive(Default)]
struct GapGate {
    state: Mutex<GateState>,
    changed: Notify,
    entered: Notify,
}

#[derive(Default)]
struct GateState {
    generation: Uuid,
    blocked: bool,
    entered: bool,
}

impl GapGate {
    async fn wait(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self.state.lock().unwrap();
                if !state.blocked {
                    return;
                }
                if !state.entered {
                    state.entered = true;
                    self.entered.notify_waiters();
                }
            }
            changed.await;
        }
    }
}

pub(crate) struct GapRelease {
    gate: Arc<[GapGate; 2]>,
    direction: usize,
    generation: Uuid,
}

impl GapRelease {
    fn hold(gate: Arc<[GapGate; 2]>, direction: usize) -> Self {
        let generation = Uuid::new_v4();
        *gate[direction].state.lock().unwrap() = GateState {
            generation,
            blocked: true,
            entered: false,
        };
        gate[direction].changed.notify_waiters();
        gate[direction].entered.notify_waiters();
        Self {
            gate,
            direction,
            generation,
        }
    }

    /// True only when the operation/event reached this still-current hold.
    pub(crate) async fn wait_entered(&self) -> bool {
        let gate = &self.gate[self.direction];
        loop {
            let entered = gate.entered.notified();
            tokio::pin!(entered);
            entered.as_mut().enable();
            {
                let state = gate.state.lock().unwrap();
                if state.generation != self.generation {
                    return false;
                }
                if state.entered {
                    return true;
                }
                if !state.blocked {
                    return false;
                }
            }
            entered.await;
        }
    }

    pub(crate) fn release(self) {
        drop(self);
    }
}

impl Drop for GapRelease {
    fn drop(&mut self) {
        let gate = &self.gate[self.direction];
        let mut state = gate.state.lock().unwrap();
        if state.generation == self.generation {
            state.blocked = false;
            gate.changed.notify_waiters();
            gate.entered.notify_waiters();
        }
    }
}

impl ObservedFixtureProvider {
    fn new(forward: FixtureScript, reverse: FixtureScript) -> Self {
        Self {
            scripts: [forward, reverse],
            connects: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            finishes: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            cancels: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            send_calls: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            sent: Arc::new(std::array::from_fn(|_| Mutex::new(Vec::new()))),
            samples: Arc::new(std::array::from_fn(|_| Mutex::new(VecDeque::new()))),
            sent_at: Arc::new(std::array::from_fn(|_| Mutex::new(Vec::new()))),
            queue_full_next: Arc::new(std::array::from_fn(|_| AtomicBool::new(false))),
            queue_full_count: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            not_started_count: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            send_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            gap_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            terminal_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            finish_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            cancel_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            started_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            final_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
        }
    }

    pub(crate) fn connect_counts(&self) -> [usize; 2] {
        std::array::from_fn(|i| self.connects[i].load(Ordering::SeqCst))
    }

    pub(crate) fn finish_counts(&self) -> [usize; 2] {
        std::array::from_fn(|i| self.finishes[i].load(Ordering::SeqCst))
    }

    pub(crate) fn cancel_counts(&self) -> [usize; 2] {
        std::array::from_fn(|i| self.cancels[i].load(Ordering::SeqCst))
    }

    pub(crate) fn send_calls(&self) -> [usize; 2] {
        std::array::from_fn(|i| self.send_calls[i].load(Ordering::SeqCst))
    }

    pub(crate) fn sent_samples(&self, direction: SessionDirection) -> Vec<Vec<i16>> {
        self.samples[index(direction)]
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn sent_sequences(&self, direction: SessionDirection) -> Vec<u64> {
        self.sent[index(direction)].lock().unwrap().clone()
    }

    pub(crate) fn sent_at(&self, direction: SessionDirection) -> Vec<Instant> {
        self.sent_at[index(direction)].lock().unwrap().clone()
    }

    pub(crate) fn queue_full_once(&self, direction: SessionDirection) {
        self.queue_full_next[index(direction)].store(true, Ordering::SeqCst);
    }

    pub(crate) fn queue_full_count(&self, direction: SessionDirection) -> usize {
        self.queue_full_count[index(direction)].load(Ordering::SeqCst)
    }

    pub(crate) fn not_started_count(&self, direction: SessionDirection) -> usize {
        self.not_started_count[index(direction)].load(Ordering::SeqCst)
    }

    pub(crate) fn delay_send(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.send_gates.clone(), index(direction))
    }

    pub(crate) fn delay_gap(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.gap_gates.clone(), index(direction))
    }

    pub(crate) fn delay_terminal(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.terminal_gates.clone(), index(direction))
    }

    pub(crate) fn delay_finish(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.finish_gates.clone(), index(direction))
    }

    pub(crate) fn delay_cancel(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.cancel_gates.clone(), index(direction))
    }

    pub(crate) fn delay_started(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.started_gates.clone(), index(direction))
    }

    pub(crate) fn delay_final(&self, direction: SessionDirection) -> GapRelease {
        GapRelease::hold(self.final_gates.clone(), index(direction))
    }
}

#[async_trait]
impl TranslationProvider for ObservedFixtureProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("fixture").unwrap()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        FixtureProvider::new(self.scripts[0].clone()).capabilities()
    }

    async fn connect(&self, config: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        let direction = index(config.direction);
        self.connects[direction].fetch_add(1, Ordering::SeqCst);
        let stream = FixtureProvider::new(self.scripts[direction].clone())
            .connect(config)
            .await?;
        Ok(Box::new(ObservedConnection {
            inner: stream,
            direction,
            finishes: self.finishes.clone(),
            cancels: self.cancels.clone(),
            send_calls: self.send_calls.clone(),
            sent: self.sent.clone(),
            samples: self.samples.clone(),
            sent_at: self.sent_at.clone(),
            queue_full_next: self.queue_full_next.clone(),
            queue_full_count: self.queue_full_count.clone(),
            not_started_count: self.not_started_count.clone(),
            send_gates: self.send_gates.clone(),
            gap_gates: self.gap_gates.clone(),
            terminal_gates: self.terminal_gates.clone(),
            finish_gates: self.finish_gates.clone(),
            cancel_gates: self.cancel_gates.clone(),
            started_gates: self.started_gates.clone(),
            final_gates: self.final_gates.clone(),
            held_event: AsyncMutex::new(None),
        }))
    }
}

struct ObservedConnection {
    inner: ProviderStream,
    direction: usize,
    finishes: Arc<[AtomicUsize; 2]>,
    cancels: Arc<[AtomicUsize; 2]>,
    send_calls: Arc<[AtomicUsize; 2]>,
    sent: Arc<[Mutex<Vec<u64>>; 2]>,
    samples: Arc<[Mutex<VecDeque<Vec<i16>>>; 2]>,
    sent_at: Arc<[Mutex<Vec<Instant>>; 2]>,
    queue_full_next: Arc<[AtomicBool; 2]>,
    queue_full_count: Arc<[AtomicUsize; 2]>,
    not_started_count: Arc<[AtomicUsize; 2]>,
    send_gates: Arc<[GapGate; 2]>,
    gap_gates: Arc<[GapGate; 2]>,
    terminal_gates: Arc<[GapGate; 2]>,
    finish_gates: Arc<[GapGate; 2]>,
    cancel_gates: Arc<[GapGate; 2]>,
    started_gates: Arc<[GapGate; 2]>,
    final_gates: Arc<[GapGate; 2]>,
    held_event: AsyncMutex<Option<ProviderEvent>>,
}

#[async_trait]
impl ProviderConnection for ObservedConnection {
    async fn send_audio(&self, frame: AudioFrame) -> ProviderResult<()> {
        self.send_calls[self.direction].fetch_add(1, Ordering::SeqCst);
        self.send_gates[self.direction].wait().await;
        if self.queue_full_next[self.direction].swap(false, Ordering::SeqCst) {
            self.queue_full_count[self.direction].fetch_add(1, Ordering::SeqCst);
            return Err(crate::providers::ProviderError::QueueFull);
        }
        let sequence = frame.sequence();
        // Retain only validated-size candidate samples; rejected sends are never observed.
        let observed_samples = (frame.samples().len() <= 1280).then(|| frame.samples().to_vec());
        if let Err(error) = self.inner.send_audio(frame).await {
            if error == crate::providers::ProviderError::NotStarted {
                self.not_started_count[self.direction].fetch_add(1, Ordering::SeqCst);
            }
            return Err(error);
        }
        if let Some(observed_samples) = observed_samples {
            let mut samples = self.samples[self.direction].lock().unwrap();
            if samples.len() == 64 {
                samples.pop_front();
            }
            samples.push_back(observed_samples);
        }
        self.sent[self.direction].lock().unwrap().push(sequence);
        self.sent_at[self.direction]
            .lock()
            .unwrap()
            .push(Instant::now());
        Ok(())
    }

    async fn finish(&self) -> ProviderResult<()> {
        self.finishes[self.direction].fetch_add(1, Ordering::SeqCst);
        self.finish_gates[self.direction].wait().await;
        self.inner.finish().await
    }

    async fn cancel(&self) -> ProviderResult<()> {
        self.cancels[self.direction].fetch_add(1, Ordering::SeqCst);
        self.cancel_gates[self.direction].wait().await;
        self.inner.cancel().await
    }

    async fn next_event(&self) -> ProviderResult<Option<ProviderEvent>> {
        loop {
            let mut held = self.held_event.lock().await;
            if held.is_none() {
                drop(held);
                let event = self.inner.next_event().await?;
                held = self.held_event.lock().await;
                *held = event;
            }
            let gate = match held.as_ref() {
                Some(ProviderEvent::SessionStarted { .. }) => {
                    Some(&self.started_gates[self.direction])
                }
                Some(
                    ProviderEvent::SourceFinal { .. } | ProviderEvent::TranslationFinal { .. },
                ) => Some(&self.final_gates[self.direction]),
                Some(ProviderEvent::Gap { .. }) => Some(&self.gap_gates[self.direction]),
                Some(ProviderEvent::Finished { .. } | ProviderEvent::Failed { .. }) => {
                    Some(&self.terminal_gates[self.direction])
                }
                _ => None,
            };
            if let Some(gate) = gate {
                drop(held);
                gate.wait().await;
                held = self.held_event.lock().await;
            }
            return Ok(held.take());
        }
    }
}

fn index(direction: SessionDirection) -> usize {
    match direction {
        SessionDirection::SourceToTarget => 0,
        SessionDirection::TargetToSource => 1,
        SessionDirection::Bidirectional => panic!("fixture stream direction must be directed"),
    }
}

pub(crate) struct RuntimeHarness {
    pub(crate) runtime: Arc<TranslationRuntime>,
    pub(crate) store: Arc<LocalStore>,
    pub(crate) bus: EventBus,
    pub(crate) audio: Arc<FixtureAudioBackend>,
    pub(crate) provider: Arc<ObservedFixtureProvider>,
    pub(crate) launch: SessionLaunch,
    pub(crate) saved: ProviderConfig,
    pub(crate) id: Uuid,
    _dir: TempDir,
}

impl RuntimeHarness {
    pub(crate) fn fail_final_capture_accounting_writes(&self) {
        rusqlite::Connection::open(self._dir.path().join("vox-bridge.sqlite"))
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_final_capture BEFORE INSERT ON session_diagnostics
                WHEN json_extract(NEW.payload_json, '$.payload.capture_accounting.statistics.finalized') = 1
                BEGIN SELECT RAISE(FAIL, 'injected capture-accounting write failure'); END;",
            )
            .unwrap();
    }

    pub(crate) fn fail_forward_diagnostic_writes(&self) {
        rusqlite::Connection::open(self._dir.path().join("vox-bridge.sqlite"))
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_forward_diagnostics BEFORE INSERT ON session_diagnostics
                WHEN json_extract(NEW.payload_json, '$.direction') = 'source_to_target'
                BEGIN SELECT RAISE(FAIL, 'injected diagnostic storage failure'); END;",
            )
            .unwrap();
    }

    pub(crate) fn live(script: FixtureScript) -> AppResult<Self> {
        let reverse = FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            })],
        )
        .map_err(|error| AppError::Internal(error.to_string()))?;
        Self::build(false, false, script, reverse)
    }

    pub(crate) fn live_reverse(script: FixtureScript) -> AppResult<Self> {
        let forward = FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            })],
        )
        .map_err(|error| AppError::Internal(error.to_string()))?;
        Self::build(false, true, forward, script)
    }

    pub(crate) fn call(forward: FixtureScript, reverse: FixtureScript) -> AppResult<Self> {
        Self::build(true, false, forward, reverse)
    }

    fn build(
        call: bool,
        reverse_live: bool,
        forward: FixtureScript,
        reverse: FixtureScript,
    ) -> AppResult<Self> {
        let dir = tempfile::tempdir().map_err(|error| AppError::Internal(error.to_string()))?;
        let store = Arc::new(LocalStore::open(dir.path())?);
        let secrets = Arc::new(SecretStore::for_tests(dir.path())?);
        let provider = Arc::new(ObservedFixtureProvider::new(forward, reverse));
        let factory = Arc::new(ProviderFactory::new(secrets).with_test_provider(provider.clone()));
        let audio = Arc::new(FixtureAudioBackend::new());
        let bus = EventBus::new();
        let id = Uuid::new_v4();
        bus.open_session(id);
        let config = if call {
            SessionConfig::call(LanguagePair::new("en", "zh")?)?
        } else {
            SessionConfig::live(
                LanguagePair::new("en", "zh")?,
                if reverse_live {
                    SessionDirection::TargetToSource
                } else {
                    SessionDirection::SourceToTarget
                },
            )?
        };
        let provider_config_id = Uuid::new_v4();
        let launch = SessionLaunch {
            config: config.clone(),
            provider_config_id,
            devices: if call {
                DeviceSelection::Call {
                    a_input_id: "a-input".into(),
                    m2_input_id: "m2-capture".into(),
                    m1_output_id: "m1-render".into(),
                    physical_output_id: "physical-output".into(),
                }
            } else {
                DeviceSelection::Live {
                    input_id: "live-input".into(),
                    m1_output_id: "m1-render".into(),
                }
            },
        };
        let saved = ProviderConfig::new(
            provider_config_id,
            "fixture",
            "https://fixture.example",
            None,
            None,
            vec![],
            None,
        )?;
        store.create_session(&StoredSession {
            id,
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            mode: if call { "call" } else { "live" }.into(),
            direction: if call {
                "bidirectional"
            } else if reverse_live {
                "target_to_source"
            } else {
                "source_to_target"
            }
            .into(),
            source_language: config.languages().source().into(),
            target_language: config.languages().target().into(),
        })?;
        let runtime = Arc::new(TranslationRuntime::new(
            factory,
            audio.clone(),
            store.clone(),
            bus.clone(),
        ));
        Ok(Self {
            runtime,
            store,
            bus,
            audio,
            provider,
            launch,
            saved,
            id,
            _dir: dir,
        })
    }
}
