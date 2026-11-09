use super::*;
use serde_json::{Value, json};
fn request(command: &str, body: Value) -> tauri::webview::InvokeRequest {
    tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap(),
        body: tauri::ipc::InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: tauri::test::INVOKE_KEY.into(),
    }
}
fn state(path: &std::path::Path) -> AppState {
    crate::app::document_test_support::with_parser(
        path,
        std::sync::Arc::new(crate::knowledge::parser::ProcessDocumentParser::new().unwrap()),
    )
}
fn session(state: &AppState) -> Uuid {
    let id = Uuid::new_v4();
    state
        .store()
        .create_session(&crate::storage::StoredSession {
            id,
            started_at: -19,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    id
}
#[test]
fn summary_commands_exact_signed_cursor_tagged_page_default_limit_and_validation() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let session = session(&state);
    for (index, version) in (-7..53).enumerate() {
        state
            .store()
            .create_summary_version(&crate::storage::SummaryVersion {
                id: Uuid::new_v4(),
                session_id: session,
                version,
                created_at: i64::MIN + index as i64,
                status: "complete-original".into(),
                structured_json: r#"{"title":"可读旧版本","text":"历史正文"}"#.into(),
            })
            .unwrap();
    }
    for version in [4294967298_i64, 9007199254740993, i64::MAX] {
        state
            .store()
            .create_summary_version(&crate::storage::SummaryVersion {
                id: Uuid::new_v4(),
                session_id: session,
                version,
                created_at: i64::MAX,
                status: "original".into(),
                structured_json: r#"{"text":"大版本"}"#.into(),
            })
            .unwrap();
    }
    let app = tauri::test::mock_builder()
        .manage(state)
        .invoke_handler(handle)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let call = |args| {
        tauri::test::get_ipc_response(&window, request("list_summaries", args))
            .map(|v| v.deserialize::<Value>().unwrap())
    };
    let page = call(json!({"sessionId":session})).unwrap();
    assert_eq!(page["rows"].as_array().unwrap().len(), 50);
    assert_eq!(page["rows"][0]["version"], "-7");
    assert_eq!(page["rows"][0]["origin"], "legacy");
    assert_eq!(page["rows"][0]["created_at_seconds"], i64::MIN.to_string());
    assert_eq!(page["rows"][0]["created_at_ms"], Value::Null);
    assert_eq!(page["next_cursor"], "42");
    let page =
        call(json!({"sessionId":session,"afterVersion":"9007199254740993","limit":1})).unwrap();
    assert_eq!(page["rows"][0]["version"], i64::MAX.to_string());
    for limit in [0, 101] {
        assert!(call(json!({"sessionId":session,"limit":limit})).is_err());
    }
    for cursor in [
        json!(0),
        json!("+1"),
        json!("01"),
        json!("-0"),
        json!("9223372036854775808"),
        json!(" 1"),
    ] {
        assert!(call(json!({"sessionId":session,"afterVersion":cursor})).is_err());
    }
    assert_eq!(
        COMMANDS,
        &["generate_summary", "list_summaries", "delete_summary"]
    );
}
#[test]
fn summary_commands_startup_opens_real_manual_readiness_and_native_terminal_projection() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let session = session(&state);
    let observer = state.clone();
    assert!(tauri::async_runtime::block_on(state.generate_summary(session, None)).is_err());
    tauri::async_runtime::block_on(state.start_intelligence()).unwrap();
    let app = tauri::test::mock_builder()
        .manage(state)
        .invoke_handler(handle)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let call = |cmd, args| {
        tauri::test::get_ipc_response(&window, request(cmd, args))
            .map(|v| v.deserialize::<Value>().unwrap())
    };
    let job = call("generate_summary", json!({"sessionId":session}));
    if job.is_err() {
        tauri::async_runtime::block_on(observer.close_all()).unwrap();
    }
    let job = job
        .expect("successful app startup must open local summary readiness before manual admission");
    let id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while matches!(
                observer.get_job(id).unwrap().unwrap().status,
                crate::intelligence::types::JobStatus::Queued
                    | crate::intelligence::types::JobStatus::Running
            ) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
    })
    .unwrap();
    let page = call("list_summaries", json!({"sessionId":session})).unwrap();
    assert_eq!(page["rows"][0]["origin"], "native");
    assert_eq!(page["rows"][0]["version"], 1);
    assert_eq!(page["rows"][0]["status"], "failed");
    assert_eq!(page["rows"][0]["error"], "no_final_text");
    assert_eq!(page["rows"][0]["input_final_count"], 0);
    assert_eq!(
        call("delete_summary", json!({"summaryId":job["object_id"]})).unwrap(),
        Value::Null
    );
    assert!(
        call("list_summaries", json!({"sessionId":session})).unwrap()["rows"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    tauri::async_runtime::block_on(observer.close_all()).unwrap();
    assert!(tauri::async_runtime::block_on(observer.generate_summary(session, None)).is_err());
}
