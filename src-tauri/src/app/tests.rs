use super::AppState;

#[test]
fn app_state_initializes_the_event_bus_and_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(state.data_dir(), dir.path());
    assert!(state.event_bus().is_healthy());
}
