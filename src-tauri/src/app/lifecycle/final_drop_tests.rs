use super::*;
use crate::{
    app::{CredentialChange, SaveProviderRequest},
    config::SecretStore,
};
use fs2::FileExt;

fn process_probe(dir: &std::path::Path) -> std::fs::File {
    std::fs::File::options()
        .read(true)
        .write(true)
        .open(dir.join("app.lock"))
        .unwrap()
}

#[tokio::test]
async fn window_close_after_joined_reopen_before_intelligence_elects_a_fresh_delivery_key() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.start_intelligence().await.unwrap();
    app.close_all().await.unwrap();
    let prior = app.close_state.lock().unwrap().audio_key.clone().unwrap();
    app.open_window().unwrap();
    app.close_all()
        .await
        .expect("a joined host reopen must permit closing before the next intelligence startup");
    let current = app.close_state.lock().unwrap().audio_key.clone().unwrap();
    assert!(current != prior);
    assert_eq!(current.backend_epoch(), prior.backend_epoch() + 1);
    assert_ne!(current.delivery_id(), prior.delivery_id());
    assert_eq!(h.audio.inventory_calls(), 0);
    assert_eq!(h.audio.open_calls(), 0);
}

#[tokio::test]
async fn final_app_rearmed_close_clears_previous_success_while_inventory_is_pending() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.start_intelligence().await.unwrap();
    app.close_all().await.unwrap();
    let state = app.close_state.clone();
    let previous = {
        let state = state.lock().unwrap();
        assert!(state.close_succeeded);
        state.audio_key.clone().unwrap()
    };
    app.open_window().unwrap();
    let retained = Arc::downgrade(&app._process_lock);
    let (entered, release) = h.audio.block_inventory();
    let observer = tokio::spawn({
        let app = app.clone();
        async move { app.list_audio_endpoints().await }
    });
    entered.await.unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    drop(app);
    let pending_success = {
        let state = state.lock().unwrap();
        assert!(state.closing);
        assert!(state.audio_key.as_ref().unwrap() != &previous);
        assert!(state.audio_ack.is_none());
        state.close_succeeded
    };
    let probe = process_probe(dir.path());
    let held = probe.try_lock_exclusive().is_err();
    if !held {
        probe.unlock().unwrap();
    }
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while retained.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the rearmed final facade must actually join its new inventory source");
    probe.try_lock_exclusive().unwrap();
    probe.unlock().unwrap();
    assert!(held);
    assert!(
        !pending_success,
        "the previous successful close cannot certify the new elected key while actual inventory is pending"
    );
    let state = state.lock().unwrap();
    assert!(state.audio_ack.is_some());
    assert!(
        !state.close_succeeded,
        "a final facade audio/delivery join does not publish complete-app success"
    );
    assert_eq!(h.audio.inventory_calls(), 1);
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.provider.connect_counts(), [0, 0]);
}

#[tokio::test]
async fn final_app_facade_drop_waits_for_native_parent_after_actor_and_active_release() {
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
    app.start_intelligence().await.unwrap();
    let (joined_actor, release_native) = h.audio.delay_actual_join("live-input");
    let session = app.create_session(h.launch.clone()).await.unwrap().id;
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(session)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    app.stop_session(session).await.unwrap();
    joined_actor.await.unwrap();
    assert!(!app.orchestrator.has_active_operation());
    let retained = Arc::downgrade(&app._process_lock);
    drop(app);
    let probe = process_probe(dir.path());
    let held = probe.try_lock_exclusive().is_err();
    if !held {
        probe.unlock().unwrap();
    }
    let _ = release_native.send(());
    tokio::time::timeout(Duration::from_secs(2), async {
        while retained.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the final App facade must eventually join delivery and its original native parent");
    probe.try_lock_exclusive().unwrap();
    probe.unlock().unwrap();
    assert!(
        held,
        "actor completion and active release cannot release app.lock before actual native join"
    );
    assert!(AppState::new(dir.path().to_path_buf()).is_ok());
}

#[tokio::test]
async fn final_app_facade_drop_waits_for_unattached_inventory_after_observer_loss() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let retained = Arc::downgrade(&app._process_lock);
    let (entered, release) = h.audio.block_inventory();
    let observer = tokio::spawn({
        let app = app.clone();
        async move { app.list_audio_endpoints().await }
    });
    entered.await.unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    drop(app);
    let probe = process_probe(dir.path());
    let held = probe.try_lock_exclusive().is_err();
    if !held {
        probe.unlock().unwrap();
    }
    let _ = release.send(());
    tokio::time::timeout(Duration::from_secs(2), async {
        while retained.upgrade().is_some() { tokio::task::yield_now().await; }
    }).await.expect("the final App facade must retain the actual inventory owner through late publication and join");
    probe.try_lock_exclusive().unwrap();
    probe.unlock().unwrap();
    assert!(
        held,
        "a dropped inventory observer cannot release the global process fence"
    );
    assert_eq!(h.audio.inventory_calls(), 1);
    assert_eq!(h.audio.open_calls(), 0);
    assert!(AppState::new(dir.path().to_path_buf()).is_ok());
}

#[test]
fn final_app_facade_inert_drop_without_runtime_uses_empty_actual_sources() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let retained = Arc::downgrade(&app._process_lock);
    drop(app);
    assert!(retained.upgrade().is_none());
    let probe = process_probe(dir.path());
    probe.try_lock_exclusive().unwrap();
    probe.unlock().unwrap();
    assert_eq!(h.audio.inventory_calls(), 0);
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.provider.connect_counts(), [0, 0]);
    assert!(AppState::new(dir.path().to_path_buf()).is_ok());
}
