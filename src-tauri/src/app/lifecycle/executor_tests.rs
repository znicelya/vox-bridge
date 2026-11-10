use super::*;
use crate::{
    app::{CredentialChange, SaveProviderRequest},
    config::SecretStore,
};
use fs2::FileExt;

fn executor() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
fn drop_off_executor(app: AppState) {
    std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        drop(app);
    })
    .join()
    .unwrap();
}
fn probe(dir: &std::path::Path) -> std::fs::File {
    std::fs::File::options()
        .read(true)
        .write(true)
        .open(dir.join("app.lock"))
        .unwrap()
}

#[test]
fn final_app_off_executor_drop_joins_held_native_on_the_original_live_executor() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let retained = Arc::downgrade(&app._process_lock);
    let runtime = executor();
    let (ended, release) = h.audio.delay_actual_join("live-input");
    runtime.block_on(async {
        app.save_provider_config(SaveProviderRequest {
            config: h.saved.clone(),
            credential: CredentialChange::Keep,
        })
        .await
        .unwrap();
        app.start_intelligence().await.unwrap();
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
        ended.await.unwrap();
        assert!(!app.orchestrator.has_active_operation());
    });
    drop_off_executor(app);
    let probe = probe(dir.path());
    let held = probe.try_lock_exclusive().is_err();
    if !held {
        probe.unlock().unwrap();
    }
    release.send(()).unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while retained.upgrade().is_some() { tokio::task::yield_now().await; }
        }).await
    }).expect("final drop outside Tokio must join the original entered native owner on its live executor");
    probe.try_lock_exclusive().unwrap();
    probe.unlock().unwrap();
    assert!(held);
}

#[test]
fn final_app_off_executor_drop_captures_first_inventory_executor_after_inert_construction() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    assert!(tokio::runtime::Handle::try_current().is_err());
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let retained = Arc::downgrade(&app._process_lock);
    let runtime = executor();
    let (entered, release) = h.audio.block_inventory();
    let observer = runtime.spawn({
        let app = app.clone();
        async move { app.list_audio_endpoints().await }
    });
    runtime.block_on(entered).unwrap();
    observer.abort();
    assert!(runtime.block_on(observer).unwrap_err().is_cancelled());
    drop_off_executor(app);
    let probe = probe(dir.path());
    let held = probe.try_lock_exclusive().is_err();
    if !held {
        probe.unlock().unwrap();
    }
    release.send(()).unwrap();
    runtime
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while retained.upgrade().is_some() {
                    tokio::task::yield_now().await;
                }
            })
            .await
        })
        .expect(
            "first inventory entry must retain a usable original executor for final facade cleanup",
        );
    probe.try_lock_exclusive().unwrap();
    probe.unlock().unwrap();
    assert!(held);
    assert_eq!(h.audio.inventory_calls(), 1);
    assert_eq!(h.audio.open_calls(), 0);
}

#[test]
fn final_app_off_executor_drop_keeps_file_fenced_when_the_captured_executor_has_stopped() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let state = app.close_state.clone();
    let retained = Arc::downgrade(&app._process_lock);
    let runtime = executor();
    let (entered, release) = h.audio.block_inventory();
    let observer = runtime.spawn({
        let app = app.clone();
        async move { app.list_audio_endpoints().await }
    });
    runtime.block_on(entered).unwrap();
    observer.abort();
    assert!(runtime.block_on(observer).unwrap_err().is_cancelled());
    drop(runtime);
    drop_off_executor(app);
    let _ = release.send(());
    let probe = probe(dir.path());
    assert!(
        probe.try_lock_exclusive().is_err(),
        "a captured Handle is not proof that its actual join executor remains alive"
    );
    assert!(retained.upgrade().is_some());
    let state = state.lock().unwrap();
    assert!(state.audio_ack.is_none());
    assert!(!state.close_succeeded);
    assert_eq!(h.audio.open_calls(), 0);
}

#[test]
fn final_app_cancelled_cleanup_retains_file_after_the_original_audio_source_joins() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let state = app.close_state.clone();
    let retained = Arc::downgrade(&app._process_lock);
    let runtime = executor();
    let (entered, release) = h.audio.block_inventory();
    let observer = runtime.spawn({
        let app = app.clone();
        async move { app.list_audio_endpoints().await }
    });
    runtime.block_on(entered).unwrap();
    observer.abort();
    assert!(runtime.block_on(observer).unwrap_err().is_cancelled());
    runtime.block_on(async {
        drop(app);
    });
    let (audio, task) = {
        let mut state = state.lock().unwrap();
        (
            state.audio.clone().unwrap(),
            state.facade_task.take().unwrap(),
        )
    };
    task.abort();
    assert!(runtime.block_on(task).unwrap_err().is_cancelled());
    let probe = probe(dir.path());
    assert!(probe.try_lock_exclusive().is_err());
    release.send(()).unwrap();
    runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(2), audio.wait()).await })
        .expect("the independent audio owner must actually join the released inventory")
        .unwrap();
    assert!(
        probe.try_lock_exclusive().is_err(),
        "a cancelled final cleanup cannot discharge its unjoined delivery scope"
    );
    assert!(retained.upgrade().is_some());
    let state = state.lock().unwrap();
    assert!(state.audio_ack.is_none());
    assert!(!state.close_succeeded);
    assert_eq!(h.audio.inventory_calls(), 1);
    assert_eq!(h.audio.open_calls(), 0);
}
