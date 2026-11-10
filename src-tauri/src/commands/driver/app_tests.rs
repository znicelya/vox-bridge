use crate::{
    app::{AppState, CredentialChange, SaveProviderRequest},
    audio::{
        PcmSpec,
        platform::{
            DriverStatus,
            driver::{DriverOperation, DriverOperations},
        },
    },
    config::SecretStore,
    error::AppResult,
    providers::{
        ProviderEvent,
        factory::ProviderFactory,
        fixture::{FixtureScript, FixtureStep},
    },
    session::{
        SessionStatus,
        test_support::{RuntimeHarness, metadata},
    },
};
use async_trait::async_trait;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, watch};
struct Driver {
    calls: AtomicUsize,
    entered: Notify,
    release: watch::Sender<bool>,
}
impl Driver {
    fn new(blocked: bool) -> Arc<Self> {
        let (release, _) = watch::channel(!blocked);
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
            release,
        })
    }
}
#[async_trait]
impl DriverOperations for Driver {
    async fn perform(&self, _: DriverOperation) -> AppResult<DriverStatus> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        let mut release = self.release.subscribe();
        loop {
            if *release.borrow_and_update() {
                break;
            }
            release.changed().await.unwrap();
        }
        Ok(DriverStatus::Ready)
    }
}
fn script() -> FixtureScript {
    FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap()
}
async fn setup(
    driver: Arc<dyn DriverOperations>,
    call: bool,
) -> (tempfile::TempDir, RuntimeHarness, AppState) {
    let h = if call {
        RuntimeHarness::call(script(), script()).unwrap()
    } else {
        RuntimeHarness::live(script()).unwrap()
    };
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = AppState::with_driver_dependencies(
        dir.path().into(),
        secrets.clone(),
        h.audio.clone(),
        Arc::new(ProviderFactory::new(secrets).with_test_provider(h.provider.clone())),
        driver,
    )
    .unwrap();
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    (dir, h, app)
}
async fn running(app: &AppState, id: uuid::Uuid) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if app.get_session(id).await.unwrap().unwrap().status == SessionStatus::Running {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn active_live_and_call_sessions_reject_all_driver_actions_without_invoking_port() {
    for call in [false, true] {
        let driver = Driver::new(false);
        let (_dir, h, app) = setup(driver.clone(), call).await;
        assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
        app.start_intelligence().await.unwrap();
        let session = app.create_session(h.launch.clone()).await.unwrap();
        running(&app, session.id).await;
        for operation in [
            DriverOperation::Install,
            DriverOperation::Repair,
            DriverOperation::Uninstall,
        ] {
            assert!(
                app.manage_audio_driver(operation).await.is_err(),
                "active audio must reject driver maintenance"
            );
        }
        assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
        app.stop_session(session.id).await.unwrap();
        app.manage_audio_driver(DriverOperation::Uninstall)
            .await
            .unwrap();
        assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
        app.close_all().await.unwrap();
    }
}
#[tokio::test]
async fn starting_reservation_rejects_driver_before_audio_open() {
    let driver = Driver::new(false);
    let (_dir, h, app) = setup(driver.clone(), false).await;
    app.start_intelligence().await.unwrap();
    let (entered, release) = h.audio.block_inventory();
    let create = tokio::spawn({
        let app = app.clone();
        let launch = h.launch.clone();
        async move { app.create_session(launch).await }
    });
    entered.await.unwrap();
    assert_eq!(h.audio.open_ports(), 0);
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        app.manage_audio_driver(DriverOperation::Repair),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    let session = create.await.unwrap().unwrap();
    app.stop_session(session.id).await.unwrap();
    app.close_all().await.unwrap();
}
#[tokio::test]
async fn cancelled_maintenance_observer_keeps_create_and_close_fenced_until_ack() {
    let driver = Driver::new(true);
    let (dir, h, mut app) = setup(driver.clone(), false).await;
    app.start_intelligence().await.unwrap();
    crate::app::driver_test_support::short_observation(&mut app);
    let call = tokio::spawn({
        let app = app.clone();
        async move { app.manage_audio_driver(DriverOperation::Install).await }
    });
    driver.entered.notified().await;
    call.abort();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            app.create_session(h.launch.clone())
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(
        app.manage_audio_driver(DriverOperation::Repair)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), app.close_all())
            .await
            .unwrap()
            .is_err()
    );
    assert!(app.open_window().is_err());
    assert!(
        AppState::new(dir.path().into()).is_err(),
        "retained work owns the process lock"
    );
    driver.release.send_replace(true);
    tokio::time::timeout(Duration::from_secs(1), app.close_all())
        .await
        .unwrap()
        .unwrap();
    app.reopen_window().await.unwrap();
    assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
    let session = app.create_session(h.launch.clone()).await.unwrap();
    app.stop_session(session.id).await.unwrap();
    app.close_all().await.unwrap();
}
#[tokio::test]
async fn maintenance_timeout_observes_without_releasing_the_operation() {
    let driver = Driver::new(true);
    let (_dir, h, mut app) = setup(driver.clone(), false).await;
    app.start_intelligence().await.unwrap();
    crate::app::driver_test_support::short_observation(&mut app);
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        app.manage_audio_driver(DriverOperation::Install),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(app.create_session(h.launch.clone()).await.is_err());
    assert!(
        app.manage_audio_driver(DriverOperation::Uninstall)
            .await
            .is_err()
    );
    driver.release.send_replace(true);
    tokio::time::timeout(Duration::from_secs(1), app.close_all())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn stopping_audio_remains_busy_until_the_native_close_acknowledgement() {
    let driver = Driver::new(false);
    let (_dir, h, app) = setup(driver.clone(), false).await;
    app.start_intelligence().await.unwrap();
    let release = h.audio.delay_close("live-input");
    let session = app.create_session(h.launch.clone()).await.unwrap();
    running(&app, session.id).await;
    let stop = tokio::spawn({
        let app = app.clone();
        async move { app.stop_session(session.id).await }
    });
    tokio::task::yield_now().await;
    assert!(
        app.manage_audio_driver(DriverOperation::Uninstall)
            .await
            .is_err()
    );
    assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    stop.await.unwrap().unwrap();
    app.close_all().await.unwrap();
}
#[tokio::test]
async fn uninstall_completion_does_not_replace_actual_inventory_readiness() {
    let driver = Driver::new(false);
    let (_dir, h, app) = setup(driver, false).await;
    app.start_intelligence().await.unwrap();
    h.audio
        .set_driver_status(DriverStatus::Failed("endpoints removed".into()));
    assert_eq!(
        app.manage_audio_driver(DriverOperation::Uninstall)
            .await
            .unwrap(),
        DriverStatus::Ready
    );
    assert_eq!(
        app.audio_driver_status().await.unwrap(),
        DriverStatus::Failed("endpoints removed".into())
    );
    assert!(app.create_session(h.launch.clone()).await.is_err());
    assert_eq!(h.audio.open_ports(), 0);
    app.close_all().await.unwrap();
}
#[tokio::test]
async fn paused_direction_still_excludes_maintenance_while_peer_can_keep_running() {
    use crate::domain::launch::DirectionPhase;
    for call in [false, true] {
        let driver = Driver::new(false);
        let (_dir, h, app) = setup(driver.clone(), call).await;
        app.start_intelligence().await.unwrap();
        let session = app.create_session(h.launch.clone()).await.unwrap();
        running(&app, session.id).await;
        h.audio.lose(if call { "a-input" } else { "live-input" });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let session = app.get_session(session.id).await.unwrap().unwrap();
                if session
                    .runtime
                    .unwrap()
                    .directions
                    .iter()
                    .any(|d| d.phase == DirectionPhase::Paused)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            app.manage_audio_driver(DriverOperation::Repair)
                .await
                .is_err()
        );
        assert_eq!(driver.calls.load(Ordering::SeqCst), 0);
        app.stop_session(session.id).await.unwrap();
        app.close_all().await.unwrap();
    }
}
#[tokio::test]
async fn admitted_work_retains_process_lock_after_all_caller_app_handles_are_dropped() {
    let driver = Driver::new(true);
    let (dir, h, app) = setup(driver.clone(), false).await;
    let caller =
        tokio::spawn(async move { app.manage_audio_driver(DriverOperation::Install).await });
    driver.entered.notified().await;
    caller.abort();
    let _ = caller.await;
    let reopen = || {
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        AppState::with_dependencies(
            dir.path().into(),
            secrets.clone(),
            h.audio.clone(),
            Arc::new(ProviderFactory::new(secrets).with_test_provider(h.provider.clone())),
        )
    };
    assert!(
        reopen().is_err(),
        "the owned driver task must retain the process lock"
    );
    driver.release.send_replace(true);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if reopen().is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
