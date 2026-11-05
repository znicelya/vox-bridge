use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    domain::session::{SessionConfig, SessionDirection, SessionMode},
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

#[derive(Clone, Debug)]
struct ActiveSession {
    record: SessionRecord,
}

#[async_trait]
pub trait SessionStartup: Send + Sync {
    async fn start(&self, config: &SessionConfig) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct NoopSessionStartup;

#[async_trait]
impl SessionStartup for NoopSessionStartup {
    async fn start(&self, _config: &SessionConfig) -> AppResult<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct SessionOrchestrator {
    store: Arc<LocalStore>,
    event_bus: EventBus,
    active: Arc<RwLock<HashMap<Uuid, ActiveSession>>>,
    startup: Arc<dyn SessionStartup>,
}

impl std::fmt::Debug for SessionOrchestrator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionOrchestrator")
            .finish_non_exhaustive()
    }
}

impl SessionOrchestrator {
    pub fn new(store: Arc<LocalStore>, event_bus: EventBus) -> Self {
        Self {
            store,
            event_bus,
            active: Arc::new(RwLock::new(HashMap::new())),
            startup: Arc::new(NoopSessionStartup),
        }
    }
    pub fn with_startup(
        store: Arc<LocalStore>,
        event_bus: EventBus,
        startup: Arc<dyn SessionStartup>,
    ) -> Self {
        Self {
            store,
            event_bus,
            active: Arc::new(RwLock::new(HashMap::new())),
            startup,
        }
    }
    pub fn store(&self) -> &Arc<LocalStore> {
        &self.store
    }
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub async fn start(&self, config: SessionConfig) -> AppResult<SessionHandle> {
        let id = Uuid::new_v4();
        let now = now_seconds();
        let mut record = SessionRecord {
            id,
            started_at: now,
            ended_at: None,
            mode: config.mode(),
            direction: config.direction(),
            source_language: config.languages().source().to_string(),
            target_language: config.languages().target().to_string(),
            status: SessionStatus::Starting,
        };
        self.store.create_session(&record.to_storage())?;
        self.store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: id,
                status: SessionStatus::Starting.as_str().into(),
                ended_at: None,
            })?;

        if let Err(error) = self.startup.start(&config).await {
            let message = error.to_string();
            let failed = self
                .store
                .update_session_lifecycle(&StoredSessionLifecycle {
                    session_id: id,
                    status: SessionStatus::Failed.as_str().into(),
                    ended_at: Some(now_seconds()),
                });
            if let Err(storage_error) = failed {
                return Err(storage_error);
            }
            let _ = self.event_bus.publish(
                id,
                SessionEvent::Error {
                    event_id: Uuid::new_v4(),
                    session_id: id,
                    occurred_at: SystemTime::now(),
                    message,
                },
            );
            self.event_bus.close(id);
            return Err(error);
        }

        record.status = SessionStatus::Running;
        self.store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: id,
                status: SessionStatus::Running.as_str().into(),
                ended_at: None,
            })?;
        self.active.write().await.insert(
            id,
            ActiveSession {
                record: record.clone(),
            },
        );
        let _ = self
            .event_bus
            .publish(id, SessionEvent::status(id, "running"));
        Ok(SessionHandle {
            id,
            orchestrator: self.clone(),
        })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Option<SessionRecord>> {
        let session = self.store.get_session(id)?;
        let Some(session) = session else {
            return Ok(None);
        };
        let lifecycle = self
            .store
            .get_session_lifecycle(id)?
            .ok_or(AppError::SessionNotFound(id))?;
        Ok(Some(SessionRecord::from_storage(session, lifecycle)?))
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
        let mut active = self.active.write().await;
        let session = active.get_mut(&id).ok_or(AppError::SessionNotFound(id))?;
        let mut record = session.record.clone();
        record.status = SessionStatus::Stopping;
        if let Err(error) = self
            .store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: id,
                status: SessionStatus::Stopping.as_str().into(),
                ended_at: None,
            })
        {
            self.emit_storage_error(id, &error);
            return Err(error);
        }
        session.record = record.clone();
        record.status = SessionStatus::Completed;
        record.ended_at = Some(now_seconds());
        if let Err(error) = self
            .store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: id,
                status: SessionStatus::Completed.as_str().into(),
                ended_at: record.ended_at,
            })
        {
            self.emit_storage_error(id, &error);
            return Err(error);
        }
        active.remove(&id);
        let _ = self
            .event_bus
            .publish(id, SessionEvent::status(id, "completed"));
        self.event_bus.close(id);
        Ok(record)
    }

    pub async fn delete_session(&self, id: Uuid) -> AppResult<()> {
        let active = self.active.read().await;
        if active.contains_key(&id) {
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
        let active = self.active.read().await;
        let session = active.get(&id).ok_or(AppError::SessionNotFound(id))?;
        if matches!(
            session.record.status,
            SessionStatus::Completed | SessionStatus::Failed
        ) {
            return Err(AppError::InvalidRequest(
                "cannot subscribe to a terminal session".into(),
            ));
        }
        let receiver = self
            .event_bus
            .try_subscribe(id)
            .ok_or_else(|| AppError::Internal("session event stream is closed".into()))?;
        let _ = self
            .event_bus
            .publish(id, SessionEvent::status(id, session.record.status.as_str()));
        Ok(receiver)
    }

    fn emit_storage_error(&self, id: Uuid, error: &AppError) {
        let _ = self.event_bus.publish(
            id,
            SessionEvent::Error {
                event_id: Uuid::new_v4(),
                session_id: id,
                occurred_at: SystemTime::now(),
                message: format!("session persistence failed: {error}"),
            },
        );
    }
}

pub struct SessionHandle {
    id: Uuid,
    orchestrator: SessionOrchestrator,
}
impl SessionHandle {
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub async fn stop(self) -> AppResult<SessionRecord> {
        self.orchestrator.stop_session(self.id).await
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
