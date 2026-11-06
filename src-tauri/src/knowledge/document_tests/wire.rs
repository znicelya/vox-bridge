use super::*;
use crate::intelligence::types::RecordOrigin;
use crate::knowledge::types::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PathArgs {
    path: std::path::PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplaceArgs {
    document_id: Uuid,
    path: std::path::PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GetArgs {
    document_id: Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageArgs {
    after: Option<i64>,
    limit: u32,
}
#[test]
fn knowledge_wire_contract_real_dtos_and_camelcase_args_match_shared_fixture() {
    let native = DocumentSummary {
        id: Uuid::from_u128(1),
        title: Some("Fixture document".into()),
        kind: DocumentKind::Text,
        version: 2,
        origin: RecordOrigin::Native,
        sha256: "a".repeat(64),
        status: DocumentStatus::Parsed,
        chunk_count: 1,
        created_at_ms: 1730000000123,
        updated_at_ms: Some(1730000000456),
        error: None,
    };
    let legacy = DocumentSummary {
        id: Uuid::from_u128(2),
        title: None,
        kind: DocumentKind::Unknown,
        version: 1,
        origin: RecordOrigin::Legacy,
        sha256: "legacy-hash".into(),
        status: DocumentStatus::RepairRequired,
        chunk_count: 1,
        created_at_ms: 37,
        updated_at_ms: None,
        error: None,
    };
    let location = TextLocation {
        page: Some(2),
        paragraph: 1,
        heading: Some("电气参数".into()),
        start_char: 0,
        end_char: 8,
    };
    let details = vec![
        DocumentDetail {
            summary: native.clone(),
            original_path: Some("/fixture/source.txt".into()),
            managed_path: Some("/fixture/knowledge/documents/source.txt".into()),
            legacy_uri: None,
            index_identity: None,
        },
        DocumentDetail {
            summary: legacy.clone(),
            original_path: None,
            managed_path: None,
            legacy_uri: Some("legacy-reference".into()),
            index_identity: None,
        },
    ];
    let kinds = [
        DocumentKind::Pdf,
        DocumentKind::Docx,
        DocumentKind::Text,
        DocumentKind::Markdown,
        DocumentKind::Unknown,
    ];
    let statuses = [
        DocumentStatus::Parsing,
        DocumentStatus::Parsed,
        DocumentStatus::Indexing,
        DocumentStatus::Ready,
        DocumentStatus::RepairRequired,
        DocumentStatus::Failed,
        DocumentStatus::Deleting,
    ];
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/contracts/knowledge-wire.json"
    ))
    .unwrap();
    assert_eq!(
        json!({"kinds":kinds,"statuses":statuses,"summaries":[native.clone(),legacy.clone()],"details":details,"page":DocumentPage{rows:vec![native,legacy],next_cursor:Some(10)},"location":location.clone(),"block":TextBlock{text:"额定电压24V。".into(),location},"commands":{"import_document":PathArgs{path:"/fixture/source.txt".into()},"replace_document":ReplaceArgs{document_id:Uuid::from_u128(1),path:"/fixture/source.txt".into()},"list_documents":PageArgs{after:None,limit:64},"get_document":GetArgs{document_id:Uuid::from_u128(2)}}}),
        fixture
    );
    assert!(
        serde_json::from_value::<ReplaceArgs>(
            json!({"document_id":Uuid::from_u128(1),"path":"/fixture/source.txt"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<GetArgs>(
            json!({"documentId":Uuid::from_u128(1),"credential":"never"})
        )
        .is_err()
    );
}
