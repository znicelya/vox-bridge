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
        unavailable: false,
    }
}

fn unavailable(stderr: &str) -> CommandOutput {
    CommandOutput {
        status: None,
        stdout: String::new(),
        stderr: stderr.into(),
        unavailable: true,
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
fn linux_failed_probe_does_not_report_endpoints_from_command_output() {
    let runner = MockRunner::with_outputs([
        CommandOutput {
            status: Some(1),
            stdout: "VoxBridge_Input VoxBridge_Output".into(),
            stderr: "not connected".into(),
            unavailable: false,
        },
        CommandOutput {
            status: Some(1),
            stdout: "VoxBridge_Input".into(),
            stderr: "not connected".into(),
            unavailable: false,
        },
        output("VoxBridge_Output"),
    ]);
    let manager = LinuxAudioDriver::new(runner);
    assert!(manager.status().is_err());
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
fn linux_pulseaudio_status_falls_back_when_pw_cli_is_unavailable() {
    let runner = MockRunner::with_outputs([
        unavailable("pw-cli not installed"),
        output("42\tVoxBridge_Input\tmodule-remap-source.c\t\tIDLE\n"),
        output("43\tVoxBridge_Output\tmodule-null-sink.c\t\tIDLE\n"),
    ]);
    assert_eq!(
        LinuxAudioDriver::new(runner).status().unwrap(),
        DriverStatus::Ready
    );
}

#[test]
fn linux_pactl_probe_failure_does_not_parse_stdout_as_endpoints() {
    let runner = MockRunner::with_outputs([
        unavailable("pw-cli not installed"),
        CommandOutput {
            status: Some(1),
            stdout: "VoxBridge_Input".into(),
            stderr: "server unavailable".into(),
            unavailable: false,
        },
        output("VoxBridge_Output"),
    ]);
    assert!(LinuxAudioDriver::new(runner).status().is_err());
}

#[test]
fn command_without_exit_status_is_a_failure() {
    let failure = command_failed(
        &CommandOutput {
            status: None,
            stdout: String::new(),
            stderr: "terminated".into(),
            unavailable: false,
        },
        "test",
    );
    assert!(
        matches!(failure, Some(DriverStatus::Failed(message)) if message.contains("terminated"))
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
    let calls = runner.calls();
    assert!(calls.iter().any(
        |(_, args, _)| args.first().is_some_and(|arg| arg == "load-module")
            && args.iter().any(|arg| arg == "sink_name=VoxBridge_M1")
    ));
    assert!(calls.iter().any(
        |(_, args, _)| args.first().is_some_and(|arg| arg == "load-module")
            && args.iter().any(|arg| arg == "sink_name=VoxBridge_Output")
    ));
}

#[test]
fn repeated_linux_install_reconciles_existing_modules_without_duplicates() {
    let existing = "7 module-null-sink sink_name=VoxBridge_M1\n8 module-remap-source source_name=VoxBridge_Input master=VoxBridge_M1.monitor\n9 module-null-sink sink_name=VoxBridge_Output\n";
    let runner = MockRunner::with_outputs([
        output(""),
        output("10"),
        output("11"),
        output("12"),
        unavailable("pw-cli not installed"),
        output("42\tVoxBridge_Input\tmodule-remap-source.c\t\tIDLE\n"),
        output("43\tVoxBridge_Output\tmodule-null-sink.c\t\tIDLE\n"),
        output(existing),
        unavailable("pw-cli not installed"),
        output("42\tVoxBridge_Input\tmodule-remap-source.c\t\tIDLE\n"),
        output("43\tVoxBridge_Output\tmodule-null-sink.c\t\tIDLE\n"),
    ]);
    let manager = LinuxAudioDriver::new(runner.clone());
    assert_eq!(manager.install().unwrap(), DriverStatus::Ready);
    assert_eq!(manager.install().unwrap(), DriverStatus::Ready);
    let load_count = runner
        .calls()
        .iter()
        .filter(|(_, args, _)| args.first().is_some_and(|arg| arg == "load-module"))
        .count();
    assert_eq!(load_count, 3);
}

#[test]
fn duplicate_exact_modules_are_reconciled_but_similarly_named_modules_are_kept() {
    let existing = "7 module-null-sink sink_name=VoxBridge_M1\n8 module-null-sink sink_name=VoxBridge_M1\n9 module-remap-source source_name=VoxBridge_Input master=VoxBridge_M1.monitor\n10 module-null-sink sink_name=VoxBridge_Output\n11 module-null-sink sink_name=VoxBridge_Output_Backup\n";
    let runner = MockRunner::with_outputs([
        output(existing),
        output(""),
        output(""),
        output(""),
        output(""),
        output("10"),
        output("11"),
        output("12"),
        unavailable("pw-cli not installed"),
        output("42\tVoxBridge_Input\tmodule-remap-source.c\t\tIDLE\n"),
        output("43\tVoxBridge_Output\tmodule-null-sink.c\t\tIDLE\n"),
    ]);
    let manager = LinuxAudioDriver::new(runner.clone());
    assert_eq!(manager.install().unwrap(), DriverStatus::Ready);
    let unloads = runner
        .calls()
        .into_iter()
        .filter(|(_, args, _)| args.first().is_some_and(|arg| arg == "unload-module"))
        .collect::<Vec<_>>();
    assert_eq!(unloads.len(), 4);
    assert_eq!(
        unloads
            .iter()
            .map(|(_, args, _)| args[1].as_str())
            .collect::<Vec<_>>(),
        ["9", "7", "8", "10"]
    );
}

#[test]
fn invalid_module_ids_are_never_passed_to_unload_module() {
    let runner = MockRunner::with_outputs([
        output(
            "oops module-null-sink sink_name=VoxBridge_Output\n8 module-remap-source source_name=VoxBridge_Input\n",
        ),
        output(""),
    ]);
    assert_eq!(
        LinuxAudioDriver::new(runner.clone()).uninstall().unwrap(),
        DriverStatus::Ready
    );
    let unloads = runner
        .calls()
        .into_iter()
        .filter(|(_, args, _)| args.first().is_some_and(|arg| arg == "unload-module"))
        .collect::<Vec<_>>();
    assert_eq!(unloads.len(), 1);
    assert_eq!(unloads[0].1, ["unload-module", "8"]);
}

#[test]
fn linux_uninstall_uses_module_ids_and_only_removes_voxbridge_modules() {
    let runner = MockRunner::with_outputs([
        output(
            "7 module-null-sink sink_name=VoxBridge_Output\n8 module-remap-source source_name=VoxBridge_Input\n9 module-null-sink sink_name=OtherApp\n",
        ),
        output(""),
        output(""),
    ]);
    let manager = LinuxAudioDriver::new(runner.clone());
    assert_eq!(manager.uninstall().unwrap(), DriverStatus::Ready);
    let unloads = runner
        .calls()
        .into_iter()
        .filter(|(_, args, _)| args.first().is_some_and(|arg| arg == "unload-module"))
        .collect::<Vec<_>>();
    assert_eq!(unloads.len(), 2);
    assert_eq!(unloads[0].1, ["unload-module", "8"]);
    assert_eq!(unloads[1].1, ["unload-module", "7"]);
}
