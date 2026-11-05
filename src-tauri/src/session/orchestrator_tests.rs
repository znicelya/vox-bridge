use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

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

#[derive(Debug)]
struct TrackingStartup {
    rolled_back: Arc<AtomicBool>,
}

#[derive(Debug)]
struct BlockingStartup {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl SessionStartup for BlockingStartup {
    async fn start(&self, _config: &SessionConfig) -> AppResult<()> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(())
    }

    async fn rollback(&self, _config: &SessionConfig) -> AppResult<()> {
        Ok(())
    }
}

#[async_trait]
impl SessionStartup for TrackingStartup {
    async fn start(&self, _config: &SessionConfig) -> AppResult<()> {
        Ok(())
    }

    async fn rollback(&self, _config: &SessionConfig) -> AppResult<()> {
        self.rolled_back.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl SessionStartup for FailingStartup {
    async fn start(&self, _config: &SessionConfig) -> AppResult<()> {
        self.called.store(true, Ordering::SeqCst);
        Err(AppError::Internal("provider unavailable".into()))
    }

    async fn rollback(&self, _config: &SessionConfig) -> AppResult<()> {
        Ok(())
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
    let handle = orchestrator.start(config).await.unwrap();
    for _ in 0..20 {
        if called.load(Ordering::SeqCst) {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(called.load(Ordering::SeqCst));
    for _ in 0..20 {
        if matches!(
            orchestrator.get(handle.id()).await.unwrap().unwrap().status,
            super::orchestrator::SessionStatus::Failed
        ) {
            break;
        }
        tokio::task::yield_now().await;
    }
    let sessions = store.list_sessions(10, None).unwrap();
    assert_eq!(sessions.len(), 1);
    let lifecycle = store
        .get_session_lifecycle(sessions[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(lifecycle.status, "failed");
}

#[tokio::test]
async fn startup_failure_error_is_replayable_after_create_returns() {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let bus = EventBus::new();
    let orchestrator = SessionOrchestrator::with_startup(
        store,
        bus,
        Arc::new(FailingStartup {
            called: Arc::new(AtomicBool::new(false)),
        }),
    );
    let handle = orchestrator
        .start(SessionConfig::live(fixture_pair(), SessionDirection::SourceToTarget).unwrap())
        .await
        .unwrap();
    let id = handle.id();
    for _ in 0..20 {
        if matches!(
            orchestrator.get(id).await.unwrap().unwrap().status,
            super::orchestrator::SessionStatus::Failed
        ) {
            break;
        }
        tokio::task::yield_now().await;
    }
    let mut receiver = orchestrator.subscribe(id).await.unwrap();
    assert!(matches!(
        receiver.recv().await.unwrap(),
        crate::events::SessionEvent::Error { .. }
    ));
}

#[tokio::test]
async fn running_persistence_failure_rolls_back_startup_and_persists_failed() {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let bus = EventBus::new();
    let rolled_back = Arc::new(AtomicBool::new(false));
    let startup = Arc::new(TrackingStartup {
        rolled_back: rolled_back.clone(),
    });
    let orchestrator = SessionOrchestrator::with_startup(store.clone(), bus, startup);
    let handle = orchestrator
        .start(SessionConfig::live(fixture_pair(), SessionDirection::SourceToTarget).unwrap())
        .await
        .unwrap();
    store.fail_lifecycle_status_for_tests("running").unwrap();
    for _ in 0..20 {
        if matches!(
            orchestrator.get(handle.id()).await.unwrap().unwrap().status,
            super::orchestrator::SessionStatus::Failed
        ) {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(rolled_back.load(Ordering::SeqCst));
    assert_eq!(
        orchestrator.get(handle.id()).await.unwrap().unwrap().status,
        super::orchestrator::SessionStatus::Failed
    );
}

#[tokio::test]
async fn subscriber_observes_starting_while_startup_is_in_progress() {
    let store = Arc::new(LocalStore::in_memory().unwrap());
    let bus = EventBus::new();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let startup = Arc::new(BlockingStartup {
        entered: entered.clone(),
        release: release.clone(),
    });
    let orchestrator = SessionOrchestrator::with_startup(store.clone(), bus, startup);
    let starting_orchestrator = orchestrator.clone();
    let start_task = tokio::spawn(async move {
        starting_orchestrator
            .start(SessionConfig::live(fixture_pair(), SessionDirection::SourceToTarget).unwrap())
            .await
    });
    entered.notified().await;
    let id = store.list_sessions(1, None).unwrap().pop().unwrap().id;
    let mut receiver = orchestrator.subscribe(id).await.unwrap();
    assert!(
        matches!(receiver.recv().await.unwrap(), crate::events::SessionEvent::Status { status, .. } if status == "starting")
    );
    release.notify_one();
    let handle = start_task.await.unwrap().unwrap();
    let mut running_receiver = orchestrator.subscribe(handle.id()).await.unwrap();
    assert!(matches!(
        running_receiver.recv().await.unwrap(),
        crate::events::SessionEvent::Status { .. }
    ));
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
        matches!(event, crate::events::SessionEvent::Status { status, .. } if status == "starting")
    );
    assert!(matches!(
        receiver.recv().await.unwrap(),
        crate::events::SessionEvent::Status { status, .. } if status == "running"
    ));
    handle.stop().await.unwrap();
    let mut terminal = orchestrator.subscribe(id).await.unwrap();
    assert!(matches!(
        terminal.recv().await.unwrap(),
        crate::events::SessionEvent::Status { status, .. } if status == "completed"
    ));
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
