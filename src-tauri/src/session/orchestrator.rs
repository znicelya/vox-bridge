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
    storage::{LocalStore, SqlSlot, StoredSession, StoredSessionLifecycle},
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

struct SessionActualOwner {
    slot: Mutex<Option<tokio::sync::OwnedSemaphorePermit>>,
    sql: SqlSlot,
    runtime: Mutex<Option<super::RunningSession>>,
}
impl SessionActualOwner {
    fn complete(&self) {
        self.runtime.lock().unwrap().take();
        self.slot.lock().unwrap().take();
    }
}
struct SessionActualControl {
    slots: Arc<tokio::sync::Semaphore>,
    closing: Mutex<Option<crate::audio::join::AudioCloseKey>>,
    owner: Mutex<Option<Arc<SessionActualOwner>>>,
    finalization: Mutex<Option<TerminalFinalization>>,
}
struct TerminalFinalization {
    lifecycle: StoredSessionLifecycle,
    lifecycle_written: bool,
    diagnostic: Option<crate::domain::transcript::RuntimeDiagnostic>,
    terminal: crate::summary::SessionTerminalPort,
}
#[derive(Clone)]
pub(crate) struct SessionCloseJoin {
    key: crate::audio::join::AudioCloseKey,
    control: Arc<SessionActualControl>,
}
impl SessionCloseJoin {
    pub(crate) fn key(&self) -> &crate::audio::join::AudioCloseKey {
        &self.key
    }
    pub(crate) async fn wait(&self) -> AppResult<()> {
        let _all = self
            .control
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Internal("session actual source closed".into()))?;
        if self.control.closing.lock().unwrap().as_ref() != Some(&self.key) {
            return Err(AppError::InvalidRequest(
                "session actual close epoch changed".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn outcome(&self) -> AppResult<()> {
        if self.control.finalization.lock().unwrap().is_some() {
            return Err(AppError::Internal(
                "original terminal persistence failed; durable recovery remains required".into(),
            ));
        }
        Ok(())
    }
}

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
    actual: Arc<SessionActualOwner>,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut active = self.active.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|operation| operation.id == self.id && operation.attached.is_none())
        {
            *active = None;
            self.actual.complete();
        }
    }
}

#[derive(Clone)]
pub struct SessionOrchestrator {
    store: Arc<LocalStore>,
    event_bus: EventBus,
    active: ActiveState,
    runtime: Arc<super::TranslationRuntime>,
    terminal: Arc<Mutex<crate::summary::SessionTerminalPort>>,
    actual: Arc<SessionActualControl>,
    #[cfg(test)]
    pub(super) phase_write_gate: Arc<Mutex<Option<super::orchestrator_tests::PhaseWriteGate>>>,
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
            terminal: Arc::new(Mutex::new(
                crate::summary::SessionTerminalPort::unavailable(),
            )),
            actual: Arc::new(SessionActualControl {
                slots: Arc::new(tokio::sync::Semaphore::new(1)),
                closing: Mutex::new(None),
                owner: Mutex::new(None),
                finalization: Mutex::new(None),
            }),
            #[cfg(test)]
            phase_write_gate: Arc::new(Mutex::new(None)),
        }
    }

    pub fn store(&self) -> &Arc<LocalStore> {
        &self.store
    }
    pub(crate) fn install_terminal_port(&self, terminal: crate::summary::SessionTerminalPort) {
        *self.terminal.lock().unwrap() = terminal;
    }
    pub(crate) fn capture_close_join(
        &self,
        key: crate::audio::join::AudioCloseKey,
    ) -> AppResult<SessionCloseJoin> {
        let mut closing = self.actual.closing.lock().unwrap();
        if closing.as_ref().is_some_and(|old| old != &key) {
            return Err(AppError::InvalidRequest(
                "session actual close epoch changed".into(),
            ));
        }
        *closing = Some(key.clone());
        Ok(SessionCloseJoin {
            key,
            control: self.actual.clone(),
        })
    }
    pub(crate) fn reopen_after_close(
        &self,
        key: &crate::audio::join::AudioCloseKey,
    ) -> AppResult<()> {
        self.validate_reopen_after_close(key)?;
        let mut closing = self.actual.closing.lock().unwrap();
        if closing.as_ref() != Some(key)
            || self.actual.slots.available_permits() != 1
            || self.actual.finalization.lock().unwrap().is_some()
        {
            return Err(AppError::InvalidRequest(
                "session actual close remains pending".into(),
            ));
        }
        *closing = None;
        Ok(())
    }
    pub(crate) fn validate_reopen_after_close(
        &self,
        key: &crate::audio::join::AudioCloseKey,
    ) -> AppResult<()> {
        if self.actual.closing.lock().unwrap().as_ref() != Some(key)
            || self.actual.slots.available_permits() != 1
            || self.actual.finalization.lock().unwrap().is_some()
        {
            return Err(AppError::InvalidRequest(
                "session actual close or terminal effects remain pending".into(),
            ));
        }
        Ok(())
    }
    /// Same-key recovery of the original failed durable effects. No active/status
    /// shortcut can prove a missing lifecycle or diagnostic INSERT completed.
    pub(crate) fn retry_terminal_close_effects(
        &self,
        key: &crate::audio::join::AudioCloseKey,
    ) -> AppResult<()> {
        if self.actual.closing.lock().unwrap().as_ref() != Some(key)
            || self.actual.slots.available_permits() != 1
        {
            return Err(AppError::InvalidRequest(
                "original terminal producer remains pending".into(),
            ));
        }
        let mut pending = self.actual.finalization.lock().unwrap();
        let finalization = pending.as_mut().ok_or_else(|| {
            AppError::Internal("original audio failure has no durable recovery producer".into())
        })?;
        if !finalization.lifecycle_written {
            let reservation = finalization.terminal.try_reserve();
            self.store
                .commit_terminal_lifecycle(&finalization.lifecycle, reservation.intent())?;
            finalization.lifecycle_written = true;
            reservation.committed(finalization.lifecycle.session_id);
        }
        let lifecycle_complete = self.store.transaction(|tx| {
            Ok(tx.query_row(
                "SELECT status=?2 AND ended_at IS ?3 FROM session_lifecycle WHERE session_id=?1",
                rusqlite::params![
                    finalization.lifecycle.session_id.to_string(),
                    finalization.lifecycle.status,
                    finalization.lifecycle.ended_at
                ],
                |row| row.get::<_, bool>(0),
            )?)
        })?;
        if !lifecycle_complete {
            return Err(AppError::Intelligence(
                crate::intelligence::types::IntelligenceErrorCode::Storage,
            ));
        }
        if let Some(diagnostic) = &finalization.diagnostic {
            let expected = serde_json::to_string(diagnostic)
                .map_err(|_| AppError::Internal("terminal diagnostic encoding failed".into()))?;
            let existing = self.store.transaction(|tx| {
                use rusqlite::OptionalExtension;
                Ok(tx.query_row("SELECT session_id=?2 AND payload_json=?3 FROM session_diagnostics WHERE id=?1",
                    rusqlite::params![diagnostic.id.to_string(),diagnostic.session_id.to_string(),expected], |row| row.get::<_,bool>(0)).optional()?)
            })?;
            match existing {
                Some(true) => {}
                Some(false) => {
                    return Err(AppError::Intelligence(
                        crate::intelligence::types::IntelligenceErrorCode::Storage,
                    ));
                }
                None => self.store.append_diagnostic(diagnostic)?,
            }
        }
        *pending = None;
        Ok(())
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
            let closing = self.actual.closing.lock().unwrap();
            if closing.is_some() {
                return Err(AppError::InvalidRequest(
                    "session admission is closing".into(),
                ));
            }
            if self.actual.finalization.lock().unwrap().is_some() {
                return Err(AppError::InvalidRequest(
                    "prior terminal persistence requires close recovery".into(),
                ));
            }
            let slot = self.actual.slots.clone().try_acquire_owned().map_err(|_| {
                AppError::InvalidRequest("prior terminal producer remains owned".into())
            })?;
            let actual = Arc::new(SessionActualOwner {
                slot: Mutex::new(Some(slot)),
                sql: SqlSlot::default(),
                runtime: Mutex::new(None),
            });
            *self.actual.owner.lock().unwrap() = Some(actual.clone());
            drop(closing);
            let id = Uuid::new_v4();
            *active = Some(SessionOperation {
                id,
                provider_config_id: launch.provider_config_id,
                attached: None,
            });
            Reservation {
                active: self.active.clone(),
                id,
                actual,
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
        *reservation.actual.runtime.lock().unwrap() = Some(running.clone());
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
        let terminal = self.terminal.lock().unwrap().clone();
        let actual = reservation.actual.clone();
        tokio::spawn(async move {
            owner
                .observe(initial, running, stop_rx, completion, terminal, actual)
                .await;
        });
        Ok(record)
    }

    /// Includes unattached preparation reservations and cleanup ownership.
    pub(crate) fn has_active_operation(&self) -> bool {
        self.active.lock().unwrap().is_some()
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
        terminal: crate::summary::SessionTerminalPort,
        actual: Arc<SessionActualOwner>,
    ) {
        let mut status = running.status();
        let mut storage_failed = false;
        let outcome = loop {
            if *stop.borrow_and_update() {
                if let Some(snapshot) = &mut record.runtime {
                    snapshot.phase = RuntimePhase::Stopping;
                }
                if self
                    .persist_phase(&mut record, SessionStatus::Stopping, &actual)
                    .await
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
            record.runtime = Some(snapshot);
            if self
                .persist_phase(&mut record, phase, &actual)
                .await
                .is_err()
            {
                storage_failed = true;
                break running.stop().await;
            }
            tokio::select! {
                biased;
                _ = stop.changed() => {},
                changed = status.changed() => {
                    if changed.is_err() { break running.stop().await; }
                }
            }
        };
        let joined = running.join_actual().await.is_ok();
        storage_failed |= !joined;
        record.runtime = None;
        record.ended_at = Some(now_seconds());
        record.status = if storage_failed || matches!(outcome, RuntimeOutcome::Failed { .. }) {
            SessionStatus::Failed
        } else {
            SessionStatus::Completed
        };
        let lifecycle = StoredSessionLifecycle {
            session_id: record.id,
            status: record.status.as_str().into(),
            ended_at: record.ended_at,
        };
        let reservation = terminal.try_reserve();
        let storage = self.store.clone();
        let terminal_lifecycle = lifecycle.clone();
        let terminal_result = actual
            .sql
            .run(actual.clone(), move || {
                let persisted = storage
                    .commit_terminal_lifecycle(&terminal_lifecycle, reservation.intent())
                    .is_ok();
                Ok((persisted, reservation))
            })
            .await;
        let (persisted, reservation) = match terminal_result {
            Ok((persisted, reservation)) => (persisted, Some(reservation)),
            Err(_) => (false, None),
        };
        storage_failed |= !persisted;
        let failure = if storage_failed {
            Some(RuntimeFailureCode::Storage)
        } else if let RuntimeOutcome::Failed { codes } = outcome {
            codes.into_iter().next()
        } else {
            None
        };
        let mut final_diagnostic = None;
        let final_event = if let Some(code) = failure {
            let event = runtime_failure_event(record.id, code);
            let SessionEvent::Diagnostic { diagnostic } = &event else {
                unreachable!("runtime failure is a diagnostic");
            };
            final_diagnostic = Some(diagnostic.clone());
            let stored_diagnostic = diagnostic.clone();
            let storage = self.store.clone();
            if actual
                .sql
                .run(actual.clone(), move || {
                    storage.append_diagnostic(&stored_diagnostic)
                })
                .await
                .is_err()
            {
                storage_failed = true;
                runtime_failure_event(record.id, RuntimeFailureCode::Storage)
            } else {
                event
            }
        } else {
            SessionEvent::status(record.id, record.status.as_str())
        };
        let sql_joined = actual.sql.drain().await.is_ok();
        storage_failed |= !sql_joined;
        self.event_bus.close_with_final(record.id, final_event);
        if storage_failed {
            *self.actual.finalization.lock().unwrap() = Some(TerminalFinalization {
                lifecycle,
                lifecycle_written: persisted,
                diagnostic: final_diagnostic,
                terminal: terminal.clone(),
            });
        }
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
        drop(active);
        if persisted {
            if let Some(reservation) = reservation {
                reservation.committed(record.id);
            }
        }
        if joined && sql_joined {
            actual.complete();
        }
    }

    async fn persist_phase(
        &self,
        record: &mut SessionRecord,
        status: SessionStatus,
        actual: &Arc<SessionActualOwner>,
    ) -> AppResult<()> {
        let status_changed = record.status != status;
        if status_changed {
            let storage = self.store.clone();
            let lifecycle = StoredSessionLifecycle {
                session_id: record.id,
                status: status.as_str().into(),
                ended_at: None,
            };
            #[cfg(test)]
            let gate = self.phase_write_gate.clone();
            actual
                .sql
                .run(actual.clone(), move || {
                    #[cfg(test)]
                    if status == SessionStatus::Running {
                        let gate = gate.lock().unwrap().take();
                        if let Some(gate) = gate {
                            gate.wait();
                        }
                    }
                    storage.update_session_lifecycle(&lifecycle)
                })
                .await?;
            record.status = status;
        }
        let mut active = self.active.lock().unwrap();
        if let Some(session) = active
            .as_mut()
            .filter(|op| op.id == record.id)
            .and_then(|op| op.attached.as_mut())
        {
            // This check, active snapshot update and event publication share
            // stop_session's election mutex. If stop won during persistence,
            // suppress the stale phase; the observer next persists Stopping and
            // drives cleanup. No SQL or runtime wait occurs in this section.
            if status != SessionStatus::Stopping && *session.stop.borrow() {
                return Ok(());
            }
            session.record = record.clone();
            if status_changed {
                let _ = self
                    .event_bus
                    .publish(record.id, SessionEvent::status(record.id, status.as_str()));
            }
        }
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Option<SessionRecord>> {
        // The attached observer publishes only after its durable write and
        // stop fence. A yielding terminal write can already be visible in SQL
        // while its diagnostic/bus close is still pending; keep callers on the
        // last committed observer view until that original producer retires.
        if let Some(record) = self
            .active
            .lock()
            .unwrap()
            .as_ref()
            .filter(|operation| operation.id == id)
            .and_then(|operation| operation.attached.as_ref())
            .map(|session| session.record.clone())
        {
            return Ok(Some(record));
        }
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

    /// Consult the authoritative active operation. Pending preparation is
    /// cancelled and joined by AppState's admission barrier before its final pass.
    /// Copy the ID under the mutex; existing stop retains observer completion.
    pub(crate) async fn stop_active(&self) -> AppResult<()> {
        let id = self
            .active
            .lock()
            .unwrap()
            .as_ref()
            .filter(|operation| operation.attached.is_some())
            .map(|operation| operation.id);
        if let Some(id) = id {
            self.stop_session(id).await?;
        }
        Ok(())
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
                        session.runtime.request_stop();
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

    /// Nonmutating eligibility check for guarded app cleanup before deletion.
    /// SQLite reads occur after the authoritative active-state mutex is released.
    pub(crate) fn ensure_session_deletable(&self, id: Uuid) -> AppResult<()> {
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
        Ok(())
    }

    pub async fn delete_session(&self, id: Uuid) -> AppResult<()> {
        self.ensure_session_deletable(id)?;
        self.store.delete_session(id)?;
        self.event_bus.close(id);
        Ok(())
    }
    pub(crate) async fn delete_session_guarded(
        &self,
        id: Uuid,
        guard: crate::summary::SummarySessionDeleteGuard,
    ) -> AppResult<()> {
        self.ensure_session_deletable(id)?;
        guard.delete_owned(id).await?;
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
