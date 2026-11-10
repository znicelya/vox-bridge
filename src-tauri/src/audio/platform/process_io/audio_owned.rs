//! Audio children retain a positive exit source independently of result observers.
use super::*;
use crate::audio::join::{
    AudioDescriptor, AudioLeafExecution, AudioProcessJoin, AudioProcessSource,
};
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

pub struct AudioOwnedProcess {
    pipes: ProcessPipes,
    actual: AudioProcessJoin,
}
impl AudioOwnedProcess {
    pub(crate) fn into_parts(self) -> (ProcessPipes, AudioProcessJoin) {
        (self.pipes, self.actual)
    }
    pub(crate) fn attach(
        mut pipes: ProcessPipes,
        actual: AudioProcessJoin,
        source: &AudioProcessSource,
    ) -> Self {
        pipes.stdin = pipes.stdin.map(|inner| {
            Box::pin(Writer {
                inner,
                _descriptor: source.descriptor(),
            }) as Pin<Box<dyn AsyncWrite + Send>>
        });
        pipes.stdout = pipes.stdout.map(|inner| {
            Box::pin(Reader {
                inner,
                _descriptor: source.descriptor(),
            }) as Pin<Box<dyn AsyncRead + Send>>
        });
        Self { pipes, actual }
    }
}
struct Reader {
    inner: Pin<Box<dyn AsyncRead + Send>>,
    _descriptor: AudioDescriptor,
}
impl AsyncRead for Reader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.inner.as_mut().poll_read(cx, buf)
    }
}
struct Writer {
    inner: Pin<Box<dyn AsyncWrite + Send>>,
    _descriptor: AudioDescriptor,
}
impl AsyncWrite for Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.inner.as_mut().poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.inner.as_mut().poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.inner.as_mut().poll_shutdown(cx)
    }
}

pub(super) async fn spawn(
    spec: ProcessSpec,
    execution: AudioLeafExecution,
) -> AppResult<AudioOwnedProcess> {
    let (source, actual) = execution.enter()?;
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
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            source.exited(None).await;
            return Err(spawn_error(error));
        }
    };
    let stdin = child
        .stdin
        .take()
        .map(|v| Box::pin(v) as Pin<Box<dyn AsyncWrite + Send>>);
    let stdout = child
        .stdout
        .take()
        .map(|v| Box::pin(v) as Pin<Box<dyn AsyncRead + Send>>);
    let (control, run) = owner(Box::new(child), Duration::from_secs(2));
    let process = AudioOwnedProcess::attach(
        ProcessPipes {
            stdin,
            stdout,
            control,
        },
        actual,
        &source,
    );
    source.run_owned(run);
    Ok(process)
}

// The result watch preserves bounded command outcomes; actual exit is separate.
fn owner(
    child: Box<dyn OwnedChild>,
    reap_timeout: Duration,
) -> (
    Arc<dyn ProcessControl>,
    impl FnOnce(AudioProcessSource) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
) {
    let (stop, mut stop_rx) = watch::channel(false);
    let (publish, result) = watch::channel(None);
    let run = move |source: AudioProcessSource| {
        Box::pin(async move {
            let mut child = child;
            let mut termination_requested = false;
            let exit = tokio::select! { biased;
                _=async { loop { if *stop_rx.borrow_and_update(){break;} if stop_rx.changed().await.is_err(){break;} } }=>{
                    termination_requested = true;
                    match child.kill(){Err(error)=>Err(error),Ok(())=>tokio::time::timeout(reap_timeout,child.wait()).await.unwrap_or(Err(AudioFailure::new(RuntimeFailureCode::CleanupTimeout,"process exit acknowledgement timed out")))}
                }
                value=child.wait()=>value,
            };
            publish.send_replace(Some(exit.clone()));
            // Retain the original Child even when the bounded result observer has
            // failed. Only a fresh positive wait can finish its actual leaf.
            let exit = match exit {
                Ok(exit) => exit,
                Err(_) => loop {
                    let waited = tokio::select! { biased;
                        _ = async { loop { if *stop_rx.borrow_and_update() { break; } if stop_rx.changed().await.is_err() { break; } } }, if !termination_requested => {
                            termination_requested = true;
                            let _ = child.kill();
                            continue;
                        }
                        waited = child.wait() => waited,
                    };
                    match waited {
                        Ok(exit) => break exit,
                        Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                    }
                },
            };
            drop(child);
            source.exited(exit.code).await;
        }) as Pin<Box<dyn std::future::Future<Output = ()> + Send>>
    };
    (Arc::new(Control { stop, result }), run)
}

pub(crate) async fn output_audio_owned(
    spawner: &dyn ProcessSpawner,
    args: &[&str],
    execution: AudioLeafExecution,
) -> AppResult<Vec<u8>> {
    output_timed(spawner, args, execution, Duration::from_secs(2)).await
}
async fn output_timed(
    spawner: &dyn ProcessSpawner,
    args: &[&str],
    execution: AudioLeafExecution,
    deadline: Duration,
) -> AppResult<Vec<u8>> {
    let expires = tokio::time::Instant::now() + deadline;
    let owned = tokio::time::timeout_at(
        expires,
        spawner.spawn_audio_owned(
            ProcessSpec {
                program: "pactl".into(),
                args: args.iter().map(OsString::from).collect(),
                pipe_stdin: false,
                pipe_stdout: true,
            },
            execution,
        ),
    )
    .await
    .map_err(|_| audio_error("audio utility spawn timed out"))??;
    let (mut process, _actual) = owned.into_parts();
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
        let _ = tokio::time::timeout(Duration::from_secs(2), control.terminate()).await;
    }
    result
}

#[cfg(test)]
mod tests;
