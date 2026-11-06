//! Explicit, serialized maintenance with one non-Send platform-manager owner.
use super::{
    AudioDriverManager, DriverStatus,
    driver_runner::{BoundedCommandRunner, DriverLimits},
    inventory::{MaintenanceLease, PlatformAudioBackend},
    process_io::{ProcessSpawner, SystemProcessSpawner},
};
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriverOperation {
    Install,
    Repair,
    Uninstall,
}
/// Completion acknowledges that platform mutation and child cleanup have ended.
/// Implementations must retain uncertain ownership instead of returning an
/// observation timeout; AppState bounds the caller's observation separately.
#[async_trait]
pub trait DriverOperations: Send + Sync {
    async fn perform(&self, operation: DriverOperation) -> AppResult<DriverStatus>;
}
pub(crate) trait DriverManagerFactory: Send + Sync {
    fn create(&self, runner: BoundedCommandRunner) -> Box<dyn AudioDriverManager>;
}
struct PlatformFactory;
impl DriverManagerFactory for PlatformFactory {
    fn create(&self, runner: BoundedCommandRunner) -> Box<dyn AudioDriverManager> {
        #[cfg(target_os = "linux")]
        {
            Box::new(super::linux::LinuxAudioDriver::new(runner))
        }
        #[cfg(target_os = "windows")]
        {
            Box::new(super::windows::WindowsAudioDriver::new(runner))
        }
        #[cfg(target_os = "macos")]
        {
            Box::new(super::macos::MacOsAudioDriver::new(runner))
        }
    }
}
struct Request {
    operation: DriverOperation,
    lease: MaintenanceLease,
    runner: BoundedCommandRunner,
    finished: oneshot::Sender<AppResult<DriverStatus>>,
}
pub(crate) struct DriverController {
    backend: Arc<PlatformAudioBackend>,
    requests: mpsc::SyncSender<Request>,
    busy: Arc<AtomicBool>,
    spawner: Arc<dyn ProcessSpawner>,
    limits: DriverLimits,
}
impl DriverController {
    pub(crate) fn new(backend: Arc<PlatformAudioBackend>) -> AppResult<Self> {
        Self::with_parts(
            backend,
            Arc::new(PlatformFactory),
            Arc::new(SystemProcessSpawner),
            DriverLimits::default(),
        )
    }
    pub(crate) fn with_parts(
        backend: Arc<PlatformAudioBackend>,
        factory: Arc<dyn DriverManagerFactory>,
        spawner: Arc<dyn ProcessSpawner>,
        limits: DriverLimits,
    ) -> AppResult<Self> {
        let (requests, receiver) = mpsc::sync_channel::<Request>(1);
        let busy = Arc::new(AtomicBool::new(false));
        let owner_busy = busy.clone();
        std::thread::Builder::new()
            .name("vox-driver-owner".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    // Construction, method calls and destruction all happen here.
                    // If a native manager panics, keep the request/lease uncertain.
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let manager = factory.create(request.runner);
                        let result = match request.operation {
                            DriverOperation::Install => manager.install(),
                            DriverOperation::Repair => manager.repair(),
                            DriverOperation::Uninstall => manager.uninstall(),
                        };
                        drop(manager);
                        safe_result(result)
                    }));
                    let Ok(result) = result else {
                        // No replacement worker or lease release after uncertain native ownership.
                        loop {
                            std::thread::park();
                        }
                    };
                    request.lease.complete();
                    owner_busy.store(false, Ordering::Release);
                    let _ = request.finished.send(result);
                }
            })
            .map_err(|_| AppError::Internal("driver owner could not start".into()))?;
        Ok(Self {
            backend,
            requests,
            busy,
            spawner,
            limits,
        })
    }
}
#[async_trait]
impl DriverOperations for DriverController {
    async fn perform(&self, operation: DriverOperation) -> AppResult<DriverStatus> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(busy_error());
        }
        let lease = match self.backend.begin_maintenance() {
            Ok(lease) => lease,
            Err(error) => {
                self.busy.store(false, Ordering::Release);
                return Err(error);
            }
        };
        let (finished, result) = oneshot::channel();
        let request = Request {
            operation,
            lease,
            runner: BoundedCommandRunner {
                runtime: tokio::runtime::Handle::current(),
                spawner: self.spawner.clone(),
                expires: Instant::now() + self.limits.operation,
                limits: self.limits,
            },
            finished,
        };
        if let Err(error) = self.requests.try_send(request) {
            let request = match error {
                mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request) => {
                    request
                }
            };
            request.lease.complete();
            self.busy.store(false, Ordering::Release);
            return Err(busy_error());
        }
        // Dropping this observer never cancels the admitted owner request.
        match result.await {
            Ok(result) => result,
            Err(_) => std::future::pending().await,
        }
    }
}
const BUSY_MESSAGE: &str = "audio is busy or driver maintenance remains pending";
pub(crate) fn busy_error() -> AppError {
    AppError::InvalidRequest(BUSY_MESSAGE.into())
}
pub(crate) fn safe_result(result: AppResult<DriverStatus>) -> AppResult<DriverStatus> {
    match result {
        Err(AppError::InvalidRequest(reason)) if reason == BUSY_MESSAGE => Err(busy_error()),
        Ok(DriverStatus::Failed(reason)) if reason == super::windows::PACKAGE_UNAVAILABLE => Ok(DriverStatus::Failed(reason)),
        Ok(DriverStatus::Failed(_)) => Ok(DriverStatus::Failed("Audio driver operation failed; recheck audio endpoints and utility availability.".into())),
        Ok(DriverStatus::NeedsApproval(_)) => Ok(DriverStatus::NeedsApproval("A licensed signed audio extension and user approval are required; verify package availability.".into())),
        Err(_) => Err(AppError::Internal("audio driver operation failed; recheck audio configuration".into())),
        result => result,
    }
}
pub(crate) struct UnavailableDriverOperations;
#[async_trait]
impl DriverOperations for UnavailableDriverOperations {
    async fn perform(&self, _: DriverOperation) -> AppResult<DriverStatus> {
        Err(AppError::InvalidRequest(
            "driver operations are not configured".into(),
        ))
    }
}
