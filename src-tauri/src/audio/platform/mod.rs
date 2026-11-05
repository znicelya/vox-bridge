//! Platform audio driver lifecycle contracts.
//!
//! Installers are deliberately invoked only by an explicit caller. Tauri startup does not
//! install drivers or request elevation. Platform packages are represented in the resource
//! manifest and unavailable packages return a typed state instead of pretending to be ready.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod linux;
pub mod macos;
pub mod windows;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverStatus {
    NeedsPrivileges,
    NeedsApproval(String),
    NeedsRestart,
    Failed(String),
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioEndpoint {
    pub id: String,
    pub display_name: String,
    pub direction: AudioDirection,
    pub sample_rates: Vec<u32>,
    pub channels: u16,
    pub ready: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub unavailable: bool,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput> {
        if privileged {
            return Err(AppError::InvalidRequest(
                "privileged audio commands require an approved platform helper".into(),
            ));
        }
        let output = match Command::new(program).args(args).output() {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CommandOutput {
                    status: None,
                    stdout: String::new(),
                    stderr: error.to_string(),
                    unavailable: true,
                });
            }
            Err(error) => {
                return Err(AppError::Internal(format!(
                    "failed to run {}: {error}",
                    program.display()
                )));
            }
        };
        Ok(CommandOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            unavailable: false,
        })
    }
}

pub trait AudioDriverManager {
    fn status(&self) -> AppResult<DriverStatus>;
    fn install(&self) -> AppResult<DriverStatus>;
    fn repair(&self) -> AppResult<DriverStatus>;
    fn uninstall(&self) -> AppResult<DriverStatus>;
    fn list_endpoints(&self) -> AppResult<Vec<AudioEndpoint>>;
}

fn command_failed(output: &CommandOutput, command: &str) -> Option<DriverStatus> {
    match output.status {
        Some(0) => None,
        Some(status) => Some(DriverStatus::Failed(format!(
            "{command} exited with status {status}: {}",
            output.stderr.trim()
        ))),
        None => Some(DriverStatus::Failed(format!(
            "{command} did not exit normally: {}",
            output.stderr.trim()
        ))),
    }
}

fn command(path: &str, args: &[&str]) -> (PathBuf, Vec<OsString>) {
    (
        PathBuf::from(path),
        args.iter().map(OsString::from).collect(),
    )
}

pub(crate) fn run_command<R: CommandRunner>(
    runner: &R,
    path: &str,
    args: &[&str],
) -> AppResult<CommandOutput> {
    let (program, args) = command(path, args);
    runner.run(&program, &args, false)
}

pub(crate) fn endpoint_pair_ready(endpoints: &[AudioEndpoint]) -> bool {
    endpoints.iter().any(|endpoint| {
        endpoint.display_name == "VoxBridge Input"
            && endpoint.direction == AudioDirection::Input
            && endpoint.ready
    }) && endpoints.iter().any(|endpoint| {
        endpoint.display_name == "VoxBridge Output"
            && endpoint.direction == AudioDirection::Output
            && endpoint.ready
    })
}

pub(crate) fn readiness(endpoints: &[AudioEndpoint]) -> DriverStatus {
    if endpoint_pair_ready(endpoints) {
        DriverStatus::Ready
    } else {
        DriverStatus::Failed(
            "VoxBridge Input and VoxBridge Output endpoints are not both available".into(),
        )
    }
}
