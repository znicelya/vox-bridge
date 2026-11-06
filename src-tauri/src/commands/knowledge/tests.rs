use super::*;
use serde_json::{Value, json};
use std::sync::Arc;
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
#[test]
fn documents_commands_real_dispatch_preserves_legacy_dtos_and_exact_camelcase_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(crate::config::SecretStore::for_tests(dir.path()).unwrap());
    let fixture = crate::session::test_support::RuntimeHarness::live(
        crate::providers::fixture::FixtureScript::with_output_audio(
            crate::audio::PcmSpec::pcm16_mono(24000),
            [],
        )
        .unwrap(),
    )
    .unwrap();
    let state = AppState::with_dependencies(
        dir.path().into(),
        secrets.clone(),
        fixture.audio.clone(),
        Arc::new(crate::providers::factory::ProviderFactory::new(secrets)),
    )
    .unwrap();
    let id = Uuid::new_v4();
    state.store().transaction(|tx|{tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'legacy-hash','legacy-reference',NULL,37)",[id.to_string()])?;Ok(())}).unwrap();
    let app = tauri::test::mock_builder()
        .manage(state)
        .invoke_handler(handle)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let call = |cmd: &str, args: Value| {
        tauri::test::get_ipc_response(&window, request(cmd, args))
            .map(|v| v.deserialize::<Value>().unwrap())
    };
    let page = call("list_documents", json!({"after":null,"limit":64})).unwrap();
    assert_eq!(
        page["rows"].as_array().unwrap().len(),
        1,
        "legacy row must be projected by actual command"
    );
    let detail = call("get_document", json!({"documentId":id})).unwrap();
    assert_eq!(detail["legacy_uri"], "legacy-reference");
    assert_eq!(detail["summary"]["title"], Value::Null);
    assert_eq!(detail["summary"]["updated_at_ms"], Value::Null);
    assert_eq!(detail["summary"]["origin"], "legacy");
    assert_eq!(detail["original_path"], Value::Null);
    assert_eq!(detail["managed_path"], Value::Null);
    assert_eq!(
        call("get_document", json!({"documentId":Uuid::new_v4()})).unwrap(),
        Value::Null
    );
    assert!(call("get_document", json!({"document_id":id})).is_err());
    assert!(call("get_document", json!({"documentId":"invalid"})).is_err());
    assert!(call("list_documents", json!({"after":null,"limit":0})).is_err());
    assert!(
        call(
            "import_document",
            json!({"path":dir.path().join("unread.txt")})
        )
        .is_err()
    );
    assert!(
        call(
            "replace_document",
            json!({"documentId":id,"path":dir.path().join("unread.txt")})
        )
        .is_err()
    );
    assert!(!dir.path().join("knowledge").exists());
}

#[test]
fn documents_commands_real_import_replace_and_duplicate_use_registered_document_service() {
    use crate::intelligence::types::{IntelligenceErrorCode as Code, JobStatus};
    use crate::knowledge::{
        parser::DocumentParser,
        types::{DocumentKind, TextBlock, TextLocation},
    };
    use tokio::sync::watch;
    struct Parser;
    #[async_trait::async_trait]
    impl DocumentParser for Parser {
        async fn parse(
            &self,
            path: PathBuf,
            _: DocumentKind,
            _: watch::Receiver<bool>,
        ) -> Result<Vec<TextBlock>, Code> {
            let text = std::fs::read_to_string(path).map_err(|_| Code::ParseFailed)?;
            let end_char = text.chars().count() as u32;
            Ok(vec![TextBlock {
                text,
                location: TextLocation {
                    page: None,
                    paragraph: 1,
                    heading: Some("源文档 heading".into()),
                    start_char: 0,
                    end_char,
                },
            }])
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let state = crate::app::document_test_support::with_parser(dir.path(), Arc::new(Parser));
    tauri::async_runtime::block_on(state.start_intelligence()).unwrap();
    let observer = state.clone();
    let app = tauri::test::mock_builder()
        .manage(state)
        .invoke_handler(handle)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let call = |cmd: &str, args: Value| {
        tauri::test::get_ipc_response(&window, request(cmd, args))
            .map(|v| v.deserialize::<Value>().unwrap())
    };
    let wait = |id: Uuid| {
        tauri::async_runtime::block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    if observer.get_job(id).unwrap().unwrap().status == JobStatus::Succeeded {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        })
    };
    let path = dir.path().join("source.txt");
    std::fs::write(&path, "first canonical text").unwrap();
    let job = call("import_document", json!({"path":path})).unwrap();
    let job_id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    let document = Uuid::parse_str(job["object_id"].as_str().unwrap()).unwrap();
    wait(job_id);
    let detail = call("get_document", json!({"documentId":document})).unwrap();
    assert_eq!(detail["summary"]["status"], "parsed");
    assert_eq!(detail["summary"]["version"], 1);
    assert_eq!(detail["summary"]["origin"], "native");
    assert!(detail["managed_path"].is_string());
    assert_eq!(detail["legacy_uri"], Value::Null);
    let replacement = dir.path().join("replacement.txt");
    std::fs::write(&replacement, "replacement canonical text").unwrap();
    let next = call(
        "replace_document",
        json!({"documentId":document,"path":replacement}),
    )
    .unwrap();
    wait(Uuid::parse_str(next["id"].as_str().unwrap()).unwrap());
    assert_ne!(next["id"], job["id"]);
    assert_eq!(next["object_id"], job["object_id"]);
    assert_eq!(
        call(
            "replace_document",
            json!({"documentId":document,"path":replacement})
        )
        .unwrap()["id"],
        next["id"]
    );
    assert_eq!(
        call("import_document", json!({"path":replacement})).unwrap()["id"],
        next["id"]
    );
    let detail = call("get_document", json!({"documentId":document})).unwrap();
    assert_eq!(detail["summary"]["version"], 2);
    assert_eq!(detail["summary"]["chunk_count"], 1);
    assert_eq!(
        call("list_documents", json!({"after":null,"limit":64})).unwrap()["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    tauri::async_runtime::block_on(observer.close_all()).unwrap();
}
