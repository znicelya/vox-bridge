//! Real worker budget and protocol checks; included only on Unix.
use super::*;
use std::{ffi::OsString, os::unix::ffi::OsStrExt, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vox_bridge_lib::{
    audio::platform::process_io::{
        ProcessPipes, ProcessResourceLimits, ProcessSpawner, ProcessSpec, SystemProcessSpawner,
    },
    intelligence::types::IntelligenceErrorCode as Code,
    knowledge::{MAX_OUTPUT_BYTES, PARSER_ID},
};
const READY: &[u8; 8] = b"VBPW\x01\0\0\0";
fn args(limit: usize) -> Vec<OsString> {
    vec![
        "--voxbridge-parse-worker".into(),
        "--allocation-limit".into(),
        limit.to_string().into(),
    ]
}
async fn spawn(args: Vec<OsString>) -> ProcessPipes {
    SystemProcessSpawner
        .spawn_limited(
            ProcessSpec {
                program: env!("CARGO_BIN_EXE_vox-bridge").into(),
                args,
                pipe_stdin: true,
                pipe_stdout: true,
            },
            ProcessResourceLimits {
                memory_bytes: 1024 * 1024 * 1024,
                cpu_seconds: 30,
            },
        )
        .await
        .unwrap()
}
fn request(name: &str, kind: DocumentKind) -> Vec<u8> {
    let body = serde_json::to_vec(&serde_json::json!({
        "version": 1, "parser_id": PARSER_ID,
        "path": {"platform": "unix", "units": fixture(name).as_os_str().as_bytes()}, "kind": kind,
    }))
    .unwrap();
    let mut frame = (body.len() as u32).to_le_bytes().to_vec();
    frame.extend(body);
    frame
}
async fn raw_worker(arguments: Vec<OsString>, frame: &[u8]) -> (Option<i32>, Vec<u8>) {
    let mut process = spawn(arguments).await;
    let mut stdout = process.stdout.take().unwrap();
    let mut stdin = process.stdin.take().unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = vec![0; 8];
        if stdout.read_exact(&mut bytes).await.is_err() {
            bytes.clear();
        }
        if bytes == READY {
            // Only a ready worker can receive any request bytes.
            stdin.write_all(frame).await.unwrap();
        }
        drop(stdin);
        stdout
            .take((MAX_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .unwrap();
        let exit = process.control.wait().await.unwrap();
        (exit.code, bytes)
    })
    .await;
    // Even failed observation retains the owner until actual reap.
    let reaped = process.control.terminate().await.unwrap();
    let result = observed.expect("real worker must finish within its test observation");
    assert_eq!(result.0, reaped.code);
    result
}
#[tokio::test]
async fn real_budget_exits_before_and_after_readiness_without_success_json() {
    let (exit, bytes) = raw_worker(args(1), &request("unicode.txt", DocumentKind::Text)).await;
    assert_eq!(exit, Some(86));
    assert!(bytes.is_empty());
    for (file, kind) in [
        ("inflated.pdf", DocumentKind::Pdf),
        ("inflated.docx", DocumentKind::Docx),
    ] {
        assert!(std::fs::metadata(fixture(file)).unwrap().len() < 20_000);
        let (exit, bytes) = raw_worker(args(4 * 1024 * 1024), &request(file, kind)).await;
        assert_eq!(exit, Some(86), "{file}");
        assert_eq!(
            bytes, READY,
            "allocation exit must not write a success result"
        );
    }
}
#[tokio::test]
async fn real_supervisor_survives_inflation_and_maps_allocator_exit_to_resource_limit() {
    let parser = ProcessDocumentParser::with_executable(env!("CARGO_BIN_EXE_vox-bridge").into())
        .with_allocation_limit(4 * 1024 * 1024)
        .unwrap();
    let (_cancel, receiver) = watch::channel(false);
    for (file, kind) in [
        ("inflated.pdf", DocumentKind::Pdf),
        ("inflated.docx", DocumentKind::Docx),
    ] {
        assert_eq!(
            parser.parse(fixture(file), kind, receiver.clone()).await,
            Err(Code::ResourceLimit)
        );
    }
    assert!(
        parser
            .parse(fixture("unicode.txt"), DocumentKind::Text, receiver.clone())
            .await
            .is_ok()
    );
    let tiny = ProcessDocumentParser::with_executable(env!("CARGO_BIN_EXE_vox-bridge").into())
        .with_allocation_limit(1)
        .unwrap();
    assert_eq!(
        tiny.parse(fixture("unicode.txt"), DocumentKind::Text, receiver)
            .await,
        Err(Code::ResourceLimit)
    );
}
#[tokio::test]
async fn real_malformed_worker_arguments_and_trailing_frames_fail_headlessly() {
    for arguments in [
        vec!["--voxbridge-parse-worker"],
        vec!["--voxbridge-parse-worker=1"],
        vec!["--voxbridge-parse-worker", "--allocation-limit", "0"],
        vec!["--extra", "--voxbridge-parse-worker"],
        vec!["--voxbridge-parse-worker", "--allocation-limit", "+1024"],
    ] {
        let (exit, bytes) = raw_worker(arguments.into_iter().map(Into::into).collect(), b"").await;
        assert_eq!(exit, Some(2));
        assert!(bytes.is_empty());
    }
    let mut trailing = request("unicode.txt", DocumentKind::Text);
    trailing.extend(0_u32.to_le_bytes());
    let (exit, bytes) = raw_worker(args(4 * 1024 * 1024), &trailing).await;
    assert_eq!(exit, Some(2));
    assert_eq!(bytes, READY);
}
#[tokio::test]
async fn real_worker_waiting_for_request_is_terminated_and_reaped_with_replayable_exit() {
    let mut process = spawn(args(4 * 1024 * 1024)).await;
    let mut ready = [0; 8];
    let observed = tokio::time::timeout(
        Duration::from_secs(5),
        process.stdout.as_mut().unwrap().read_exact(&mut ready),
    )
    .await;
    let first = process.control.terminate().await.unwrap();
    drop(process.stdin.take());
    drop(process.stdout.take());
    assert!(observed.unwrap().is_ok());
    assert_eq!(&ready, READY);
    assert_ne!(first.code, Some(0));
    assert_eq!(process.control.wait().await.unwrap(), first);
    assert_eq!(process.control.terminate().await.unwrap(), first);
}
