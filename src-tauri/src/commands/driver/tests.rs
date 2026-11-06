use crate::audio::platform::{
    AudioDriverManager, CommandOutput, CommandRunner, linux::LinuxAudioDriver,
};
use crate::error::AppResult;
use std::{ffi::OsString, path::Path, sync::Mutex};

#[derive(Default)]
struct OrderedRunner(Mutex<Vec<Vec<String>>>);
impl CommandRunner for OrderedRunner {
    fn run(&self, _: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput> {
        assert!(!privileged);
        let args: Vec<String> = args
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        self.0.lock().unwrap().push(args.clone());
        let stdout = if args == ["list", "short", "modules"] {
            "1 module-null-sink sink_name=VoxBridge_M1\n2 module-null-sink sink_name=VoxBridge_Output_Backup\n3 module-remap-source source_name=VoxBridge_Input master=VoxBridge_M1.monitor\n4 module-null-sink sink_name=VoxBridge_Output\n"
        } else {
            ""
        };
        Ok(CommandOutput {
            status: Some(0),
            stdout: stdout.into(),
            ..Default::default()
        })
    }
}
#[test]
fn uninstall_removes_dependent_source_before_backing_sink_and_keeps_similarly_named_devices() {
    let runner = std::sync::Arc::new(OrderedRunner::default());
    struct Shared(std::sync::Arc<OrderedRunner>);
    impl CommandRunner for Shared {
        fn run(
            &self,
            program: &Path,
            args: &[OsString],
            privileged: bool,
        ) -> AppResult<CommandOutput> {
            self.0.run(program, args, privileged)
        }
    }
    LinuxAudioDriver::new(Shared(runner.clone()))
        .uninstall()
        .unwrap();
    let calls = runner.0.lock().unwrap();
    assert_eq!(
        &calls[1..],
        &[
            vec!["unload-module", "3"],
            vec!["unload-module", "1"],
            vec!["unload-module", "4"]
        ]
    );
}

use crate::audio::platform::{
    DriverStatus,
    driver::{DriverController, DriverManagerFactory, DriverOperation, DriverOperations},
    driver_runner::{BoundedCommandRunner, DriverLimits},
    inventory::PlatformAudioBackend,
    process_io::SystemProcessSpawner,
};
use std::{
    rc::Rc,
    sync::{Arc, mpsc},
    thread::ThreadId,
    time::Duration,
};

struct Factory {
    events: Arc<Mutex<Vec<(&'static str, ThreadId)>>>,
    entered: tokio::sync::Notify,
    release: Mutex<Option<mpsc::Receiver<()>>>,
}
struct NonSendManager {
    _local: Rc<()>,
    factory: Arc<Factory>,
    release: Option<mpsc::Receiver<()>>,
}
impl DriverManagerFactory for Arc<Factory> {
    fn create(&self, _: BoundedCommandRunner) -> Box<dyn AudioDriverManager> {
        self.events
            .lock()
            .unwrap()
            .push(("create", std::thread::current().id()));
        Box::new(NonSendManager {
            _local: Rc::new(()),
            factory: self.clone(),
            release: self.release.lock().unwrap().take(),
        })
    }
}
impl Drop for NonSendManager {
    fn drop(&mut self) {
        self.factory
            .events
            .lock()
            .unwrap()
            .push(("drop", std::thread::current().id()));
    }
}
impl NonSendManager {
    fn perform(&self, operation: &'static str) -> AppResult<DriverStatus> {
        self.factory
            .events
            .lock()
            .unwrap()
            .push((operation, std::thread::current().id()));
        self.factory.entered.notify_one();
        if let Some(release) = &self.release {
            release.recv().unwrap();
        }
        Ok(DriverStatus::Ready)
    }
}
impl AudioDriverManager for NonSendManager {
    fn install(&self) -> AppResult<DriverStatus> {
        self.perform("install")
    }
    fn repair(&self) -> AppResult<DriverStatus> {
        self.perform("repair")
    }
    fn uninstall(&self) -> AppResult<DriverStatus> {
        self.perform("uninstall")
    }
    fn status(&self) -> AppResult<DriverStatus> {
        unreachable!()
    }
    fn list_endpoints(&self) -> AppResult<Vec<crate::audio::platform::AudioEndpoint>> {
        unreachable!()
    }
}
fn controlled() -> (
    Arc<DriverController>,
    Arc<PlatformAudioBackend>,
    Arc<Factory>,
    mpsc::Sender<()>,
) {
    let (release, wait) = mpsc::channel();
    let factory = Arc::new(Factory {
        events: Arc::default(),
        entered: tokio::sync::Notify::new(),
        release: Mutex::new(Some(wait)),
    });
    let backend = Arc::new(PlatformAudioBackend::new());
    let driver = Arc::new(
        DriverController::with_parts(
            backend.clone(),
            Arc::new(factory.clone()),
            Arc::new(SystemProcessSpawner),
            DriverLimits::default(),
        )
        .unwrap(),
    );
    (driver, backend, factory, release)
}
#[tokio::test]
async fn non_send_manager_lives_on_one_owner_and_cancelled_caller_cannot_replace_it() {
    let (driver, backend, factory, release) = controlled();
    assert!(
        factory.events.lock().unwrap().is_empty(),
        "construction must not create a manager"
    );
    let call = tokio::spawn({
        let driver = driver.clone();
        async move { driver.perform(DriverOperation::Install).await }
    });
    tokio::time::timeout(Duration::from_secs(1), factory.entered.notified())
        .await
        .unwrap();
    call.abort();
    let requests = (0..64).map(|_| driver.perform(DriverOperation::Repair));
    assert!(
        futures_util::future::join_all(requests)
            .await
            .into_iter()
            .all(|result| result.is_err())
    );
    assert!(backend.begin_maintenance().is_err());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Ok(lease) = backend.begin_maintenance() {
                lease.complete();
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        driver.perform(DriverOperation::Uninstall).await.unwrap(),
        DriverStatus::Ready
    );
    let events = factory.events.lock().unwrap();
    assert_eq!(
        events.iter().map(|(event, _)| *event).collect::<Vec<_>>(),
        ["create", "install", "drop", "create", "uninstall", "drop"]
    );
    assert!(
        events
            .iter()
            .all(|(_, thread)| *thread == events[0].1 && *thread != std::thread::current().id())
    );
}
#[test]
fn operation_tags_preserve_existing_driver_status_wire_shape() {
    for (tag, operation) in [
        ("install", DriverOperation::Install),
        ("repair", DriverOperation::Repair),
        ("uninstall", DriverOperation::Uninstall),
    ] {
        assert_eq!(
            serde_json::from_value::<DriverOperation>(serde_json::json!(tag)).unwrap(),
            operation
        );
    }
    assert!(serde_json::from_value::<DriverOperation>(serde_json::json!("Install")).is_err());
    assert_eq!(
        serde_json::to_value(DriverStatus::NeedsApproval("approval".into())).unwrap(),
        serde_json::json!({"NeedsApproval":"approval"})
    );
}
#[test]
fn linux_module_properties_keep_both_parser_levels_of_quoting() {
    #[derive(Clone, Default)]
    struct Runner(Arc<Mutex<Vec<Vec<String>>>>);
    impl CommandRunner for Runner {
        fn run(&self, _: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput> {
            assert!(!privileged);
            self.0.lock().unwrap().push(
                args.iter()
                    .map(|v| v.to_string_lossy().into_owned())
                    .collect(),
            );
            Ok(CommandOutput {
                status: Some(0),
                ..Default::default()
            })
        }
    }
    let runner = Runner::default();
    LinuxAudioDriver::new(runner.clone()).install().unwrap();
    let calls = runner.0.lock().unwrap();
    let properties: Vec<_> = calls
        .iter()
        .flatten()
        .filter(|arg| arg.contains("_properties="))
        .map(String::as_str)
        .collect();
    // pactl joins argv with spaces; modargs removes the outer quotes, then
    // pa_proplist_from_string needs the inner quotes to preserve the value.
    assert_eq!(
        properties,
        [
            r#"sink_properties="device.description='VoxBridge M1 Backend'""#,
            r#"source_properties="device.description='VoxBridge Input'""#,
            r#"sink_properties="device.description='VoxBridge Output'""#
        ]
    );
}
#[test]
fn public_driver_results_exclude_raw_process_output_and_keep_platform_package_gates() {
    use crate::{
        audio::platform::{
            driver::safe_result, macos::MacOsAudioDriver, windows::WindowsAudioDriver,
        },
        error::AppError,
    };
    for result in [
        Err(AppError::Internal("RAW PRIVATE STDERR".into())),
        Ok(DriverStatus::Failed("RAW PRIVATE STDERR".into())),
        Ok(DriverStatus::NeedsApproval("RAW PRIVATE STDERR".into())),
    ] {
        let safe = safe_result(result);
        assert!(!format!("{safe:?}").contains("RAW PRIVATE"));
    }
    let windows = safe_result(WindowsAudioDriver::new(OrderedRunner::default()).install()).unwrap();
    assert!(
        matches!(windows, DriverStatus::Failed(reason) if reason.contains("signed endpoint package is unavailable") && reason.contains("license gate"))
    );
    let mac = safe_result(MacOsAudioDriver::new(OrderedRunner::default()).install()).unwrap();
    assert!(
        matches!(mac, DriverStatus::NeedsApproval(reason) if reason.contains("signed") && reason.contains("approval"))
    );
}
#[tokio::test]
async fn terminal_but_quarantined_backend_rejects_app_maintenance_as_busy_without_manager_calls() {
    use crate::{
        app::AppState, audio::platform::inventory::driver_test_support, config::SecretStore,
        error::AppError, providers::factory::ProviderFactory,
    };
    let (driver, backend, factory, _release) = controlled();
    let request = driver_test_support::seed(&backend);
    let lease = driver_test_support::claim(&backend, &request).unwrap();
    lease.quarantine();
    lease.release();
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = AppState::with_driver_dependencies(
        dir.path().into(),
        secrets.clone(),
        backend,
        Arc::new(ProviderFactory::new(secrets)),
        driver,
    )
    .unwrap();
    assert!(app.list_sessions(None, None).await.unwrap().is_empty());
    assert!(matches!(
        app.manage_audio_driver(DriverOperation::Uninstall).await,
        Err(AppError::InvalidRequest(_))
    ));
    assert!(factory.events.lock().unwrap().is_empty());
}
