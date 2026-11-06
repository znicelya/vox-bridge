use super::AppState;

#[test]
fn app_state_initializes_the_event_bus_and_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(state.data_dir(), dir.path());
    assert!(state.event_bus().is_healthy());
}

#[test]
fn app_construction_is_lazy_and_does_not_recover_without_process_ownership() {
    use crate::storage::{LocalStore, StoredSession};
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(dir.path()).unwrap();
    let id = uuid::Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    drop(store);
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        state
            .store()
            .get_session_lifecycle(id)
            .unwrap()
            .unwrap()
            .status,
        "created"
    );
    assert!(!dir.path().join("secrets.json").exists());
    assert!(!dir.path().join("secrets.dek.lock").exists());
    assert!(!dir.path().join("provider-configs.json").exists());
}
