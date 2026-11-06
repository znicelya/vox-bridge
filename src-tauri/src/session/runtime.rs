use super::{
    direction::{DiagnosticAccumulator, DirectionActor, RuntimeRouteHealth, router_direction},
    transcript_sink::TranscriptSink,
};
use crate::{
    audio::{
        PcmSpec,
        graph::AudioGraph,
        platform::inventory::{AudioInventory, EndpointRole, RuntimeEndpoint},
        platform::{AudioDirection, DriverStatus},
        runtime::{AudioBackend, AudioOpenRequest},
    },
    config::ProviderConfig,
    domain::{
        audio::{AudioSource, PLAYBACK_SAMPLE_RATE_HZ, SAMPLE_RATE_HZ},
        launch::{
            DeviceSelection, DirectionPhase, DirectionSnapshot, RuntimeOutcome, RuntimePhase,
            RuntimeSnapshot, SessionContext, SessionLaunch,
        },
        session::{SessionDirection, SessionMode},
        transcript::RuntimeFailureCode,
    },
    error::{AppError, AppResult},
    events::EventBus,
    providers::{
        ProviderError,
        factory::{PreparedProviderSession, ProviderFactory},
    },
    storage::LocalStore,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
    time::Instant,
};
use uuid::Uuid;

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
    graph: AudioGraph,
}

#[derive(Clone)]
pub(super) struct DirectionPlan {
    pub(super) direction: SessionDirection,
    pub(super) request: AudioOpenRequest,
    pub(super) endpoints: Vec<RuntimeEndpoint>,
}

#[derive(Clone)]
pub struct RunningSession {
    stop: watch::Sender<bool>,
    resume: mpsc::Sender<ResumeRequest>,
    status: watch::Receiver<RuntimeSnapshot>,
    completion: watch::Receiver<Option<RuntimeOutcome>>,
    pending_resumes: Arc<Mutex<[bool; 2]>>,
}

pub(super) struct ResumeRequest {
    direction: SessionDirection,
    result: oneshot::Sender<AppResult<()>>,
}

pub(super) enum DirectionControl {
    Resume(oneshot::Sender<AppResult<()>>),
}

pub(super) enum ActorUpdate {
    Snapshot(DirectionSnapshot),
    ResumeComplete {
        direction: SessionDirection,
        result: oneshot::Sender<AppResult<()>>,
        response: AppResult<()>,
    },
}

pub(super) struct ActorExit {
    pub(super) direction: SessionDirection,
    pub(super) failures: Vec<RuntimeFailureCode>,
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
        let (endpoints, directions, graph) = prepare_audio(&inventory, launch)?;
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
                provider_name: saved.display_name().to_owned(),
                endpoints,
            },
            provider,
            directions,
            graph,
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
        let (resume, resume_rx) = mpsc::channel(2);
        let pending_resumes = Arc::new(Mutex::new([false; 2]));
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
            pending_resumes.clone(),
        ));
        RunningSession {
            stop,
            resume,
            status,
            completion,
            pending_resumes,
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

    /// Cancellation request only. The existing runtime owner still performs and
    /// acknowledges all cleanup through the shared completion receiver.
    pub(crate) fn request_stop(&self) {
        self.stop.send_replace(true);
    }

    pub async fn stop(&self) -> RuntimeOutcome {
        self.request_stop();
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
        if *self.stop.borrow() {
            return Err(AppError::InvalidRequest("runtime is stopping".into()));
        }
        let key = direction_key(direction) as usize;
        let paused = self
            .status
            .borrow()
            .directions
            .iter()
            .any(|d| d.direction == direction && d.phase == DirectionPhase::Paused);
        if !paused {
            return Err(AppError::InvalidRequest("direction is not paused".into()));
        }
        {
            let mut pending = self.pending_resumes.lock().unwrap();
            if pending[key] {
                return Err(AppError::InvalidRequest("resume already pending".into()));
            }
            pending[key] = true;
        }
        let (result, receive) = oneshot::channel();
        if self
            .resume
            .try_send(ResumeRequest { direction, result })
            .is_err()
        {
            self.pending_resumes.lock().unwrap()[key] = false;
            return Err(AppError::InvalidRequest(
                "runtime is stopping or busy".into(),
            ));
        }
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
    pending_resumes: Arc<Mutex<[bool; 2]>>,
) {
    let (updates_tx, mut updates) = mpsc::channel(32);
    let mut controls = BTreeMap::new();
    let mut tasks = JoinSet::new();
    let mut failures = Vec::new();
    let health = RuntimeRouteHealth::default();
    let router = Arc::new(crate::audio::router::AudioRouter::paused_unbound(
        prepared.graph,
        health.clone(),
    ));
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
                    if !failures.contains(&RuntimeFailureCode::Storage) {
                        failures.push(RuntimeFailureCode::Storage);
                    }
                    stop_tx.send_replace(true);
                    continue;
                }
            };
        let actor = DirectionActor {
            session_id: prepared.session_id,
            route: router_direction(prepared.context.launch.config.mode(), plan.direction)
                .expect("validated direction"),
            router: router.clone(),
            health: health.clone(),
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
            transition_failures: Vec::new(),
            diagnostics: DiagnosticAccumulator::new(Instant::now()),
        };
        tasks.spawn(actor.run());
    }
    drop(updates_tx);
    let startup_deadline = Instant::now() + Duration::from_secs(30);
    let mut startup = true;
    let mut stopping = false;
    while !tasks.is_empty() {
        if *stop.borrow() && !stopping {
            stopping = true;
            status.send_modify(|snapshot| snapshot.phase = RuntimePhase::Stopping);
        }
        tokio::select! {
            _ = stop.changed(), if !stopping => {},
            _ = tokio::time::sleep_until(startup_deadline), if startup && !stopping => {
                if !failures.contains(&RuntimeFailureCode::StartupTimeout) { failures.push(RuntimeFailureCode::StartupTimeout); }
                stop_tx.send_replace(true);
            }
            Some(request) = resumes.recv() => {
                let key = direction_key(request.direction);
                let paused = status.borrow().directions.iter().any(|d| d.direction == request.direction && d.phase == DirectionPhase::Paused);
                if stopping || !paused {
                    pending_resumes.lock().unwrap()[key as usize] = false;
                    let _ = request.result.send(Err(AppError::InvalidRequest("direction cannot resume".into())));
                } else if let Some(control) = controls.get(&key) {
                    if let Err(error) = control.try_send(DirectionControl::Resume(request.result)) {
                        pending_resumes.lock().unwrap()[key as usize] = false;
                        let DirectionControl::Resume(result) = error.into_inner();
                        let _ = result.send(Err(AppError::InvalidRequest("direction is busy or stopped".into())));
                    }
                } else {
                    pending_resumes.lock().unwrap()[key as usize] = false;
                    let _ = request.result.send(Err(AppError::InvalidRequest("direction is not part of runtime".into())));
                }
            }
            Some(update) = updates.recv() => match update {
                ActorUpdate::ResumeComplete { direction, result, response } => {
                    pending_resumes.lock().unwrap()[direction_key(direction) as usize] = false;
                    let _ = result.send(response);
                },
                ActorUpdate::Snapshot(direction) => {
                    if startup && (direction.failure.is_some() || matches!(direction.phase, DirectionPhase::Paused | DirectionPhase::Stopped)) {
                        stop_tx.send_replace(true);
                    }
                    status.send_modify(|snapshot| {
                        if let Some(current) = snapshot.directions.iter_mut().find(|current| current.direction == direction.direction) { *current = direction; }
                        if startup && snapshot.directions.iter().all(|d| d.phase == DirectionPhase::Running) { startup = false; }
                        if !stopping { snapshot.phase = if startup { RuntimePhase::Starting } else { aggregate_phase(&snapshot.directions) }; }
                    });
                }
            },
            joined = tasks.join_next() => {
                match joined {
                    Some(Ok(exit)) => {
                        for code in exit.failures { if !failures.contains(&code) { failures.push(code); } }
                        status.send_modify(|snapshot| {
                            if let Some(direction) = snapshot.directions.iter_mut().find(|d| d.direction == exit.direction) { direction.phase = DirectionPhase::Stopped; }
                        });
                    }
                    Some(Err(_)) => if !failures.contains(&RuntimeFailureCode::CleanupTimeout) { failures.push(RuntimeFailureCode::CleanupTimeout); },
                    None => break,
                }
                if startup { stop_tx.send_replace(true); }
            }
        }
    }
    resumes.close();
    let outcome = if failures.is_empty() {
        RuntimeOutcome::Completed
    } else {
        RuntimeOutcome::Failed { codes: failures }
    };
    status.send_modify(|snapshot| {
        snapshot.phase = RuntimePhase::Stopped;
        for direction in &mut snapshot.directions {
            direction.phase = DirectionPhase::Stopped;
        }
        snapshot.outcome = Some(outcome.clone());
    });
    completion.send_replace(Some(outcome));
}

fn prepare_audio(
    inventory: &AudioInventory,
    launch: &SessionLaunch,
) -> AppResult<(Vec<RuntimeEndpoint>, Vec<DirectionPlan>, AudioGraph)> {
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
            let graph = AudioGraph::for_live(input.endpoint.clone(), output.endpoint.clone())?;
            router_direction(SessionMode::Live, launch.config.direction())?;
            Ok((
                vec![input.clone(), output.clone()],
                vec![DirectionPlan {
                    direction: launch.config.direction(),
                    endpoints: vec![input, output],
                    request: AudioOpenRequest {
                        inventory_generation: inventory.generation,
                        input_id: input_id.clone(),
                        output_id: m1_output_id.clone(),
                        source: AudioSource::Microphone,
                    },
                }],
                graph,
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
            let graph = AudioGraph::for_call(
                a.endpoint.clone(),
                m2.endpoint.clone(),
                m1.endpoint.clone(),
                physical.endpoint.clone(),
            )?;
            Ok((
                vec![a.clone(), m2.clone(), m1.clone(), physical.clone()],
                vec![
                    DirectionPlan {
                        direction: SessionDirection::SourceToTarget,
                        endpoints: vec![a, m1],
                        request: AudioOpenRequest {
                            inventory_generation: inventory.generation,
                            input_id: a_input_id.clone(),
                            output_id: m1_output_id.clone(),
                            source: AudioSource::Microphone,
                        },
                    },
                    DirectionPlan {
                        direction: SessionDirection::TargetToSource,
                        endpoints: vec![m2, physical],
                        request: AudioOpenRequest {
                            inventory_generation: inventory.generation,
                            input_id: m2_input_id.clone(),
                            output_id: physical_output_id.clone(),
                            source: AudioSource::System,
                        },
                    },
                ],
                graph,
            ))
        }
        _ => Err(AppError::InvalidRequest(
            "session mode and device selection do not match".into(),
        )),
    }
}

pub(super) fn validate_request(inventory: &AudioInventory, plan: &DirectionPlan) -> AppResult<()> {
    if inventory.driver != DriverStatus::Ready {
        return Err(AppError::Audio {
            code: RuntimeFailureCode::AudioBackendUnavailable,
            message: "audio driver is not ready",
        });
    }
    for retained in &plan.endpoints {
        let id = &retained.endpoint.id;
        let direction = retained.endpoint.direction.clone();
        let endpoint = inventory
            .endpoints
            .iter()
            .find(|endpoint| &endpoint.endpoint.id == id)
            .ok_or_else(|| AppError::InvalidRequest("selected endpoint is missing".into()))?;
        if endpoint.endpoint.direction != direction
            || endpoint.role != retained.role
            || !endpoint.endpoint.ready
        {
            return Err(AppError::InvalidRequest(
                "selected endpoint is unavailable".into(),
            ));
        }
    }
    Ok(())
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

pub(super) fn audio_failure(error: &AppError) -> RuntimeFailureCode {
    match error {
        AppError::Audio { code, .. } => code.clone(),
        AppError::InvalidRequest(_) => RuntimeFailureCode::InvalidEvent,
        _ => RuntimeFailureCode::DeviceUnavailable,
    }
}

pub(super) fn runtime_error(code: RuntimeFailureCode) -> AppError {
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

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}
