use super::{
    MAX_OUTPUT_BYTES,
    protocol::{self, Request, Response},
    types::{DocumentKind, TextBlock},
};
use crate::{
    audio::platform::process_io::{
        ProcessControl, ProcessPipes, ProcessResourceLimits, ProcessSpawner, ProcessSpec,
    },
    error::{AppError, AppResult},
    intelligence::types::IntelligenceErrorCode as Code,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};

#[async_trait::async_trait]
pub trait DocumentParser: Send + Sync {
    /// The caller retains its parser permit and awaits this future through actual
    /// reap. Cancellation/deadlines request cleanup; they never detach the child.
    async fn parse(
        &self,
        managed_path: PathBuf,
        kind: DocumentKind,
        cancel: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code>;
}
pub struct ProcessDocumentParser {
    spawner: Arc<dyn ProcessSpawner>,
    executable: PathBuf,
    deadline: Duration,
    cleanup_allowance: Duration,
    #[cfg(unix)]
    allocation_bytes: usize,
}
impl ProcessDocumentParser {
    pub fn new() -> AppResult<Self> {
        Ok(Self::with_executable(std::env::current_exe().map_err(
            |_| AppError::Intelligence(Code::Configuration),
        )?))
    }
    /// Lower the Unix worker's requested-heap ceiling, including startup bytes.
    /// Intended for backend policy and real-binary budget tests; it cannot raise
    /// the production ceiling or change OS process limits.
    #[cfg(unix)]
    pub fn with_allocation_limit(mut self, bytes: usize) -> AppResult<Self> {
        if bytes == 0 || bytes > 1024 * 1024 * 1024 {
            return Err(AppError::Intelligence(Code::Configuration));
        }
        self.allocation_bytes = bytes;
        Ok(self)
    }
    /// Backend-only executable injection for Cargo's real binary integration target.
    pub fn with_executable(executable: PathBuf) -> Self {
        Self {
            spawner: Arc::new(crate::audio::platform::process_io::SystemProcessSpawner),
            executable,
            deadline: Duration::from_secs(30),
            cleanup_allowance: Duration::from_secs(2),
            #[cfg(unix)]
            allocation_bytes: 1024 * 1024 * 1024,
        }
    }
}
async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}
fn exit_error(code: Option<i32>) -> Code {
    if code == Some(86) {
        Code::ResourceLimit
    } else {
        Code::ParseFailed
    }
}

/// A failed control observation is not a reap. Keep the same control/child owner;
/// do not issue repeated termination requests or release the caller's permit.
async fn cleanup(control: &Arc<dyn ProcessControl>, allowance: Duration) {
    let mut termination = Box::pin(control.terminate());
    let observed = match tokio::time::timeout(allowance, &mut termination).await {
        Ok(value) => value,
        Err(_) => termination.await,
    };
    if observed.is_ok() {
        return;
    }
    loop {
        if control.wait().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
#[async_trait::async_trait]
impl DocumentParser for ProcessDocumentParser {
    async fn parse(
        &self,
        managed_path: PathBuf,
        kind: DocumentKind,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code> {
        let expires = tokio::time::Instant::now() + self.deadline;
        if *cancel.borrow() {
            return Err(Code::Cancelled);
        }
        if kind == DocumentKind::Unknown {
            return Err(Code::UnsupportedDocument);
        }
        let request =
            serde_json::to_vec(&Request::new(managed_path, kind)).map_err(|_| Code::ParseFailed)?;
        if request.len() > protocol::MAX_REQUEST_BYTES {
            return Err(Code::ResourceLimit);
        }
        #[cfg(unix)]
        let args = vec![
            "--voxbridge-parse-worker".into(),
            "--allocation-limit".into(),
            self.allocation_bytes.to_string().into(),
        ];
        #[cfg(not(unix))]
        let args = vec!["--voxbridge-parse-worker".into()];
        let spec = ProcessSpec {
            program: self.executable.clone(),
            args,
            pipe_stdin: true,
            pipe_stdout: true,
        };
        let limits = ProcessResourceLimits {
            memory_bytes: 1024 * 1024 * 1024,
            cpu_seconds: 30,
        };
        let mut spawn = Box::pin(self.spawner.spawn_limited(spec, limits));
        let started = tokio::select! {biased;
            _=cancelled(&mut cancel)=>Err(Code::Cancelled),
            _=tokio::time::sleep_until(expires)=>Err(Code::Timeout),
            result=&mut spawn=>Ok(result),
        };
        let mut process = match started {
            Ok(result) => result.map_err(|_| Code::Configuration)?,
            Err(error) => {
                // The possibly-entered spawn future remains owned, including a
                // blocking OS spawn which a Tokio timeout cannot preempt.
                if let Ok(mut process) = spawn.await {
                    drop(process.stdin.take());
                    drop(process.stdout.take());
                    cleanup(&process.control, self.cleanup_allowance).await;
                }
                return Err(error);
            }
        };
        let mut result = tokio::select! {biased;
            _=cancelled(&mut cancel)=>Err(Code::Cancelled),
            _=tokio::time::sleep_until(expires)=>Err(Code::Timeout),
            result=exchange(&mut process,&request)=>result,
        };
        if result.is_err() {
            drop(process.stdin.take());
            drop(process.stdout.take());
            cleanup(&process.control, self.cleanup_allowance).await;
            // An I/O failure requests cleanup immediately. Only the acknowledged
            // exit may identify an allocator failure; EOF alone cannot do so.
            if result == Err(Code::ParseFailed)
                && process
                    .control
                    .wait()
                    .await
                    .is_ok_and(|exit| exit.code == Some(86))
            {
                result = Err(Code::ResourceLimit);
            }
        }
        result
    }
}
async fn exchange(process: &mut ProcessPipes, request: &[u8]) -> Result<Vec<TextBlock>, Code> {
    let mut input = process.stdin.take().ok_or(Code::ParseFailed)?;
    let output = process.stdout.as_mut().ok_or(Code::ParseFailed)?;
    let mut ready = [0; 8];
    if output.read_exact(&mut ready).await.is_err() {
        return Err(Code::ParseFailed);
    }
    if &ready != protocol::READY {
        return Err(Code::ParseFailed);
    }
    if input
        .write_all(&(request.len() as u32).to_le_bytes())
        .await
        .is_err()
        || input.write_all(request).await.is_err()
        || input.shutdown().await.is_err()
    {
        return Err(Code::ParseFailed);
    }
    // Tokio ChildStdin shutdown alone does not close the OS pipe. EOF is part
    // of the one-request protocol, so relinquish the handle before reading.
    drop(input);
    let length = match output.read_u32_le().await {
        Ok(length) => length as usize,
        Err(_) => return Err(Code::ParseFailed),
    };
    if length > MAX_OUTPUT_BYTES {
        return Err(Code::ResourceLimit);
    }
    let mut body = vec![0; length];
    if output.read_exact(&mut body).await.is_err() {
        return Err(Code::ParseFailed);
    }
    let mut extra = [0; 1];
    match output.read(&mut extra).await {
        Ok(0) => {}
        Ok(_) => return Err(Code::ParseFailed),
        Err(_) => return Err(Code::ParseFailed),
    }
    let exit = process
        .control
        .wait()
        .await
        .map_err(|_| Code::ParseFailed)?;
    if exit.code != Some(0) {
        return Err(exit_error(exit.code));
    }
    match serde_json::from_slice::<Response>(&body).map_err(|_| Code::ParseFailed)? {
        Response::Parsed { blocks } => {
            protocol::validate_blocks(&blocks)?;
            Ok(blocks)
        }
        Response::Failed { error } => Err(match error {
            Code::UnsupportedDocument | Code::ResourceLimit | Code::ParseFailed => error,
            _ => Code::ParseFailed,
        }),
    }
}
#[cfg(test)]
mod tests;
