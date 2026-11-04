use std::sync::Arc;

use crate::{
    domain::session::{LanguagePair, SessionConfig, SessionDirection},
    events::EventBus,
    storage::LocalStore,
};

use super::orchestrator::SessionOrchestrator;

fn fixture_orchestrator() -> SessionOrchestrator {
    SessionOrchestrator::new(Arc::new(LocalStore::in_memory().unwrap()), EventBus::new())
}

fn fixture_pair() -> LanguagePair {
    LanguagePair::new("en", "fr").unwrap()
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
