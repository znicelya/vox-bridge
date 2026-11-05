use super::linux::LinuxAudioDriver;
use super::macos::MacOsAudioDriver;
use super::windows::WindowsAudioDriver;
use super::*;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct MockRunner {
    outputs: Arc<Mutex<VecDeque<CommandOutput>>>,
    calls: Arc<Mutex<Vec<(String, Vec<String>, bool)>>>,
}

impl MockRunner {
    fn with_outputs(outputs: impl IntoIterator<Item = CommandOutput>) -> Self {
        Self {
            outputs: Arc::new(Mutex::new(outputs.into_iter().collect())),
            calls: Arc::default(),
        }
    }

    fn calls(&self) -> Vec<(String, Vec<String>, bool)> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for MockRunner {
    fn run(&self, program: &Path, args: &[OsString], privileged: bool) -> AppResult<CommandOutput> {
        self.calls.lock().unwrap().push((
            program.to_string_lossy().into_owned(),
            args.iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            privileged,
        ));
        Ok(self.outputs.lock().unwrap().pop_front().unwrap_or_default())
    }
}

fn output(stdout: &str) -> CommandOutput {
    CommandOutput {
        status: Some(0),
        stdout: stdout.into(),
        stderr: String::new(),
    }
}

#[test]
fn linux_is_ready_only_when_both_logical_endpoints_exist() {
    let runner = MockRunner::with_outputs([
        output(""),
        output(""),
        output(""),
        output(""),
        output(""),
        output(""),
        output(""),
    ]);
    let manager = LinuxAudioDriver::new(runner);
    assert_eq!(
        manager.status().unwrap(),
        DriverStatus::Failed(
            "VoxBridge Input and VoxBridge Output endpoints are not both available".into()
        )
    );
}

#[test]
fn linux_status_maps_pipewire_and_pulseaudio_endpoint_names() {
    let runner = MockRunner::with_outputs([
        output(""),
        output("42\tVoxBridge_Input\tmodule-remap-source.c\ts16le 2ch 48000Hz\tIDLE\n"),
        output("43\tVoxBridge_Output\tmodule-null-sink.c\ts16le 2ch 48000Hz\tRUNNING\n"),
        output(""),
        output("42\tVoxBridge_Input\tmodule-remap-source.c\ts16le 2ch 48000Hz\tIDLE\n"),
        output("43\tVoxBridge_Output\tmodule-null-sink.c\ts16le 2ch 48000Hz\tRUNNING\n"),
    ]);
    let manager = LinuxAudioDriver::new(runner);
    assert_eq!(manager.status().unwrap(), DriverStatus::Ready);
    let endpoints = manager.list_endpoints().unwrap();
    assert!(
        endpoints
            .iter()
            .any(|endpoint| endpoint.id == "VoxBridge_Input" && endpoint.ready)
    );
    assert!(
        endpoints
            .iter()
            .any(|endpoint| endpoint.id == "VoxBridge_Output" && endpoint.ready)
    );
}

#[test]
fn windows_install_is_gated_when_signed_package_is_unavailable() {
    let runner = MockRunner::default();
    let manager = WindowsAudioDriver::new(runner.clone());
    assert!(
        matches!(manager.install().unwrap(), DriverStatus::Failed(message) if message.contains("signed endpoint package is unavailable"))
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn macos_install_is_gated_when_extension_license_is_unavailable() {
    let runner = MockRunner::default();
    let manager = MacOsAudioDriver::new(runner.clone());
    assert!(
        matches!(manager.install().unwrap(), DriverStatus::NeedsApproval(message) if message.contains("extension package is unavailable"))
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn linux_install_uses_non_privileged_pactl_commands() {
    let runner = MockRunner::with_outputs([
        output(""),
        output(""),
        output(""),
        output(""),
        output(""),
        output(""),
        output(""),
    ]);
    let manager = LinuxAudioDriver::new(runner.clone());
    let _ = manager.install();
    assert!(runner.calls().iter().all(|(_, _, privileged)| !privileged));
    assert!(
        runner
            .calls()
            .iter()
            .any(|(program, _, _)| program == "pactl")
    );
}
