use crate::{
    export::{
        ExportFormat, export_session, json::render_json, markdown::render_markdown,
        plain_text::render_plain_text,
    },
    storage::{LocalStore, StoredSession, StoredTranscriptSegment, SummaryVersion},
};
use std::path::Path;
use uuid::Uuid;

fn fixture() -> (LocalStore, Uuid) {
    let store = LocalStore::in_memory().unwrap();
    let id = Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id,
            started_at: 1_799_000_000,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "fr".into(),
        })
        .unwrap();
    store
        .append_final_segment(&StoredTranscriptSegment {
            id: Uuid::new_v4(),
            session_id: id,
            direction: "source_to_target".into(),
            sequence: 1,
            occurred_at: 1_798_986_808,
            language: "en".into(),
            text: "Hello there".into(),
            is_final: true,
            details: None,
            occurred_at_ms: None,
        })
        .unwrap();
    store
        .append_final_segment(&StoredTranscriptSegment {
            id: Uuid::new_v4(),
            session_id: id,
            direction: "source_to_target".into(),
            sequence: 2,
            occurred_at: 1_798_986_808,
            language: "fr".into(),
            text: "Bonjour".into(),
            is_final: true,
            details: None,
            occurred_at_ms: None,
        })
        .unwrap();
    store
        .create_summary_version(&SummaryVersion {
            id: Uuid::new_v4(),
            session_id: id,
            version: 1,
            created_at: 1_799_000_010,
            status: "complete".into(),
            structured_json: r#"{"title":"Customer discovery","text":"A completed summary.","audio_path":"/private/session.wav","api_key":"secret-value","password":"password-value","passphrase":"passphrase-value","access_key":"access-value","secret_key":"secret-key-value","authorization":"authorization-value","bearer":"bearer-value","openai_api_key":"openai-key-value","API Key":"spaced-api-key","Access-Key":"mixed-access-key","client.key":"mixed-client-key","Pass Phrase":"spaced-passphrase"}"#
                .into(),
        })
        .unwrap();
    (store, id)
}

#[test]
fn markdown_export_contains_direction_timestamp_and_final_content() {
    let (store, id) = fixture();
    let output = render_markdown(&store.get_export_data(id).unwrap().unwrap());
    assert!(output.contains("Customer discovery"));
    assert!(output.contains("14:33:28"));
    assert!(output.contains("source_to_target"));
    assert!(output.contains("Hello there"));
    assert!(!output.contains("audio_path"));
    assert!(!output.contains("secret-value"));
    for value in [
        "password-value",
        "passphrase-value",
        "access-value",
        "secret-key-value",
        "authorization-value",
        "bearer-value",
        "openai-key-value",
        "spaced-api-key",
        "mixed-access-key",
        "mixed-client-key",
        "spaced-passphrase",
    ] {
        assert!(!output.contains(value), "leaked {value}");
    }
    assert!(!output.contains("interim"));
}

#[test]
fn json_export_has_stable_fields_and_only_final_segments() {
    let (store, id) = fixture();
    let output = render_json(&store.get_export_data(id).unwrap().unwrap()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["session"]["id"], id.to_string());
    assert_eq!(value["session"]["status"], "created");
    assert_eq!(value["session"]["ended_at"], serde_json::Value::Null);
    assert!(value["segments"].is_array());
    assert!(value["summaries"].is_array());
    assert_eq!(value["segments"][0]["text"], "Hello there");
    assert!(value.get("audio_path").is_none());
    assert!(value.get("secret").is_none());
    assert!(!output.contains("audio_path"));
    assert!(!output.contains("secret-value"));
    for value in [
        "password-value",
        "passphrase-value",
        "access-value",
        "secret-key-value",
        "authorization-value",
        "bearer-value",
        "openai-key-value",
        "spaced-api-key",
        "mixed-access-key",
        "mixed-client-key",
        "spaced-passphrase",
    ] {
        assert!(!output.contains(value), "leaked {value}");
    }
    assert!(output.find("\"session\"").unwrap() < output.find("\"segments\"").unwrap());
}

#[test]
fn plain_text_export_uses_timestamped_direction_source_and_translation_lines() {
    let (store, id) = fixture();
    let output = render_plain_text(&store.get_export_data(id).unwrap().unwrap());
    assert!(output.contains("[14:33:28] [source_to_target] source (en): Hello there"));
    assert!(output.contains("[14:33:28] [source_to_target] translation (fr): Bonjour"));
    assert!(!output.contains("audio_path"));
    assert!(!output.contains("secret-value"));
    for value in [
        "password-value",
        "passphrase-value",
        "access-value",
        "secret-key-value",
        "authorization-value",
        "bearer-value",
        "openai-key-value",
        "spaced-api-key",
        "mixed-access-key",
        "mixed-client-key",
        "spaced-passphrase",
    ] {
        assert!(!output.contains(value), "leaked {value}");
    }
}

#[test]
fn export_rejects_unknown_or_mismatched_extensions_and_directories() {
    let (store, id) = fixture();
    let dir = tempfile::tempdir().unwrap();
    assert!(export_session(&store, id, ExportFormat::Json, &dir.path().join("out.csv")).is_err());
    assert!(export_session(&store, id, ExportFormat::Json, &dir.path().join("out.md")).is_err());
    assert!(export_session(&store, id, ExportFormat::Json, dir.path()).is_err());
}

#[test]
fn export_writes_complete_output_to_destination() {
    let (store, id) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("session.json");
    assert_eq!(
        export_session(&store, id, ExportFormat::Json, &destination).unwrap(),
        destination
    );
    let output = std::fs::read_to_string(&destination).unwrap();
    assert!(output.contains("Hello there"));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn export_can_replace_an_existing_destination() {
    let (store, id) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("session.json");
    export_session(&store, id, ExportFormat::Json, &destination).unwrap();
    export_session(&store, id, ExportFormat::Json, &destination).unwrap();
    assert!(
        std::fs::read_to_string(destination)
            .unwrap()
            .contains("Hello there")
    );
}

#[test]
fn export_rejects_read_only_existing_destination() {
    let (store, id) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("session.json");
    std::fs::write(&destination, "old").unwrap();
    let mut permissions = std::fs::metadata(&destination).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&destination, permissions).unwrap();
    assert!(export_session(&store, id, ExportFormat::Json, &destination).is_err());
}

#[test]
fn export_rejects_missing_sessions() {
    let store = LocalStore::in_memory().unwrap();
    assert!(
        export_session(
            &store,
            Uuid::new_v4(),
            ExportFormat::Markdown,
            Path::new("session.md")
        )
        .is_err()
    );
}

#[test]
fn legacy_role_mapping_uses_each_rows_direction_and_marks_ambiguity() {
    let (store, id) = fixture();
    let mut data = store.get_export_data(id).unwrap().unwrap();
    data.segments[0].direction = "target_to_source".into();
    data.segments[1].direction = "target_to_source".into();
    assert_eq!(super::segment_role(&data, &data.segments[0]), "translation");
    assert_eq!(super::segment_role(&data, &data.segments[1]), "source");
    data.segments[0].direction = "bidirectional".into();
    assert_eq!(super::segment_role(&data, &data.segments[0]), "unknown");
    data.session.target_language = data.session.source_language.clone();
    assert_eq!(super::segment_role(&data, &data.segments[1]), "unknown");
}
#[test]
fn json_export_identifies_legacy_rows_and_includes_diagnostics_and_context() {
    use crate::domain::transcript::*;
    let (store, id) = fixture();
    store
        .append_diagnostic(&RuntimeDiagnostic {
            id: Uuid::new_v4(),
            session_id: id,
            direction: None,
            stream_epoch: None,
            occurred_at_ms: 10,
            payload: DiagnosticPayload::Latency {
                capture_to_send_ms: Some(80),
                response_to_enqueue_ms: None,
                enqueue_to_callback_ms: None,
                provider_processing_ms: None,
            },
        })
        .unwrap();
    let mut data = store.get_export_data(id).unwrap().unwrap();
    data.context = Some(serde_json::json!({"provider_name":"Saved profile"}));
    let value: serde_json::Value = serde_json::from_str(&render_json(&data).unwrap()).unwrap();
    assert_eq!(value["segments"][0]["origin"], "legacy");
    assert!(value["segments"][0]["details"].is_null());
    assert_eq!(
        value["diagnostics"][0]["payload"]["latency"]["capture_to_send_ms"],
        80
    );
    assert_eq!(value["context"]["provider_name"], "Saved profile");
}
