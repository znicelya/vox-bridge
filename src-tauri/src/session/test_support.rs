use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
            AudioBackend, AudioControl, AudioHealth, AudioOpenRequest, AudioPorts, PlaybackReport,
        },
    },
    config::{ProviderConfig, SecretStore},
    domain::{
        launch::{DeviceSelection, RuntimePhase, SessionLaunch},
        session::{LanguagePair, ProviderId, SessionConfig, SessionDirection},
        transcript::{
            AudioStatistics, CaptureStatistics, PlaybackBoundary, PlaybackStatistics,
            RuntimeFailureCode,
        },
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

#[derive(Default)]
struct FixtureAudioState {
    captures: HashMap<String, BoundedAudioQueue>,
    outputs: HashMap<String, Arc<PlaybackProbe>>,
    health: HashMap<String, watch::Sender<AudioHealth>>,
    ready: HashMap<String, bool>,
    delayed_opens: HashMap<String, oneshot::Receiver<Result<(), RuntimeFailureCode>>>,
    delayed_closes: HashMap<String, oneshot::Receiver<()>>,
}

struct PlaybackProbe {
    receiver: AsyncMutex<mpsc::Receiver<AudioFrame>>,
    accepted: AtomicUsize,
    consumed: AtomicUsize,
    sealed: AtomicBool,
    changed: Notify,
}

#[derive(Clone)]
pub(crate) struct FixtureAudioBackend {
    inventory: AudioInventory,
    state: Arc<Mutex<FixtureAudioState>>,
    open_ports: Arc<AtomicUsize>,
    total_opens: Arc<AtomicUsize>,
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
        let ready = endpoints
            .iter()
            .map(|endpoint| (endpoint.endpoint.id.clone(), true))
            .collect();
        Self {
            inventory: AudioInventory {
                generation: Uuid::new_v4(),
                driver: DriverStatus::Ready,
                endpoints,
            },
            state: Arc::new(Mutex::new(FixtureAudioState {
                ready,
                ..Default::default()
            })),
            open_ports: Arc::new(AtomicUsize::new(0)),
            total_opens: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(crate) async fn send_capture(&self, input_id: &str, frame: AudioFrame) -> AppResult<()> {
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
        let mut receiver = probe.receiver.lock().await;
        let frame = receiver.recv().await;
        if frame.is_some() {
            probe.consumed.fetch_add(1, Ordering::SeqCst);
            probe.changed.notify_waiters();
        }
        drop(receiver);
        frame
    }

    pub(crate) fn lose(&self, id: &str) {
        let mut state = self.state.lock().unwrap();
        state.ready.insert(id.into(), false);
        if let Some(health) = state.health.get(id) {
            health.send_replace(AudioHealth::Lost);
        }
    }

    pub(crate) fn restore(&self, id: &str) {
        self.state.lock().unwrap().ready.insert(id.into(), true);
    }

    pub(crate) fn open_ports(&self) -> usize {
        self.open_ports.load(Ordering::SeqCst)
    }

    pub(crate) fn total_opens(&self) -> usize {
        self.total_opens.load(Ordering::SeqCst)
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
            .insert(input_id.into(), wait);
        release
    }
}

struct FixtureControl {
    input_id: String,
    output_id: String,
    capture: BoundedAudioQueue,
    probe: Arc<PlaybackProbe>,
    backend: FixtureAudioBackend,
    closed: AtomicBool,
    delayed_close: AsyncMutex<Option<oneshot::Receiver<()>>>,
}

impl FixtureControl {
    fn close_once(&self) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.capture.close();
            let mut state = self.backend.state.lock().unwrap();
            state.captures.remove(&self.input_id);
            state.outputs.remove(&self.output_id);
            self.backend.open_ports.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

#[async_trait]
impl AudioControl for FixtureControl {
    fn suppress_now(&self) {
        self.capture.close();
        self.capture.clear();
    }

    fn statistics(&self) -> AudioStatistics {
        AudioStatistics {
            capture: CaptureStatistics {
                dropped_native_blocks: 0,
                dropped_normalized_frames: self.capture.dropped_frames(),
                finalized: self.closed.load(Ordering::SeqCst),
            },
            playback: PlaybackStatistics {
                input_sample_rate_hz: 24_000,
                accepted_for_processing_samples: self.probe.accepted.load(Ordering::SeqCst) as u64,
                discarded_before_conversion_samples: 0,
                native: None,
                finalized: self.closed.load(Ordering::SeqCst),
                tail_disposition_known: self.closed.load(Ordering::SeqCst),
            },
        }
    }

    async fn stop_capture(&self) -> AppResult<()> {
        self.capture.close();
        Ok(())
    }

    async fn discard_pending(&self) -> AppResult<()> {
        self.capture.close();
        self.capture.clear();
        while self.probe.receiver.lock().await.try_recv().is_ok() {}
        self.probe.changed.notify_waiters();
        Ok(())
    }

    async fn drain_playback(&self) -> AppResult<PlaybackReport> {
        self.probe.sealed.store(true, Ordering::SeqCst);
        loop {
            let changed = self.probe.changed.notified();
            if self.probe.receiver.lock().await.is_empty() {
                return Ok(PlaybackReport {
                    boundary: PlaybackBoundary::CallbackConsumed,
                    statistics: self.statistics().playback,
                });
            }
            changed.await;
        }
    }

    async fn close(&self) -> AppResult<()> {
        if let Some(wait) = self.delayed_close.lock().await.take() {
            let _ = wait.await;
        }
        self.close_once();
        Ok(())
    }
}

impl Drop for FixtureControl {
    fn drop(&mut self) {
        self.close_once();
    }
}

#[async_trait]
impl AudioBackend for FixtureAudioBackend {
    async fn inventory(&self) -> AppResult<AudioInventory> {
        let state = self.state.lock().unwrap();
        let mut inventory = self.inventory.clone();
        for endpoint in &mut inventory.endpoints {
            endpoint.endpoint.ready = state
                .ready
                .get(&endpoint.endpoint.id)
                .copied()
                .unwrap_or(false);
        }
        Ok(inventory)
    }

    async fn open(&self, request: AudioOpenRequest) -> AppResult<AudioPorts> {
        if request.inventory_generation != self.inventory.generation {
            return Err(AppError::InvalidRequest("stale fixture inventory".into()));
        }
        let delayed = self
            .state
            .lock()
            .unwrap()
            .delayed_opens
            .remove(&request.input_id);
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
        if !state.ready.get(&request.input_id).copied().unwrap_or(false)
            || !state
                .ready
                .get(&request.output_id)
                .copied()
                .unwrap_or(false)
        {
            return Err(AppError::InvalidRequest(
                "fixture endpoint unavailable".into(),
            ));
        }
        let capture = BoundedAudioQueue::new(8);
        let (playback, receiver) = mpsc::channel(8);
        let probe = Arc::new(PlaybackProbe {
            receiver: AsyncMutex::new(receiver),
            accepted: AtomicUsize::new(0),
            consumed: AtomicUsize::new(0),
            sealed: AtomicBool::new(false),
            changed: Notify::new(),
        });
        let (health, receiver) = watch::channel(AudioHealth::Ready);
        state
            .captures
            .insert(request.input_id.clone(), capture.clone());
        state
            .outputs
            .insert(request.output_id.clone(), probe.clone());
        state
            .health
            .insert(request.input_id.clone(), health.clone());
        state.health.insert(request.output_id.clone(), health);
        self.open_ports.fetch_add(1, Ordering::SeqCst);
        self.total_opens.fetch_add(1, Ordering::SeqCst);
        let delayed_close = state.delayed_closes.remove(&request.input_id);
        let control = Arc::new(FixtureControl {
            input_id: request.input_id,
            output_id: request.output_id,
            capture: capture.clone(),
            probe,
            backend: self.clone(),
            closed: AtomicBool::new(false),
            delayed_close: AsyncMutex::new(delayed_close),
        });
        Ok(AudioPorts {
            capture,
            playback,
            health: receiver,
            control,
        })
    }
}

pub(crate) struct ObservedFixtureProvider {
    scripts: [FixtureScript; 2],
    connects: Arc<[AtomicUsize; 2]>,
    finishes: Arc<[AtomicUsize; 2]>,
    sent: Arc<[Mutex<Vec<u64>>; 2]>,
    sent_at: Arc<[Mutex<Vec<Instant>>; 2]>,
    queue_full_next: Arc<[AtomicBool; 2]>,
    queue_full_count: Arc<[AtomicUsize; 2]>,
    not_started_count: Arc<[AtomicUsize; 2]>,
    send_gates: Arc<[GapGate; 2]>,
    gap_gates: Arc<[GapGate; 2]>,
    terminal_gates: Arc<[GapGate; 2]>,
}

#[derive(Default)]
struct GapGate {
    blocked: AtomicBool,
    released: AtomicBool,
    changed: Notify,
}

pub(crate) struct GapRelease {
    gate: Arc<[GapGate; 2]>,
    direction: usize,
}

impl GapRelease {
    pub(crate) fn release(self) {
        self.gate[self.direction]
            .released
            .store(true, Ordering::SeqCst);
        self.gate[self.direction].changed.notify_waiters();
    }
}

impl ObservedFixtureProvider {
    fn new(forward: FixtureScript, reverse: FixtureScript) -> Self {
        Self {
            scripts: [forward, reverse],
            connects: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            finishes: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            sent: Arc::new(std::array::from_fn(|_| Mutex::new(Vec::new()))),
            sent_at: Arc::new(std::array::from_fn(|_| Mutex::new(Vec::new()))),
            queue_full_next: Arc::new(std::array::from_fn(|_| AtomicBool::new(false))),
            queue_full_count: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            not_started_count: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            send_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            gap_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
            terminal_gates: Arc::new(std::array::from_fn(|_| GapGate::default())),
        }
    }

    pub(crate) fn connect_counts(&self) -> [usize; 2] {
        std::array::from_fn(|i| self.connects[i].load(Ordering::SeqCst))
    }

    pub(crate) fn finish_counts(&self) -> [usize; 2] {
        std::array::from_fn(|i| self.finishes[i].load(Ordering::SeqCst))
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
        let direction = index(direction);
        self.send_gates[direction]
            .released
            .store(false, Ordering::SeqCst);
        self.send_gates[direction]
            .blocked
            .store(true, Ordering::SeqCst);
        GapRelease {
            gate: self.send_gates.clone(),
            direction,
        }
    }

    pub(crate) fn delay_gap(&self, direction: SessionDirection) -> GapRelease {
        let direction = index(direction);
        self.gap_gates[direction]
            .released
            .store(false, Ordering::SeqCst);
        self.gap_gates[direction]
            .blocked
            .store(true, Ordering::SeqCst);
        GapRelease {
            gate: self.gap_gates.clone(),
            direction,
        }
    }

    pub(crate) fn delay_terminal(&self, direction: SessionDirection) -> GapRelease {
        let direction = index(direction);
        self.terminal_gates[direction]
            .released
            .store(false, Ordering::SeqCst);
        self.terminal_gates[direction]
            .blocked
            .store(true, Ordering::SeqCst);
        GapRelease {
            gate: self.terminal_gates.clone(),
            direction,
        }
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
            sent: self.sent.clone(),
            sent_at: self.sent_at.clone(),
            queue_full_next: self.queue_full_next.clone(),
            queue_full_count: self.queue_full_count.clone(),
            not_started_count: self.not_started_count.clone(),
            send_gates: self.send_gates.clone(),
            gap_gates: self.gap_gates.clone(),
            terminal_gates: self.terminal_gates.clone(),
            held_event: AsyncMutex::new(None),
        }))
    }
}

struct ObservedConnection {
    inner: ProviderStream,
    direction: usize,
    finishes: Arc<[AtomicUsize; 2]>,
    sent: Arc<[Mutex<Vec<u64>>; 2]>,
    sent_at: Arc<[Mutex<Vec<Instant>>; 2]>,
    queue_full_next: Arc<[AtomicBool; 2]>,
    queue_full_count: Arc<[AtomicUsize; 2]>,
    not_started_count: Arc<[AtomicUsize; 2]>,
    send_gates: Arc<[GapGate; 2]>,
    gap_gates: Arc<[GapGate; 2]>,
    terminal_gates: Arc<[GapGate; 2]>,
    held_event: AsyncMutex<Option<ProviderEvent>>,
}

#[async_trait]
impl ProviderConnection for ObservedConnection {
    async fn send_audio(&self, frame: AudioFrame) -> ProviderResult<()> {
        let gate = &self.send_gates[self.direction];
        loop {
            let changed = gate.changed.notified();
            if !gate.blocked.load(Ordering::SeqCst) || gate.released.load(Ordering::SeqCst) {
                break;
            }
            changed.await;
        }
        if self.queue_full_next[self.direction].swap(false, Ordering::SeqCst) {
            self.queue_full_count[self.direction].fetch_add(1, Ordering::SeqCst);
            return Err(crate::providers::ProviderError::QueueFull);
        }
        let sequence = frame.sequence();
        if let Err(error) = self.inner.send_audio(frame).await {
            if error == crate::providers::ProviderError::NotStarted {
                self.not_started_count[self.direction].fetch_add(1, Ordering::SeqCst);
            }
            return Err(error);
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
        self.inner.finish().await
    }

    async fn cancel(&self) -> ProviderResult<()> {
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
            let gate = &self.gap_gates[self.direction];
            let blocked_gap = matches!(held.as_ref(), Some(ProviderEvent::Gap { .. }))
                && gate.blocked.load(Ordering::SeqCst)
                && !gate.released.load(Ordering::SeqCst);
            let terminal_gate = &self.terminal_gates[self.direction];
            let blocked_terminal = matches!(
                held.as_ref(),
                Some(ProviderEvent::Finished { .. } | ProviderEvent::Failed { .. })
            ) && terminal_gate.blocked.load(Ordering::SeqCst)
                && !terminal_gate.released.load(Ordering::SeqCst);
            if blocked_gap || blocked_terminal {
                let changed = if blocked_gap {
                    gate.changed.notified()
                } else {
                    terminal_gate.changed.notified()
                };
                drop(held);
                changed.await;
                continue;
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
