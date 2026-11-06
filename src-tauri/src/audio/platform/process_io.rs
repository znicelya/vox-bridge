//! Bounded child ownership. Waiting never holds a child mutex ahead of termination.
use crate::domain::transcript::RuntimeFailureCode;
use crate::{
    audio::runtime::{AudioFailure, audio_error},
    error::AppResult,
};
use async_trait::async_trait;
use std::{ffi::OsString, path::PathBuf, pin::Pin, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite},
    sync::watch,
};
#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub pipe_stdin: bool,
    pub pipe_stdout: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessExit {
    pub code: Option<i32>,
}
#[async_trait]
pub trait ProcessControl: Send + Sync {
    async fn wait(&self) -> AppResult<ProcessExit>;
    async fn terminate(&self) -> AppResult<ProcessExit>;
}
pub struct ProcessPipes {
    pub stdin: Option<Pin<Box<dyn AsyncWrite + Send>>>,
    pub stdout: Option<Pin<Box<dyn AsyncRead + Send>>>,
    pub control: Arc<dyn ProcessControl>,
}
#[async_trait]
pub trait ProcessSpawner: Send + Sync {
    async fn spawn(&self, spec: ProcessSpec) -> AppResult<ProcessPipes>;
}
pub struct SystemProcessSpawner;
type ExitResult = Result<ProcessExit, AudioFailure>;
struct Control {
    stop: watch::Sender<bool>,
    result: watch::Receiver<Option<ExitResult>>,
}
#[async_trait]
impl ProcessControl for Control {
    async fn wait(&self) -> AppResult<ProcessExit> {
        let mut rx = self.result.clone();
        loop {
            if let Some(value) = rx.borrow_and_update().clone() {
                return value.map_err(Into::into);
            }
            rx.changed()
                .await
                .map_err(|_| audio_error("process owner ended without exit acknowledgement"))?;
        }
    }
    async fn terminate(&self) -> AppResult<ProcessExit> {
        self.stop.send_replace(true);
        self.wait().await
    }
}
#[async_trait]
trait OwnedChild: Send {
    async fn wait(&mut self) -> ExitResult;
    fn kill(&mut self) -> Result<(), AudioFailure>;
}
#[async_trait]
impl OwnedChild for tokio::process::Child {
    async fn wait(&mut self) -> ExitResult {
        tokio::process::Child::wait(self)
            .await
            .map(|status| ProcessExit {
                code: status.code(),
            })
            .map_err(|_| AudioFailure::device("process wait failed"))
    }
    fn kill(&mut self) -> Result<(), AudioFailure> {
        self.start_kill()
            .map_err(|_| AudioFailure::device("process termination failed"))
    }
}
fn supervise(mut child: Box<dyn OwnedChild>, reap_timeout: Duration) -> Arc<dyn ProcessControl> {
    let (stop, mut stop_rx) = watch::channel(false);
    let (publish, result) = watch::channel(None);
    tokio::spawn(async move {
        let exit = tokio::select! { biased;
            _=async { loop { if *stop_rx.borrow_and_update(){break;} if stop_rx.changed().await.is_err(){break;} } }=>{
                match child.kill(){Err(error)=>Err(error),Ok(())=>tokio::time::timeout(reap_timeout,child.wait()).await.unwrap_or(Err(AudioFailure::new(RuntimeFailureCode::CleanupTimeout,"process exit acknowledgement timed out")))}
            }
            value=child.wait()=>value,
        };
        publish.send_replace(Some(exit));
    });
    Arc::new(Control { stop, result })
}
#[async_trait]
impl ProcessSpawner for SystemProcessSpawner {
    async fn spawn(&self, spec: ProcessSpec) -> AppResult<ProcessPipes> {
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
        let mut child = command.spawn().map_err(spawn_error)?;
        let stdin = child
            .stdin
            .take()
            .map(|v| Box::pin(v) as Pin<Box<dyn AsyncWrite + Send>>);
        let stdout = child
            .stdout
            .take()
            .map(|v| Box::pin(v) as Pin<Box<dyn AsyncRead + Send>>);
        let control = supervise(Box::new(child), Duration::from_secs(2));
        Ok(ProcessPipes {
            stdin,
            stdout,
            control,
        })
    }
}
pub(crate) async fn output(spawner: &dyn ProcessSpawner, args: &[&str]) -> AppResult<Vec<u8>> {
    output_with_deadline(spawner, args, Duration::from_secs(2)).await
}
async fn output_with_deadline(
    spawner: &dyn ProcessSpawner,
    args: &[&str],
    deadline: Duration,
) -> AppResult<Vec<u8>> {
    let expires = tokio::time::Instant::now() + deadline;
    let mut process = tokio::time::timeout_at(
        expires,
        spawner.spawn(ProcessSpec {
            program: "pactl".into(),
            args: args.iter().map(OsString::from).collect(),
            pipe_stdin: false,
            pipe_stdout: true,
        }),
    )
    .await
    .map_err(|_| audio_error("audio utility spawn timed out"))??;
    let control = process.control.clone();
    let result = tokio::time::timeout_at(expires, async {
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| audio_error("audio utility has no output pipe"))?;
        let mut bytes = Vec::new();
        stdout
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| audio_error("audio utility output failed"))?;
        if bytes.len() > 1_048_576 {
            return Err(audio_error("audio utility output exceeded limit"));
        }
        if control.wait().await?.code != Some(0) {
            return Err(audio_error("audio utility failed"));
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| audio_error("audio utility timed out"))
    .and_then(|v| v);
    if result.is_err() {
        let _ = tokio::time::timeout(Duration::from_secs(2), process.control.terminate()).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Child {
        exit: watch::Receiver<bool>,
        exited: watch::Sender<bool>,
        kills: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl OwnedChild for Child {
        async fn wait(&mut self) -> ExitResult {
            loop {
                if *self.exit.borrow_and_update() {
                    return Ok(ProcessExit { code: Some(0) });
                }
                self.exit.changed().await.unwrap();
            }
        }
        fn kill(&mut self) -> Result<(), AudioFailure> {
            self.kills.fetch_add(1, Ordering::Relaxed);
            self.exited.send_replace(true);
            Ok(())
        }
    }
    #[tokio::test]
    async fn blocked_wait_does_not_block_idempotent_kill_and_reap() {
        let (exit, rx) = watch::channel(false);
        let kills = Arc::new(AtomicUsize::new(0));
        let c = supervise(
            Box::new(Child {
                exit: rx,
                exited: exit,
                kills: kills.clone(),
            }),
            Duration::from_millis(30),
        );
        let wait = c.wait();
        let (a, b, wait) = tokio::join!(c.terminate(), c.terminate(), wait);
        assert_eq!(a.unwrap().code, Some(0));
        b.unwrap();
        wait.unwrap();
        assert_eq!(kills.load(Ordering::Relaxed), 1);
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    pub struct TestControl {
        exit: watch::Sender<Option<ProcessExit>>,
        pub kills: AtomicUsize,
    }
    impl TestControl {
        pub fn new() -> Arc<Self> {
            let (exit, _) = watch::channel(None);
            Arc::new(Self {
                exit,
                kills: AtomicUsize::new(0),
            })
        }
        pub fn exit(&self, code: i32) {
            self.exit
                .send_replace(Some(ProcessExit { code: Some(code) }));
        }
        pub fn exited(&self) -> bool {
            self.exit.borrow().is_some()
        }
    }
    #[async_trait]
    impl ProcessControl for TestControl {
        async fn wait(&self) -> AppResult<ProcessExit> {
            let mut rx = self.exit.subscribe();
            loop {
                if let Some(value) = rx.borrow_and_update().clone() {
                    return Ok(value);
                }
                rx.changed().await.unwrap();
            }
        }
        async fn terminate(&self) -> AppResult<ProcessExit> {
            self.exit.send_if_modified(|value| {
                if value.is_some() {
                    return false;
                }
                self.kills.fetch_add(1, Ordering::Relaxed);
                *value = Some(ProcessExit { code: Some(-9) });
                true
            });
            self.wait().await
        }
    }
}

#[cfg(test)]
mod output_tests {
    use super::test_support::TestControl;
    use super::*;
    use std::sync::{Mutex, atomic::Ordering};
    struct OutputSpawner {
        control: Arc<TestControl>,
        bytes: Mutex<Option<Vec<u8>>>,
        spawn_delay: Duration,
        read_delay: Duration,
    }
    #[async_trait]
    impl ProcessSpawner for OutputSpawner {
        async fn spawn(&self, spec: ProcessSpec) -> AppResult<ProcessPipes> {
            assert_eq!(spec.program, PathBuf::from("pactl"));
            tokio::time::sleep(self.spawn_delay).await;
            let bytes = self.bytes.lock().unwrap().take().unwrap();
            let (read, mut write) = tokio::io::duplex(64);
            let delay = self.read_delay;
            let control = self.control.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                tokio::time::sleep(delay).await;
                if write.write_all(&bytes).await.is_ok() {
                    control.exit(0);
                }
            });
            Ok(ProcessPipes {
                stdin: None,
                stdout: Some(Box::pin(read)),
                control: self.control.clone(),
            })
        }
    }
    #[tokio::test]
    async fn utility_spawn_and_output_share_one_deadline() {
        let spawner = OutputSpawner {
            control: TestControl::new(),
            bytes: Mutex::new(Some(b"[]".to_vec())),
            spawn_delay: Duration::from_millis(60),
            read_delay: Duration::from_millis(60),
        };
        let result = output_with_deadline(
            &spawner,
            &["--format=json", "list", "sources"],
            Duration::from_millis(90),
        )
        .await;
        assert!(
            result.is_err(),
            "spawn and read must not each get a fresh deadline"
        );
        assert_eq!(spawner.control.kills.load(Ordering::Acquire), 1);
    }
    #[tokio::test]
    async fn oversized_output_is_bounded_and_terminated() {
        let spawner = OutputSpawner {
            control: TestControl::new(),
            bytes: Mutex::new(Some(vec![b'x'; 1_048_700])),
            spawn_delay: Duration::ZERO,
            read_delay: Duration::ZERO,
        };
        assert!(
            output_with_deadline(&spawner, &[], Duration::from_secs(1))
                .await
                .is_err()
        );
        assert_eq!(spawner.control.kills.load(Ordering::Acquire), 1);
    }
}

fn spawn_error(error: std::io::Error) -> crate::error::AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AudioFailure::new(
            RuntimeFailureCode::AudioBackendUnavailable,
            "install pulseaudio-utils and enable PulseAudio or pipewire-pulse",
        )
        .into()
    } else {
        audio_error("audio process could not start")
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    #[test]
    fn missing_tool_keeps_a_typed_backend_failure_without_raw_os_text() {
        let error = spawn_error(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "private machine-specific path",
        ));
        assert!(matches!(
            error,
            crate::error::AppError::Audio {
                code: RuntimeFailureCode::AudioBackendUnavailable,
                ..
            }
        ));
        assert!(!error.to_string().contains("private"));
    }
}
