use crate::{
    audio::platform::{
        driver_runner::{DriverLimits, run_utility},
        process_io::{ProcessControl, ProcessExit, ProcessPipes, ProcessSpawner, ProcessSpec},
    },
    error::AppResult,
};
use async_trait::async_trait;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    sync::{Notify, watch},
};

struct Control {
    exit: watch::Sender<Option<i32>>,
    kills: AtomicUsize,
    killed: Notify,
    fail_termination: AtomicBool,
}
impl Control {
    fn new(exit: Option<i32>) -> Arc<Self> {
        let (exit, _) = watch::channel(exit);
        Arc::new(Self {
            exit,
            kills: AtomicUsize::new(0),
            killed: Notify::new(),
            fail_termination: AtomicBool::new(false),
        })
    }
}
#[async_trait]
impl ProcessControl for Control {
    async fn wait(&self) -> AppResult<ProcessExit> {
        let mut exit = self.exit.subscribe();
        loop {
            if let Some(code) = *exit.borrow_and_update() {
                return Ok(ProcessExit { code: Some(code) });
            }
            exit.changed().await.unwrap();
        }
    }
    async fn terminate(&self) -> AppResult<ProcessExit> {
        self.kills.fetch_add(1, Ordering::SeqCst);
        self.killed.notify_one();
        if self.fail_termination.load(Ordering::SeqCst) {
            return Err(crate::error::AppError::Internal(
                "fixture terminate failed".into(),
            ));
        }
        self.wait().await
    }
}
struct Spawner {
    control: Arc<Control>,
    bytes: Vec<u8>,
    stalled_read: bool,
    gate: Option<Arc<Notify>>,
    entered: Notify,
    specs: Mutex<Vec<ProcessSpec>>,
}
#[async_trait]
impl ProcessSpawner for Spawner {
    async fn spawn(&self, spec: ProcessSpec) -> AppResult<ProcessPipes> {
        self.specs.lock().unwrap().push(spec);
        self.entered.notify_one();
        if let Some(gate) = &self.gate {
            gate.notified().await;
        }
        let (read, mut write) = tokio::io::duplex(64);
        let bytes = self.bytes.clone();
        let stalled = self.stalled_read;
        let control = self.control.clone();
        tokio::spawn(async move {
            if stalled {
                let _ = control.wait().await;
            } else {
                let _ = write.write_all(&bytes).await;
            }
        });
        Ok(ProcessPipes {
            stdin: None,
            stdout: Some(Box::pin(read)),
            control: self.control.clone(),
        })
    }
}
fn limits() -> DriverLimits {
    DriverLimits {
        operation: Duration::from_millis(100),
        command: Duration::from_millis(25),
        reap: Duration::from_millis(15),
        output_bytes: 16,
    }
}
fn spawner(
    control: Arc<Control>,
    bytes: &[u8],
    stalled_read: bool,
    gate: Option<Arc<Notify>>,
) -> Arc<Spawner> {
    Arc::new(Spawner {
        control,
        bytes: bytes.into(),
        stalled_read,
        gate,
        entered: Notify::new(),
        specs: Mutex::new(Vec::new()),
    })
}
async fn run(
    spawner: Arc<dyn ProcessSpawner>,
    limits: DriverLimits,
) -> AppResult<crate::audio::platform::CommandOutput> {
    run_utility(
        spawner,
        "pactl".into(),
        vec!["list".into(), "short".into(), "modules".into()],
        std::time::Instant::now() + limits.operation,
        limits,
    )
    .await
}
#[tokio::test]
async fn utility_output_is_capped_and_termination_requires_exit_acknowledgement() {
    let control = Control::new(None);
    let spawner = spawner(control.clone(), &[b'x'; 64], false, None);
    let work = tokio::spawn(run(spawner.clone(), limits()));
    tokio::time::timeout(Duration::from_secs(1), control.killed.notified())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(35)).await;
    assert!(
        !work.is_finished(),
        "reap timeout cannot release uncertain process ownership"
    );
    control.exit.send_replace(Some(-9));
    let error = tokio::time::timeout(Duration::from_secs(1), work)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(!error.to_string().contains("xxxx"));
    assert_eq!(control.kills.load(Ordering::SeqCst), 1);
    let specs = spawner.specs.lock().unwrap();
    assert!(!specs[0].pipe_stdin);
    assert!(specs[0].pipe_stdout);
    assert_eq!(specs[0].args, ["list", "short", "modules"]);
}
#[tokio::test]
async fn stalled_spawn_is_owned_and_late_child_is_terminated_before_return() {
    let control = Control::new(None);
    let gate = Arc::new(Notify::new());
    let spawner = spawner(control.clone(), b"", true, Some(gate.clone()));
    let work = tokio::spawn(run(spawner.clone(), limits()));
    spawner.entered.notified().await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(!work.is_finished());
    gate.notify_one();
    tokio::time::timeout(Duration::from_secs(1), control.killed.notified())
        .await
        .unwrap();
    assert!(!work.is_finished());
    control.exit.send_replace(Some(-9));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), work)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
}
#[tokio::test]
async fn utility_read_timeout_terminates_and_overall_deadline_prevents_another_spawn() {
    let control = Control::new(None);
    let spawner = spawner(control.clone(), b"", true, None);
    let work = tokio::spawn(run(spawner.clone(), limits()));
    tokio::time::timeout(Duration::from_secs(1), control.killed.notified())
        .await
        .unwrap();
    control.exit.send_replace(Some(-9));
    assert!(work.await.unwrap().is_err());
    assert!(
        run_utility(
            spawner.clone(),
            "pactl".into(),
            vec![],
            std::time::Instant::now(),
            limits()
        )
        .await
        .is_err()
    );
    assert_eq!(spawner.specs.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn successful_utility_keeps_exit_code_and_stdout_but_no_stderr() {
    let control = Control::new(Some(7));
    let spawner = spawner(control, b"modules", false, None);
    let output = run(spawner, limits()).await.unwrap();
    assert_eq!(output.status, Some(7));
    assert_eq!(output.stdout, "modules");
    assert!(output.stderr.is_empty());
}

#[tokio::test]
async fn real_linux_manager_uses_bounded_runner_and_keeps_the_shared_lease_until_reaped() {
    use crate::audio::platform::{
        AudioDriverManager,
        driver::{DriverController, DriverManagerFactory, DriverOperation, DriverOperations},
        driver_runner::BoundedCommandRunner,
        inventory::PlatformAudioBackend,
        linux::LinuxAudioDriver,
    };
    struct LinuxFactory;
    impl DriverManagerFactory for LinuxFactory {
        fn create(&self, runner: BoundedCommandRunner) -> Box<dyn AudioDriverManager> {
            Box::new(LinuxAudioDriver::new(runner))
        }
    }
    let backend = Arc::new(PlatformAudioBackend::new());
    let control = Control::new(None);
    let spawner = spawner(control.clone(), &[b'x'; 64], false, None);
    let driver = Arc::new(
        DriverController::with_parts(backend.clone(), Arc::new(LinuxFactory), spawner, limits())
            .unwrap(),
    );
    let work = tokio::spawn({
        let driver = driver.clone();
        async move { driver.perform(DriverOperation::Repair).await }
    });
    tokio::time::timeout(Duration::from_secs(1), control.killed.notified())
        .await
        .unwrap();
    work.abort();
    tokio::time::sleep(Duration::from_millis(35)).await;
    assert!(backend.begin_maintenance().is_err());
    assert!(driver.perform(DriverOperation::Install).await.is_err());
    control.exit.send_replace(Some(-9));
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
}
#[tokio::test]
async fn termination_error_does_not_substitute_for_a_later_exit_acknowledgement() {
    let control = Control::new(None);
    control.fail_termination.store(true, Ordering::SeqCst);
    let spawner = spawner(control.clone(), &[b'x'; 64], false, None);
    let work = tokio::spawn(run(spawner, limits()));
    tokio::time::timeout(Duration::from_secs(1), control.killed.notified())
        .await
        .unwrap();
    assert!(!work.is_finished());
    control.exit.send_replace(Some(-9));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), work)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
}
#[tokio::test]
async fn multi_command_manager_shares_one_operation_deadline() {
    use crate::audio::platform::{
        AudioDriverManager,
        driver::{DriverController, DriverManagerFactory, DriverOperation, DriverOperations},
        driver_runner::BoundedCommandRunner,
        inventory::PlatformAudioBackend,
        linux::LinuxAudioDriver,
        process_io::test_support::TestControl,
    };
    struct LinuxFactory;
    impl DriverManagerFactory for LinuxFactory {
        fn create(&self, runner: BoundedCommandRunner) -> Box<dyn AudioDriverManager> {
            Box::new(LinuxAudioDriver::new(runner))
        }
    }
    struct DelayedSpawner(AtomicUsize);
    #[async_trait]
    impl ProcessSpawner for DelayedSpawner {
        async fn spawn(&self, _: ProcessSpec) -> AppResult<ProcessPipes> {
            self.0.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            let control = TestControl::new();
            control.exit(0);
            Ok(ProcessPipes {
                stdin: None,
                stdout: Some(Box::pin(tokio::io::empty())),
                control,
            })
        }
    }
    let spawner = Arc::new(DelayedSpawner(AtomicUsize::new(0)));
    let driver = DriverController::with_parts(
        Arc::new(PlatformAudioBackend::new()),
        Arc::new(LinuxFactory),
        spawner.clone(),
        DriverLimits {
            operation: Duration::from_millis(50),
            command: Duration::from_secs(1),
            ..limits()
        },
    )
    .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            driver.perform(DriverOperation::Install)
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(
        spawner.0.load(Ordering::SeqCst) <= 3,
        "each successful command must not restart the 50ms operation budget"
    );
}
