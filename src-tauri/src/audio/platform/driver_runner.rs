//! The synchronous manager calls this adapter only on its one owner thread.
//! Deadlines bound utility execution, not ownership: uncertain children retain the
//! owner and maintenance lease until exit is acknowledged.
use super::{
    CommandOutput, CommandRunner,
    process_io::{ProcessControl, ProcessSpawner, ProcessSpec},
};
use crate::{
    domain::transcript::RuntimeFailureCode,
    error::{AppError, AppResult},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::AsyncReadExt;

#[derive(Clone, Copy)]
pub(crate) struct DriverLimits {
    pub operation: Duration,
    pub command: Duration,
    pub reap: Duration,
    pub output_bytes: usize,
}
impl Default for DriverLimits {
    fn default() -> Self {
        Self {
            operation: Duration::from_secs(15),
            command: Duration::from_secs(2),
            reap: Duration::from_secs(2),
            output_bytes: 1_048_576,
        }
    }
}
pub(crate) struct BoundedCommandRunner {
    pub(super) runtime: tokio::runtime::Handle,
    pub(super) spawner: Arc<dyn ProcessSpawner>,
    pub(super) expires: Instant,
    pub(super) limits: DriverLimits,
}
impl CommandRunner for BoundedCommandRunner {
    fn run(&self, program: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput> {
        if privileged {
            return Err(failure("driver authorization helper is unavailable"));
        }
        self.runtime.block_on(run_utility(
            self.spawner.clone(),
            program.into(),
            args.into(),
            self.expires,
            self.limits,
        ))
    }
}
fn failure(message: &str) -> AppError {
    AppError::Internal(message.into())
}

// A returned Err from termination/wait is not an exit acknowledgement. Retain
// the control and operation indefinitely if the supervisor cannot prove exit.
async fn terminate_and_observe(control: Arc<dyn ProcessControl>, reap: Duration) {
    let termination = control.terminate();
    tokio::pin!(termination);
    let result = match tokio::time::timeout(reap, &mut termination).await {
        Ok(result) => result,
        Err(_) => termination.await,
    };
    if result.is_err() && control.wait().await.is_err() {
        std::future::pending::<()>().await;
    }
}

pub(crate) async fn run_utility(
    spawner: Arc<dyn ProcessSpawner>,
    program: PathBuf,
    args: Vec<OsString>,
    operation_expires: Instant,
    limits: DriverLimits,
) -> AppResult<CommandOutput> {
    if Instant::now() >= operation_expires {
        return Err(failure("driver operation timed out"));
    }
    let expires =
        tokio::time::Instant::from_std(operation_expires.min(Instant::now() + limits.command));
    let spawn = spawner.spawn(ProcessSpec {
        program,
        args,
        pipe_stdin: false,
        pipe_stdout: true,
    });
    tokio::pin!(spawn);
    let (spawned, late) = match tokio::time::timeout_at(expires, &mut spawn).await {
        Ok(result) => (result, false),
        // Never cancel a spawn whose OS ownership may already have begun.
        Err(_) => (spawn.await, true),
    };
    let mut process = match spawned {
        Ok(process) => process,
        Err(AppError::Audio {
            code: RuntimeFailureCode::AudioBackendUnavailable,
            ..
        }) if !late => {
            return Ok(CommandOutput {
                unavailable: true,
                ..Default::default()
            });
        }
        Err(_) => return Err(failure("audio utility could not start")),
    };
    if late {
        terminate_and_observe(process.control.clone(), limits.reap).await;
        return Err(failure("audio utility spawn timed out"));
    }
    let result = tokio::time::timeout_at(expires, async {
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| failure("audio utility output unavailable"))?;
        let mut bytes = Vec::new();
        stdout
            .take(limits.output_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| failure("audio utility output failed"))?;
        if bytes.len() > limits.output_bytes {
            return Err(failure("audio utility output exceeded limit"));
        }
        let exit = process
            .control
            .wait()
            .await
            .map_err(|_| failure("audio utility exit is unconfirmed"))?;
        Ok(CommandOutput {
            status: exit.code,
            stdout: String::from_utf8_lossy(&bytes).into_owned(),
            stderr: String::new(),
            unavailable: false,
        })
    })
    .await
    .unwrap_or_else(|_| Err(failure("audio utility timed out")));
    if result.is_err() {
        terminate_and_observe(process.control.clone(), limits.reap).await;
    }
    result
}
