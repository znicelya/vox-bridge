//! Shell is an interpreter only; PATH supplies a fake pactl and never calls audio.
#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt, process::Command};
fn run_script(name: &str, fail: &str) -> (bool, String) {
    let dir = tempfile::tempdir().unwrap();
    let pactl = dir.path().join("pactl");
    fs::write(&pactl, r#"#!/bin/sh
printf '%s\n' "$*" >> "$VOX_TEST_CALLS"
if [ "$1 $2 $3" = 'list short modules' ]; then
  [ "$VOX_TEST_FAIL" != list ] || exit 1
  printf '%s\n' '1 module-null-sink sink_name=VoxBridge_M1' '2 module-null-sink sink_name=VoxBridge_Output_Backup' '3 module-remap-source source_name=VoxBridge_Input master=VoxBridge_M1.monitor' '4 module-null-sink sink_name=VoxBridge_Output'
elif [ "$1" = unload-module ] && [ "$VOX_TEST_FAIL" = unload ]; then
  exit 1
fi
"#).unwrap();
    fs::set_permissions(&pactl, fs::Permissions::from_mode(0o755)).unwrap();
    let calls = dir.path().join("calls");
    let status = Command::new("/bin/sh")
        .arg(format!(
            "{}/resources/audio/linux/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
        .env("VOX_TEST_CALLS", &calls)
        .env("VOX_TEST_FAIL", fail)
        .output()
        .unwrap()
        .status;
    (status.success(), fs::read_to_string(calls).unwrap())
}
#[test]
fn setup_scripts_order_dependencies_and_preserve_property_quotes() {
    for name in ["setup-pipewire.sh", "setup-pulseaudio.sh"] {
        let (success, calls) = run_script(name, "");
        assert!(success);
        let calls: Vec<_> = calls.lines().collect();
        assert_eq!(
            &calls[1..4],
            ["unload-module 3", "unload-module 1", "unload-module 4"]
        );
        assert!(
            calls[4].ends_with(r#"sink_properties="device.description='VoxBridge M1 Backend'""#)
        );
        assert!(calls[5].ends_with(r#"source_properties="device.description='VoxBridge Input'""#));
        assert!(calls[6].ends_with(r#"sink_properties="device.description='VoxBridge Output'""#));
        assert_eq!(calls.len(), 7);
    }
}
#[test]
fn setup_scripts_stop_after_failed_inventory_or_unload_without_loading_modules() {
    for name in ["setup-pipewire.sh", "setup-pulseaudio.sh"] {
        for failure in ["list", "unload"] {
            let (success, calls) = run_script(name, failure);
            assert!(!success);
            assert!(!calls.lines().any(|call| call.starts_with("load-module")));
        }
    }
}
