use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    config::ProviderConfig,
    domain::{
        launch::{
            DirectionPhase, DirectionSnapshot, RuntimeOutcome, RuntimePhase, RuntimeSnapshot,
            SessionContext, SessionLaunch,
        },
        session::{SessionDirection, SessionMode},
        transcript::RuntimeFailureCode,
    },
    error::{AppError, AppResult},
    events::{EventBus, SessionEvent},
    storage::{LocalStore, StoredSession, StoredSessionLifecycle},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Created,
    Starting,
    Running,
    Stopping,
    Paused,
    Failed,
    Completed,
}

impl SessionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Completed => "completed",
        }
    }
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "created" => Ok(Self::Created),
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "stopping" => Ok(Self::Stopping),
            "paused" => Ok(Self::Paused),
            "failed" => Ok(Self::Failed),
            "completed" => Ok(Self::Completed),
            other => Err(AppError::Internal(format!(
                "unknown session status: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: Uuid,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub mode: SessionMode,
    pub direction: SessionDirection,
    pub source_language: String,
    pub target_language: String,
    pub status: SessionStatus,
    pub context: Option<SessionContext>,
    pub runtime: Option<RuntimeSnapshot>,
}

impl SessionRecord {
    fn from_storage(session: StoredSession, lifecycle: StoredSessionLifecycle) -> AppResult<Self> {
        let mode = match session.mode.as_str() {
            "live" => SessionMode::Live,
            "call" => SessionMode::Call,
            other => return Err(AppError::Internal(format!("unknown session mode: {other}"))),
        };
        let direction = match session.direction.as_str() {
            "source_to_target" => SessionDirection::SourceToTarget,
            "target_to_source" => SessionDirection::TargetToSource,
            "bidirectional" => SessionDirection::Bidirectional,
            other => {
                return Err(AppError::Internal(format!(
                    "unknown session direction: {other}"
                )));
            }
        };
        Ok(Self {
            id: session.id,
            started_at: session.started_at,
            ended_at: lifecycle.ended_at,
            mode,
            direction,
            source_language: session.source_language,
            target_language: session.target_language,
            status: SessionStatus::parse(&lifecycle.status)?,
            context: None,
            runtime: None,
        })
    }
    fn to_storage(&self) -> StoredSession {
        StoredSession {
            id: self.id,
            started_at: self.started_at,
            mode: match self.mode {
                SessionMode::Live => "live",
                SessionMode::Call => "call",
            }
            .into(),
            direction: match self.direction {
                SessionDirection::SourceToTarget => "source_to_target",
                SessionDirection::TargetToSource => "target_to_source",
                SessionDirection::Bidirectional => "bidirectional",
            }
            .into(),
            source_language: self.source_language.clone(),
            target_language: self.target_language.clone(),
        }
    }
}

// A single short-held mutex governs admission and stop election. Preparation and
// cleanup are never performed while holding it.
type ActiveState = Arc<Mutex<Option<SessionOperation>>>;
type SessionCompletion = Option<Result<SessionRecord, String>>;

struct SessionOperation {
    id: Uuid,
    provider_config_id: Uuid,
    attached: Option<ActiveSession>,
}

struct ActiveSession {
    record: SessionRecord,
    runtime: super::RunningSession,
    stop: watch::Sender<bool>,
    completion: watch::Receiver<SessionCompletion>,
}

struct Reservation {
    active: ActiveState,
    id: Uuid,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut active = self.active.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|operation| operation.id == self.id && operation.attached.is_none())
        {
            *active = None;
        }
    }
}

#[derive(Clone)]
pub struct SessionOrchestrator {
    store: Arc<LocalStore>,
    event_bus: EventBus,
    active: ActiveState,
    runtime: Arc<super::TranslationRuntime>,
}

impl std::fmt::Debug for SessionOrchestrator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionOrchestrator")
            .finish_non_exhaustive()
    }
}

impl SessionOrchestrator {
    pub fn with_runtime(
        store: Arc<LocalStore>,
        event_bus: EventBus,
        runtime: Arc<super::TranslationRuntime>,
    ) -> Self {
        Self {
            store,
            event_bus,
            active: Arc::new(Mutex::new(None)),
            runtime,
        }
    }

    pub fn store(&self) -> &Arc<LocalStore> {
        &self.store
    }
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub async fn start(
        &self,
        launch: SessionLaunch,
        saved: ProviderConfig,
    ) -> AppResult<SessionRecord> {
        let reservation = {
            let mut active = self.active.lock().unwrap();
            if active.is_some() {
                return Err(AppError::InvalidRequest(
                    "stop the active session first".into(),
                ));
            }
            let id = Uuid::new_v4();
            *active = Some(SessionOperation {
                id,
                provider_config_id: launch.provider_config_id,
                attached: None,
            });
            Reservation {
                active: self.active.clone(),
                id,
            }
        };
        let id = reservation.id;
        let prepared = self.runtime.prepare(id, &launch, &saved).await?;
        let directions = match launch.config.direction() {
            SessionDirection::Bidirectional => vec![
                SessionDirection::SourceToTarget,
                SessionDirection::TargetToSource,
            ],
            direction => vec![direction],
        };
        let record = SessionRecord {
            id,
            started_at: now_seconds(),
            ended_at: None,
            mode: launch.config.mode(),
            direction: launch.config.direction(),
            source_language: launch.config.languages().source().into(),
            target_language: launch.config.languages().target().into(),
            status: SessionStatus::Starting,
            context: Some(prepared.context().clone()),
            // This is the committed initial state. Only the observer exposes
            // newer runtime snapshots, after their lifecycle write succeeds.
            runtime: Some(RuntimeSnapshot {
                phase: RuntimePhase::Starting,
                directions: directions
                    .into_iter()
                    .map(|direction| DirectionSnapshot {
                        direction,
                        phase: DirectionPhase::Starting,
                        stream_epoch: 0,
                        failure: None,
                    })
                    .collect(),
                outcome: None,
            }),
        };
        self.store
            .create_runtime_session(&record.to_storage(), prepared.context())?;
        // No await from durable admission through runtime attachment and observer
        // spawn: dropping a caller can only drop the reservation or the result.
        self.event_bus.open_session(id);
        let _ = self
            .event_bus
            .publish(id, SessionEvent::status(id, "starting"));
        let (stop, stop_rx) = watch::channel(false);
        let (completion, completion_rx) = watch::channel(None);
        let running = self.runtime.start(prepared);
        {
            let mut active = self.active.lock().unwrap();
            active.as_mut().expect("reserved session").attached = Some(ActiveSession {
                record: record.clone(),
                runtime: running.clone(),
                stop,
                completion: completion_rx,
            });
        }
        let owner = self.clone();
        let initial = record.clone();
        tokio::spawn(async move {
            owner.observe(initial, running, stop_rx, completion).await;
        });
        Ok(record)
    }

    pub(crate) async fn uses_provider_config(&self, config_id: Uuid) -> bool {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|operation| operation.provider_config_id == config_id)
    }

    pub async fn resume_direction(&self, id: Uuid, direction: SessionDirection) -> AppResult<()> {
        let runtime = {
            let active = self.active.lock().unwrap();
            let session = active
                .as_ref()
                .filter(|op| op.id == id)
                .and_then(|op| op.attached.as_ref())
                .ok_or(AppError::SessionNotFound(id))?;
            if *session.stop.borrow() {
                return Err(AppError::InvalidRequest("runtime is stopping".into()));
            }
            session.runtime.clone()
        };
        runtime.resume_direction(direction).await
    }

    // The observer alone writes lifecycle transitions and owns cleanup, even if
    // all stop callers are cancelled. Runtime watches may coalesce transitions.
    async fn observe(
        &self,
        mut record: SessionRecord,
        running: super::RunningSession,
        mut stop: watch::Receiver<bool>,
        completion: watch::Sender<SessionCompletion>,
    ) {
        let mut status = running.status();
        let mut storage_failed = false;
        let outcome = loop {
            if *stop.borrow_and_update() {
                if self
                    .persist_phase(&mut record, SessionStatus::Stopping)
                    .is_err()
                {
                    storage_failed = true;
                }
                break running.stop().await;
            }
            let snapshot = status.borrow_and_update().clone();
            if snapshot.phase == RuntimePhase::Stopped {
                break snapshot.outcome.unwrap_or(RuntimeOutcome::Failed {
                    codes: vec![RuntimeFailureCode::CleanupTimeout],
                });
            }
            let phase = match snapshot.phase {
                RuntimePhase::Starting => SessionStatus::Starting,
                RuntimePhase::Running => SessionStatus::Running,
                RuntimePhase::Paused => SessionStatus::Paused,
                RuntimePhase::Stopping => SessionStatus::Stopping,
                RuntimePhase::Stopped => unreachable!(),
            };
            if self.persist_phase(&mut record, phase).is_err() {
                storage_failed = true;
                break running.stop().await;
            }
            record.runtime = Some(snapshot);
            self.update_active_record(&record);
            tokio::select! {
                biased;
                _ = stop.changed() => {},
                changed = status.changed() => {
                    if changed.is_err() { break running.stop().await; }
                }
            }
        };
        record.runtime = None;
        record.ended_at = Some(now_seconds());
        record.status = if storage_failed || matches!(outcome, RuntimeOutcome::Failed { .. }) {
            SessionStatus::Failed
        } else {
            SessionStatus::Completed
        };
        let persisted = self
            .store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: record.id,
                status: record.status.as_str().into(),
                ended_at: record.ended_at,
            })
            .is_ok();
        storage_failed |= !persisted;
        let failure = if storage_failed {
            Some(RuntimeFailureCode::Storage)
        } else if let RuntimeOutcome::Failed { codes } = outcome {
            codes.into_iter().next()
        } else {
            None
        };
        let final_event = if let Some(code) = failure {
            let event = runtime_failure_event(record.id, code);
            let SessionEvent::Diagnostic { diagnostic } = &event else {
                unreachable!("runtime failure is a diagnostic");
            };
            if self.store.append_diagnostic(diagnostic).is_err() {
                storage_failed = true;
                runtime_failure_event(record.id, RuntimeFailureCode::Storage)
            } else {
                event
            }
        } else {
            SessionEvent::status(record.id, record.status.as_str())
        };
        self.event_bus.close_with_final(record.id, final_event);
        let result = if storage_failed {
            Err("session lifecycle persistence failed".into())
        } else {
            Ok(record.clone())
        };
        // Publish before removing the operation. Attached receivers retain this
        // result; later calls consult durable state rather than an error cache.
        let mut active = self.active.lock().unwrap();
        completion.send_replace(Some(result));
        if active.as_ref().is_some_and(|op| op.id == record.id) {
            *active = None;
        }
    }

    fn persist_phase(&self, record: &mut SessionRecord, status: SessionStatus) -> AppResult<()> {
        if record.status == status {
            return Ok(());
        }
        self.store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: record.id,
                status: status.as_str().into(),
                ended_at: None,
            })?;
        record.status = status;
        self.update_active_record(record);
        let _ = self
            .event_bus
            .publish(record.id, SessionEvent::status(record.id, status.as_str()));
        Ok(())
    }

    fn update_active_record(&self, record: &SessionRecord) {
        let mut active = self.active.lock().unwrap();
        if let Some(session) = active
            .as_mut()
            .filter(|op| op.id == record.id)
            .and_then(|op| op.attached.as_mut())
        {
            session.record = record.clone();
        }
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Option<SessionRecord>> {
        let Some(session) = self.store.get_session(id)? else {
            return Ok(None);
        };
        let lifecycle = self
            .store
            .get_session_lifecycle(id)?
            .ok_or(AppError::SessionNotFound(id))?;
        let mut record = SessionRecord::from_storage(session, lifecycle)?;
        record.context = self.store.session_context(id)?;
        if !matches!(
            record.status,
            SessionStatus::Failed | SessionStatus::Completed
        ) {
            record.runtime = self
                .active
                .lock()
                .unwrap()
                .as_ref()
                .filter(|op| op.id == id)
                .and_then(|op| op.attached.as_ref())
                .and_then(|session| session.record.runtime.clone());
        }
        Ok(Some(record))
    }

    pub async fn list(&self, limit: u32, cursor: Option<String>) -> AppResult<Vec<SessionRecord>> {
        let summaries = self.store.list_sessions(limit, cursor)?;
        let mut records = Vec::with_capacity(summaries.len());
        for summary in summaries {
            if let Some(record) = self.get(summary.id).await? {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub async fn stop_session(&self, id: Uuid) -> AppResult<SessionRecord> {
        let completion = {
            let active = self.active.lock().unwrap();
            active
                .as_ref()
                .filter(|op| op.id == id)
                .and_then(|op| op.attached.as_ref())
                .map(|session| {
                    if !*session.stop.borrow() {
                        session.stop.send_replace(true);
                    }
                    session.completion.clone()
                })
        };
        if let Some(mut completion) = completion {
            loop {
                if let Some(result) = completion.borrow_and_update().clone() {
                    return result.map_err(AppError::Internal);
                }
                if completion.changed().await.is_err() {
                    return Err(AppError::Internal(
                        "session lifecycle owner stopped unexpectedly".into(),
                    ));
                }
            }
        }
        let record = self.get(id).await?.ok_or(AppError::SessionNotFound(id))?;
        if matches!(
            record.status,
            SessionStatus::Completed | SessionStatus::Failed
        ) {
            return Ok(record);
        }
        Err(AppError::InvalidRequest(
            "session has no active runtime; lifecycle recovery is required".into(),
        ))
    }

    pub async fn delete_session(&self, id: Uuid) -> AppResult<()> {
        if self
            .active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|op| op.id == id)
        {
            return Err(AppError::InvalidRequest(
                "cannot delete an active session; stop it first".into(),
            ));
        }
        if self.store.get_session(id)?.is_none() {
            return Err(AppError::SessionNotFound(id));
        }
        self.store.delete_session(id)?;
        self.event_bus.close(id);
        Ok(())
    }

    pub async fn subscribe(
        &self,
        id: Uuid,
    ) -> AppResult<tokio::sync::broadcast::Receiver<SessionEvent>> {
        let active = self.active.lock().unwrap();
        if let Some(session) = active
            .as_ref()
            .filter(|op| op.id == id)
            .and_then(|op| op.attached.as_ref())
        {
            if let Some(receiver) = self.event_bus.try_subscribe(id) {
                let _ = self
                    .event_bus
                    .publish(id, SessionEvent::status(id, session.record.status.as_str()));
                return Ok(receiver);
            }
        }
        drop(active);
        if self.store.get_session(id)?.is_none() {
            return Err(AppError::SessionNotFound(id));
        }
        self.event_bus
            .subscribe_or_replay(id)
            .ok_or_else(|| AppError::InvalidRequest("session event stream is unavailable".into()))
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn runtime_failure_event(id: Uuid, code: RuntimeFailureCode) -> SessionEvent {
    use crate::domain::transcript::{DiagnosticPayload, RuntimeDiagnostic};
    SessionEvent::Diagnostic {
        diagnostic: RuntimeDiagnostic {
            id: Uuid::new_v4(),
            session_id: id,
            direction: None,
            stream_epoch: None,
            occurred_at_ms: super::runtime::now_ms(),
            payload: DiagnosticPayload::Failure {
                code,
                provider_code: None,
                log_id: None,
            },
        },
    }
}
