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
use tokio::sync::{Mutex as AsyncMutex, Notify, mpsc, watch};
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
        transcript::{AudioStatistics, CaptureStatistics, PlaybackBoundary, PlaybackStatistics},
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
}

struct FixtureControl {
    input_id: String,
    output_id: String,
    capture: BoundedAudioQueue,
    probe: Arc<PlaybackProbe>,
    backend: FixtureAudioBackend,
    closed: AtomicBool,
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
        let control = Arc::new(FixtureControl {
            input_id: request.input_id,
            output_id: request.output_id,
            capture: capture.clone(),
            probe,
            backend: self.clone(),
            closed: AtomicBool::new(false),
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
}

impl ObservedFixtureProvider {
    fn new(forward: FixtureScript, reverse: FixtureScript) -> Self {
        Self {
            scripts: [forward, reverse],
            connects: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            finishes: Arc::new(std::array::from_fn(|_| AtomicUsize::new(0))),
            sent: Arc::new(std::array::from_fn(|_| Mutex::new(Vec::new()))),
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
        }))
    }
}

struct ObservedConnection {
    inner: ProviderStream,
    direction: usize,
    finishes: Arc<[AtomicUsize; 2]>,
    sent: Arc<[Mutex<Vec<u64>>; 2]>,
}

#[async_trait]
impl ProviderConnection for ObservedConnection {
    async fn send_audio(&self, frame: AudioFrame) -> ProviderResult<()> {
        let sequence = frame.sequence();
        self.inner.send_audio(frame).await?;
        self.sent[self.direction].lock().unwrap().push(sequence);
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
        self.inner.next_event().await
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
