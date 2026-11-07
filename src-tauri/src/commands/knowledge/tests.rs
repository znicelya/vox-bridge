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
    state
        .store()
        .transaction(|tx| {
            tx.execute(
                "INSERT INTO documents (id,sha256,uri,title,\
                    created_at) VALUES (?1,'legacy-hash',\
                    'legacy-reference',NULL,37)",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
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
    let page = call(
        "list_documents",
        json!({
        "after":null,
        "limit":64}
        ),
    )
    .unwrap();
    assert_eq!(
        page["rows"].as_array().unwrap().len(),
        1,
        "legacy row must be projected by actual command"
    );
    let detail = call(
        "get_document",
        json!({
        "documentId":id}
        ),
    )
    .unwrap();
    assert_eq!(detail["legacy_uri"], "legacy-reference");
    assert_eq!(detail["summary"]["title"], Value::Null);
    assert_eq!(detail["summary"]["updated_at_ms"], Value::Null);
    assert_eq!(detail["summary"]["origin"], "legacy");
    assert_eq!(detail["original_path"], Value::Null);
    assert_eq!(detail["managed_path"], Value::Null);
    assert_eq!(
        call(
            "get_document",
            json!({
            "documentId":Uuid::new_v4()}
            )
        )
        .unwrap(),
        Value::Null
    );
    assert!(
        call(
            "get_document",
            json!({
            "document_id":id}
            )
        )
        .is_err()
    );
    assert!(
        call(
            "get_document",
            json!({
            "documentId":"invalid"}
            )
        )
        .is_err()
    );
    assert!(
        call(
            "list_documents",
            json!({
            "after":null,
            "limit":0}
            )
        )
        .is_err()
    );
    assert!(
        call(
            "import_document",
            json!({
            "path":dir.path().join("unread.txt")}
            )
        )
        .is_err()
    );
    assert!(
        call(
            "replace_document",
            json!({
            "documentId":id,
            "path":dir.path().join("unread.txt")}
            )
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
    let job = call(
        "import_document",
        json!({
        "path":path}
        ),
    )
    .unwrap();
    let job_id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    let document = Uuid::parse_str(job["object_id"].as_str().unwrap()).unwrap();
    wait(job_id);
    let detail = call(
        "get_document",
        json!({
        "documentId":document}
        ),
    )
    .unwrap();
    assert_eq!(detail["summary"]["status"], "parsed");
    assert_eq!(detail["summary"]["version"], 1);
    assert_eq!(detail["summary"]["origin"], "native");
    assert!(detail["managed_path"].is_string());
    assert_eq!(detail["legacy_uri"], Value::Null);
    let replacement = dir.path().join("replacement.txt");
    std::fs::write(&replacement, "replacement canonical text").unwrap();
    let next = call(
        "replace_document",
        json!({
        "documentId":document,
        "path":replacement}
        ),
    )
    .unwrap();
    wait(Uuid::parse_str(next["id"].as_str().unwrap()).unwrap());
    assert_ne!(next["id"], job["id"]);
    assert_eq!(next["object_id"], job["object_id"]);
    assert_eq!(
        call(
            "replace_document",
            json!({
            "documentId":document,
            "path":replacement}
            )
        )
        .unwrap()["id"],
        next["id"]
    );
    assert_eq!(
        call(
            "import_document",
            json!({
            "path":replacement}
            )
        )
        .unwrap()["id"],
        next["id"]
    );
    let detail = call(
        "get_document",
        json!({
        "documentId":document}
        ),
    )
    .unwrap();
    assert_eq!(detail["summary"]["version"], 2);
    assert_eq!(detail["summary"]["chunk_count"], 1);
    assert_eq!(
        call(
            "list_documents",
            json!({
            "after":null,
            "limit":64}
            )
        )
        .unwrap()["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    tauri::async_runtime::block_on(observer.close_all()).unwrap();
}
#[test]
fn knowledge_commands_dispatch_empty_search_and_validate_exact_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::app::document_test_support::with_parser(dir.path(), Arc::new(CommandParser));
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
    let empty = call(
        "search_knowledge",
        json!({
        "request":{
        "query":"voltage",
        "selected_segment_ids":[],
        "session_id":null,
        "limit":5}
        }
        ),
    )
    .unwrap();
    assert_eq!(
        empty,
        json!({
        "hits":[],
        "identity":null,
        "unavailable_documents":0}
        )
    );
    assert!(
        call(
            "search_knowledge",
            json!({
            "request":{
            "query":"",
            "selected_segment_ids":[],
            "session_id":null,
            "limit":5}
            }
            )
        )
        .is_err()
    );
    assert!(
        call(
            "search_knowledge",
            json!({
            "request":{
            "query":"voltage",
            "selected_segment_ids":[],
            "session_id":null,
            "limit":21}
            }
            )
        )
        .is_err()
    );
    assert!(
        call(
            "reindex_document",
            json!({
            "documentId":Uuid::new_v4()}
            )
        )
        .is_err()
    );
    assert!(
        call(
            "delete_document",
            json!({
            "document_id":Uuid::new_v4()}
            )
        )
        .is_err()
    );
    tauri::async_runtime::block_on(observer.close_all()).unwrap();
}
struct CommandParser;
#[async_trait::async_trait]
impl crate::knowledge::parser::DocumentParser for CommandParser {
    async fn parse(
        &self,
        path: PathBuf,
        _: crate::knowledge::types::DocumentKind,
        _: tokio::sync::watch::Receiver<bool>,
    ) -> Result<
        Vec<crate::knowledge::types::TextBlock>,
        crate::intelligence::types::IntelligenceErrorCode,
    > {
        let text = std::fs::read_to_string(path).unwrap();
        let end = text.chars().count() as u32;
        Ok(vec![crate::knowledge::types::TextBlock {
            text,
            location: crate::knowledge::types::TextLocation {
                page: None,
                paragraph: 1,
                heading: None,
                start_char: 0,
                end_char: end,
            },
        }])
    }
}
#[derive(Default)]
struct CommandAxes(std::sync::Mutex<Vec<Vec<String>>>);
#[async_trait::async_trait]
impl crate::knowledge::embedding::Embeddings for CommandAxes {
    async fn embed(
        &self,
        _: &crate::intelligence::types::ModelProfile,
        inputs: &[String],
        _: crate::knowledge::embedding::EmbeddingExecution,
    ) -> crate::error::AppResult<Vec<Vec<f32>>> {
        self.0.lock().unwrap().push(inputs.to_vec());
        Ok(inputs
            .iter()
            .map(|_| {
                let mut vector = vec![0.; 128];
                vector[0] = 1.;
                vector
            })
            .collect())
    }
}
#[test]
fn knowledge_commands_selected_partial_multi_row_and_foreign_session_reach_owned_service() {
    use crate::intelligence::{test_support::test_llm_profile, types::*};
    let dir = tempfile::tempdir().unwrap();
    let embeddings = Arc::new(CommandAxes::default());
    let state = crate::app::document_test_support::with_knowledge_ports(
        dir.path(),
        Arc::new(CommandParser),
        embeddings.clone(),
    );
    let observer = state.clone();
    tauri::async_runtime::block_on(async {
        let profile = ModelProfile {
            kind: ServiceKind::Embedding,
            max_context_tokens: None,
            max_output_tokens: None,
            dimensions: Some(128),
            ..test_llm_profile()
        };
        state
            .save_model_profile(SaveModelProfile {
                profile: profile.clone(),
                credential: crate::app::CredentialChange::Replace {
                    api_key: "synthetic".into(),
                },
            })
            .await
            .unwrap();
        let mut settings = state.get_intelligence_settings().await.unwrap();
        settings.embedding_profile_id = Some(profile.id);
        state.save_intelligence_settings(settings).await.unwrap();
        state.start_intelligence().await.unwrap();
    });
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
    let path = dir.path().join("pump.txt");
    std::fs::write(&path, "rated voltage 24V").unwrap();
    let job = call(
        "import_document",
        json!({
        "path":path}
        ),
    )
    .unwrap();
    let document = Uuid::parse_str(job["object_id"].as_str().unwrap()).unwrap();
    tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if observer
                    .get_document(document)
                    .unwrap()
                    .unwrap()
                    .summary
                    .status
                    == crate::knowledge::types::DocumentStatus::Ready
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    });
    let session = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    for id in [session, foreign] {
        observer
            .store()
            .create_session(&crate::storage::StoredSession {
                id,
                started_at: 1,
                mode: "live".into(),
                direction: "source_to_target".into(),
                source_language: "en".into(),
                target_language: "zh".into(),
            })
            .unwrap();
    }
    let mut segments = Vec::new();
    for (sequence, text) in [
        (
            0,
            "A longer row containing few selected words and extra context",
        ),
        (1, "A second longer row with additional words"),
    ] {
        let id = Uuid::new_v4();
        observer
            .store()
            .append_final_segment(&crate::storage::StoredTranscriptSegment {
                id,
                session_id: session,
                direction: "source_to_target".into(),
                sequence,
                occurred_at: 1,
                language: "en".into(),
                text: text.into(),
                is_final: true,
                details: None,
                occurred_at_ms: None,
            })
            .unwrap();
        segments.push(id);
    }
    for (query, ids) in [
        ("few selected words", vec![segments[0]]),
        ("extra context\nA second", segments.clone()),
    ] {
        let result = call(
            "search_knowledge",
            json!({
            "request":{
            "query":query,
            "selected_segment_ids":ids,
            "session_id":session,
            "limit":5}
            }
            ),
        )
        .unwrap();
        assert_eq!(result["hits"][0]["document_id"], document.to_string());
        assert_eq!(embeddings.0.lock().unwrap().last().unwrap(), &[query]);
    }
    let calls = embeddings.0.lock().unwrap().len();
    assert!(
        call(
            "search_knowledge",
            json!({
            "request":{
            "query":"few words",
            "selected_segment_ids":segments,
            "session_id":foreign,
            "limit":5}
            }
            )
        )
        .is_err()
    );
    assert_eq!(embeddings.0.lock().unwrap().len(), calls);
    let reindex = call(
        "reindex_document",
        json!({
        "documentId":document}
        ),
    )
    .unwrap();
    assert_eq!(reindex["kind"], "reindex");
    tauri::async_runtime::block_on(async {
        let id = Uuid::parse_str(reindex["id"].as_str().unwrap()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while observer.get_job(id).unwrap().unwrap().status != JobStatus::Succeeded {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    });
    let deletion = call(
        "delete_document",
        json!({
        "documentId":document}
        ),
    )
    .unwrap();
    assert_eq!(deletion["kind"], "delete_document");
    tauri::async_runtime::block_on(observer.close_all()).unwrap();
}
