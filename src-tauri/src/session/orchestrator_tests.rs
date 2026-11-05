use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    domain::session::{LanguagePair, SessionConfig, SessionDirection},
    events::EventBus,
    storage::LocalStore,
};

use super::orchestrator::{SessionOrchestrator, SessionStartup};
use crate::error::{AppError, AppResult};

fn fixture_orchestrator() -> SessionOrchestrator {
    SessionOrchestrator::new(Arc::new(LocalStore::in_memory().unwrap()), EventBus::new())
}

#[derive(Debug)]
struct FailingStartup {
    called: Arc<AtomicBool>,
}

#[async_trait]
impl SessionStartup for FailingStartup {
    async fn start(&self, _config: &SessionConfig) -> AppResult<()> {
        self.called.store(true, Ordering::SeqCst);
        Err(AppError::Internal("provider unavailable".into()))
    }
}

fn fixture_pair() -> LanguagePair {
    LanguagePair::new("en", "fr").unwrap()
}

#[tokio::test]
async fn startup_failure_persists_failed_without_running_event() {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let bus = EventBus::new();
    let called = Arc::new(AtomicBool::new(false));
    let startup = Arc::new(FailingStartup {
        called: called.clone(),
    });
    let orchestrator = SessionOrchestrator::with_startup(store.clone(), bus.clone(), startup);
    let pair = fixture_pair();
    let config = SessionConfig::live(pair, SessionDirection::SourceToTarget).unwrap();
    let result = orchestrator.start(config).await;
    assert!(result.is_err());
    assert!(called.load(Ordering::SeqCst));
    let sessions = store.list_sessions(10, None).unwrap();
    assert_eq!(sessions.len(), 1);
    let lifecycle = store
        .get_session_lifecycle(sessions[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(lifecycle.status, "failed");
}

#[tokio::test]
async fn subscribe_receives_current_status_and_terminal_stream_is_rejected() {
    let orchestrator = fixture_orchestrator();
    let handle = orchestrator
        .start(SessionConfig::live(fixture_pair(), SessionDirection::SourceToTarget).unwrap())
        .await
        .unwrap();
    let id = handle.id();
    let mut receiver = orchestrator.subscribe(id).await.unwrap();
    let event = receiver.recv().await.unwrap();
    assert!(
        matches!(event, crate::events::SessionEvent::Status { status, .. } if status == "running")
    );
    handle.stop().await.unwrap();
    assert!(orchestrator.subscribe(id).await.is_err());
}

#[tokio::test]
async fn deleting_active_or_missing_sessions_is_rejected_and_completed_channels_close() {
    let orchestrator = fixture_orchestrator();
    let handle = orchestrator
        .start(SessionConfig::live(fixture_pair(), SessionDirection::SourceToTarget).unwrap())
        .await
        .unwrap();
    let id = handle.id();
    assert!(orchestrator.delete_session(id).await.is_err());
    handle.stop().await.unwrap();
    orchestrator.delete_session(id).await.unwrap();
    assert!(matches!(
        orchestrator.delete_session(id).await,
        Err(AppError::SessionNotFound(_))
    ));
    assert!(!orchestrator.event_bus().close(id));
}

#[tokio::test]
async fn stopping_a_live_session_persists_end_time_and_status() {
    let orchestrator = fixture_orchestrator();
    let handle = orchestrator
        .start(SessionConfig::live(fixture_pair(), SessionDirection::SourceToTarget).unwrap())
        .await
        .unwrap();

    let record = handle.stop().await.unwrap();

    assert_eq!(record.status, super::orchestrator::SessionStatus::Completed);
    assert!(record.ended_at.is_some());
    let stored = orchestrator
        .store()
        .get_session_lifecycle(record.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "completed");
    assert_eq!(stored.ended_at, record.ended_at);
}
