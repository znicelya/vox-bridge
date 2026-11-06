use super::*;

use crate::{error::AppError, intelligence::types::IntelligenceErrorCode as Code};

struct LimitedControl {
    stop: watch::Sender<bool>,
    result: watch::Receiver<Option<ProcessExit>>,
}
#[async_trait]
impl ProcessControl for LimitedControl {
    async fn wait(&self) -> AppResult<ProcessExit> {
        let mut result = self.result.clone();
        loop {
            if let Some(exit) = result.borrow_and_update().clone() {
                return Ok(exit);
            }
            result
                .changed()
                .await
                .map_err(|_| AppError::Intelligence(Code::Interrupted))?;
        }
    }
    async fn terminate(&self) -> AppResult<ProcessExit> {
        self.stop.send_replace(true);
        self.wait().await
    }
}

/// The parser's cleanup allowance is only an observation window. Unlike the
/// ordinary audio supervisor, this owner never publishes an error as an exit.
fn supervise_limited(
    mut child: Box<dyn OwnedChild>,
    allowance: Duration,
) -> Arc<dyn ProcessControl> {
    let (stop, mut receiver) = watch::channel(false);
    let (publish, result) = watch::channel(None);
    tokio::spawn(async move {
        let mut terminating = false;
        let exit = loop {
            if terminating {
                if let Ok(Ok(exit)) = tokio::time::timeout(allowance, child.wait()).await {
                    break exit;
                }
                // A temporary wait failure or observation timeout keeps the child.
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            tokio::select! { biased;
                _ = async {
                    loop {
                        if *receiver.borrow_and_update() { break; }
                        if receiver.changed().await.is_err() { break; }
                    }
                } => {
                    // Exactly one request. Even a failed kill leaves a real child
                    // owned until wait eventually acknowledges its natural exit.
                    let _ = child.kill();
                    terminating = true;
                }
                outcome = child.wait() => match outcome {
                    Ok(exit) => break exit,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        };
        drop(child);
        publish.send_replace(Some(exit));
    });
    Arc::new(LimitedControl { stop, result })
}

/// Blocking OS creation remains owned by this future until the handle is handed
/// to the strict supervisor. Parser cancellation must continue awaiting it.
pub(super) async fn spawn(
    spec: ProcessSpec,
    limits: ProcessResourceLimits,
) -> AppResult<ProcessPipes> {
    if limits.memory_bytes == 0 || limits.cpu_seconds == 0 {
        return Err(configuration());
    }
    let (mut child, installed) = tokio::task::spawn_blocking(move || spawn_blocking(spec, limits))
        .await
        .map_err(|_| configuration())??;
    let stdin = child
        .child
        .stdin
        .take()
        .map(|pipe| Box::pin(pipe) as Pin<Box<dyn AsyncWrite + Send>>);
    let stdout = child
        .child
        .stdout
        .take()
        .map(|pipe| Box::pin(pipe) as Pin<Box<dyn AsyncRead + Send>>);
    let control = supervise_limited(Box::new(child), Duration::from_secs(2));
    finish_spawn(
        ProcessPipes {
            stdin,
            stdout,
            control,
        },
        installed,
    )
    .await
}
async fn finish_spawn(mut process: ProcessPipes, installed: bool) -> AppResult<ProcessPipes> {
    if !installed {
        // No pipe escapes failed Windows assignment. Retain the configured job
        // and child through actual reap, including kill/wait observation errors.
        drop(process.stdin.take());
        drop(process.stdout.take());
        process.control.terminate().await?;
        return Err(configuration());
    }
    Ok(process)
}
fn configuration() -> AppError {
    AppError::Intelligence(Code::Configuration)
}

struct SystemLimitedChild {
    child: tokio::process::Child,
    #[cfg(windows)]
    job: windows::Job,
    #[cfg(windows)]
    assigned: bool,
}
#[async_trait]
impl OwnedChild for SystemLimitedChild {
    async fn wait(&mut self) -> ExitResult {
        OwnedChild::wait(&mut self.child).await
    }
    fn kill(&mut self) -> Result<(), AudioFailure> {
        #[cfg(windows)]
        if self.assigned {
            return self.job.terminate();
        }
        OwnedChild::kill(&mut self.child)
    }
}
fn spawn_blocking(
    spec: ProcessSpec,
    limits: ProcessResourceLimits,
) -> AppResult<(SystemLimitedChild, bool)> {
    let mut command = tokio::process::Command::new(spec.program);
    command
        .args(spec.args)
        .kill_on_drop(true)
        .stdin(if spec.pipe_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(if spec.pipe_stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::null());
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    install_unix_limits(&mut command, limits)?;
    #[cfg(windows)]
    let job = windows::Job::new(limits)?;
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    return Err(configuration());
    let child = command.spawn().map_err(|_| configuration())?;
    #[cfg(windows)]
    {
        let assigned = job.assign(&child);
        Ok((
            SystemLimitedChild {
                child,
                job,
                assigned,
            },
            assigned,
        ))
    }
    #[cfg(not(windows))]
    Ok((SystemLimitedChild { child }, true))
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn install_unix_limits(
    command: &mut tokio::process::Command,
    limits: ProcessResourceLimits,
) -> AppResult<()> {
    let cpu = libc::rlim_t::from(limits.cpu_seconds);
    #[cfg(target_os = "linux")]
    let memory = libc::rlim_t::try_from(limits.memory_bytes).map_err(|_| configuration())?;
    // SAFETY: the callback uses only setrlimit and errno capture after fork.
    // No allocation, logging, locks, formatting, or runtime work is performed.
    unsafe {
        command.pre_exec(move || {
            let cpu = libc::rlimit {
                rlim_cur: cpu,
                rlim_max: cpu,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, &cpu) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            #[cfg(target_os = "linux")]
            {
                let memory = libc::rlimit {
                    rlim_cur: memory,
                    rlim_max: memory,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &memory) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(())
}
#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::*;

    pub(super) struct Job(OwnedHandle);
    impl Job {
        pub(super) fn new(limits: ProcessResourceLimits) -> AppResult<Self> {
            let memory = usize::try_from(limits.memory_bytes).map_err(|_| configuration())?;
            let ticks = i64::from(limits.cpu_seconds)
                .checked_mul(10_000_000)
                .ok_or_else(configuration)?;
            // SAFETY: null security/name creates an unnamed, noninheritable job.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(configuration());
            }
            // SAFETY: successful CreateJobObjectW transfers one owned handle.
            let job = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY
                | JOB_OBJECT_LIMIT_PROCESS_TIME
                | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            info.BasicLimitInformation.PerProcessUserTimeLimit = ticks;
            info.ProcessMemoryLimit = memory;
            // SAFETY: the typed record is live for the call and its exact byte size
            // matches JobObjectExtendedLimitInformation. Job retains the handle.
            if unsafe {
                SetInformationJobObject(
                    job.0.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            } == 0
            {
                return Err(configuration());
            }
            Ok(job)
        }
        pub(super) fn assign(&self, child: &tokio::process::Child) -> bool {
            let Some(handle) = child.raw_handle() else {
                return false;
            };
            // SAFETY: both borrowed handles remain live throughout assignment.
            unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), handle) != 0 }
        }
        pub(super) fn terminate(&self) -> Result<(), AudioFailure> {
            // SAFETY: the job remains owned through successful child wait/reap.
            if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
                Err(AudioFailure::device("limited process termination failed"))
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Child {
        exit: watch::Receiver<bool>,
        alive: Arc<AtomicBool>,
        kills: Arc<AtomicUsize>,
        wait_failure: bool,
        kill_failure: bool,
    }
    impl Drop for Child {
        fn drop(&mut self) {
            self.alive.store(false, Ordering::Release);
        }
    }
    #[async_trait]
    impl OwnedChild for Child {
        async fn wait(&mut self) -> ExitResult {
            if std::mem::take(&mut self.wait_failure) {
                return Err(AudioFailure::device("synthetic wait error"));
            }
            loop {
                if *self.exit.borrow_and_update() {
                    return Ok(ProcessExit { code: Some(0) });
                }
                self.exit.changed().await.unwrap();
            }
        }
        fn kill(&mut self) -> Result<(), AudioFailure> {
            self.kills.fetch_add(1, Ordering::Relaxed);
            if self.kill_failure {
                Err(AudioFailure::device("synthetic kill error"))
            } else {
                Ok(())
            }
        }
    }
    async fn retained_until_reap(wait_failure: bool, kill_failure: bool) {
        let (exit, receiver) = watch::channel(false);
        let alive = Arc::new(AtomicBool::new(true));
        let kills = Arc::new(AtomicUsize::new(0));
        let control = supervise_limited(
            Box::new(Child {
                exit: receiver,
                alive: alive.clone(),
                kills: kills.clone(),
                wait_failure,
                kill_failure,
            }),
            Duration::from_millis(5),
        );
        let first = tokio::time::timeout(Duration::from_millis(30), control.terminate()).await;
        let retained = alive.load(Ordering::Acquire);
        exit.send_replace(true);
        let a = tokio::time::timeout(Duration::from_secs(1), control.wait()).await;
        let b = tokio::time::timeout(Duration::from_secs(1), control.terminate()).await;
        assert!(
            first.is_err(),
            "observer must remain pending until real exit"
        );
        assert!(
            retained,
            "unconfirmed child must stay owned beyond cleanup allowance"
        );
        assert_eq!(a.unwrap().unwrap().code, Some(0));
        assert_eq!(b.unwrap().unwrap().code, Some(0));
        assert_eq!(kills.load(Ordering::Acquire), 1);
    }
    #[tokio::test]
    async fn limited_parser_keeps_child_after_reap_observation_timeout() {
        retained_until_reap(false, false).await;
    }
    #[tokio::test]
    async fn limited_parser_wait_error_is_not_exit_acknowledgement() {
        retained_until_reap(true, false).await;
    }
    #[tokio::test]
    async fn limited_parser_kill_error_is_not_exit_acknowledgement() {
        retained_until_reap(false, true).await;
    }
    #[tokio::test]
    async fn limited_parser_failed_limit_assignment_keeps_child_until_actual_reap() {
        let (exit, receiver) = watch::channel(false);
        let alive = Arc::new(AtomicBool::new(true));
        let kills = Arc::new(AtomicUsize::new(0));
        let control = supervise_limited(
            Box::new(Child {
                exit: receiver,
                alive: alive.clone(),
                kills: kills.clone(),
                wait_failure: true,
                kill_failure: true,
            }),
            Duration::from_millis(5),
        );
        let (stdin, mut request_observer) = tokio::io::duplex(64);
        let task = tokio::spawn(finish_spawn(
            ProcessPipes {
                stdin: Some(Box::pin(stdin)),
                stdout: None,
                control,
            },
            false,
        ));
        let mut bytes = Vec::new();
        tokio::time::timeout(
            Duration::from_millis(100),
            request_observer.read_to_end(&mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        let pending = !task.is_finished();
        let retained = alive.load(Ordering::Acquire);
        exit.send_replace(true);
        let result = task.await.unwrap();
        assert!(
            bytes.is_empty(),
            "failed limit installation must write no request bytes"
        );
        assert!(
            pending && retained,
            "assignment error is not a reap acknowledgement"
        );
        assert!(matches!(
            result,
            Err(AppError::Intelligence(Code::Configuration))
        ));
        assert_eq!(kills.load(Ordering::Acquire), 1);
        assert!(!alive.load(Ordering::Acquire));
    }
}
