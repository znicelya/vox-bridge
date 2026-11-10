use super::*;
use crate::{
    app::{CredentialChange, SaveProviderRequest},
    config::SecretStore,
};

#[tokio::test]
async fn window_close_keeps_the_original_native_parent_after_stop_and_observer_cancellation() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    assert!(
        app.create_session(h.launch.clone()).await.is_err(),
        "unready delivery must fence runtime admission"
    );
    app.start_intelligence().await.unwrap();
    let (native_finished, release) = h.audio.delay_actual_join("live-input");
    let record = app.create_session(h.launch.clone()).await.unwrap();
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(record.id)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    let stopped = app.stop_session(record.id).await.unwrap();
    native_finished.await.unwrap();
    assert_eq!(stopped.status, crate::session::SessionStatus::Completed);
    assert!(!app.orchestrator.has_active_operation());
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(30));
    let bounded = app.close_all().await.is_err();
    let key = app.close_state.lock().unwrap().audio_key.clone().unwrap();
    let fenced = app.open_window().is_err();
    let observer = tokio::spawn({
        let app = app.clone();
        async move { app.close_all().await }
    });
    tokio::task::yield_now().await;
    observer.abort();
    let _ = observer.await;
    let same_key = app.close_state.lock().unwrap().audio_key.as_ref() == Some(&key);
    let _ = release.send(());
    tokio::time::timeout(Duration::from_secs(2), app.close_all())
        .await
        .unwrap()
        .unwrap();
    let ack_key = app
        .close_state
        .lock()
        .unwrap()
        .audio_ack
        .as_ref()
        .unwrap()
        .key()
        .clone();
    app.reopen_window().await.unwrap();
    app.close_all().await.unwrap();
    assert!(bounded && fenced && same_key);
    assert!(
        ack_key == key,
        "retry must observe the original elected audio key and delivery UUID"
    );
}

#[tokio::test]
async fn window_close_covers_unattached_inventory_and_rejects_late_publication() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let (entered, release) = h.audio.block_inventory();
    let inventory = tokio::spawn({
        let app = app.clone();
        async move { app.list_audio_endpoints().await }
    });
    entered.await.unwrap();
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(30));
    let bounded = app.close_all().await.is_err();
    let rejected =
        app.list_audio_endpoints().await.is_err() && app.audio_driver_status().await.is_err();
    let _ = release.send(());
    let late = inventory.await.unwrap();
    app.close_all().await.unwrap();
    app.reopen_window().await.unwrap();
    assert!(app.list_audio_endpoints().await.is_ok());
    app.close_all().await.unwrap();
    assert!(
        bounded && rejected && late.is_err(),
        "actual inventory entry and same-epoch publication belong to window close even without an active session"
    );
}
