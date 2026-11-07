use super::super::codec::ChunkMetadata;
use crate::{
    intelligence::{test_support::test_llm_profile, types::ServiceKind},
    knowledge::{identity::EmbeddingIdentity, types::TextLocation},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn profile() -> crate::intelligence::types::ModelProfile {
    let mut profile = test_llm_profile();
    profile.id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    profile.kind = ServiceKind::Embedding;
    profile.base_url = "https://example.test/v1/".into();
    profile.model = Some("embedding-model".into());
    profile.max_context_tokens = None;
    profile.max_output_tokens = None;
    profile
}
fn location(heading: Option<String>) -> TextLocation {
    TextLocation {
        page: None,
        paragraph: 7,
        heading,
        start_char: 10,
        end_char: 31,
    }
}
#[test]
fn index_codec_identity_canonical_semantics_preserve_space_across_nonvector_edits() {
    let p = profile();
    let identity = EmbeddingIdentity::from_profile(&p, 128).unwrap();
    let canonical = r#"{"profile_id":"11111111-1111-4111-8111-111111111111","base_url":"https://example.test/v1","model":"embedding-model","dimensions":128,"normalization_version":1}"#;
    assert_eq!(serde_json::to_string(&identity).unwrap(), canonical);
    assert_eq!(
        identity.key().unwrap(),
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    );
    let mut edited = p.clone();
    edited.name = "Another label".into();
    edited.timeout_ms = 2000;
    edited.dimensions = Some(128);
    assert_eq!(
        EmbeddingIdentity::from_profile(&edited, 128)
            .unwrap()
            .key()
            .unwrap(),
        identity.key().unwrap()
    );
    edited.base_url = "https://another.example/v1".into();
    assert_ne!(
        EmbeddingIdentity::from_profile(&edited, 128)
            .unwrap()
            .key()
            .unwrap(),
        identity.key().unwrap()
    );
    edited = p.clone();
    edited.id = Uuid::new_v4();
    assert_ne!(
        EmbeddingIdentity::from_profile(&edited, 128)
            .unwrap()
            .key()
            .unwrap(),
        identity.key().unwrap()
    );
    assert_ne!(
        EmbeddingIdentity::from_profile(&p, 256)
            .unwrap()
            .key()
            .unwrap(),
        identity.key().unwrap()
    );
}
#[test]
fn index_codec_identity_rejects_invalid_endpoint_kind_and_dimension() {
    let mut p = profile();
    for d in [0, 127, 4097, u32::MAX] {
        assert!(EmbeddingIdentity::from_profile(&p, d).is_err());
    }
    p.dimensions = Some(256);
    assert!(EmbeddingIdentity::from_profile(&p, 128).is_err());
    p = profile();
    p.base_url = "https://user:secret@example.test/v1".into();
    assert!(EmbeddingIdentity::from_profile(&p, 128).is_err());
    p = profile();
    p.kind = ServiceKind::Llm;
    assert!(EmbeddingIdentity::from_profile(&p, 128).is_err());
}
#[test]
fn index_codec_full_six_thousand_cjk_heading_is_exact_and_reference_stays_bounded() {
    let loc = location(Some("题".repeat(6000)));
    let metadata = ChunkMetadata::new(
        Uuid::new_v4(),
        u32::MAX,
        Uuid::new_v4(),
        "a".repeat(64),
        &loc,
    )
    .unwrap();
    let encoded = metadata.encode().unwrap();
    assert!(encoded.len() < 16384);
    assert!(!encoded.contains("题"));
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["location"]["page"], serde_json::Value::Null);
    assert_eq!(value["location"]["paragraph"], 7);
    assert_eq!(value["location"]["start_char"], 10);
    assert_eq!(value["location"]["end_char"], 31);
    let canonical = format!(
        "{{\"page\":null,\"paragraph\":7,\"heading\":\"{}\",\"start_char\":10,\"end_char\":31}}",
        "题".repeat(6000)
    );
    assert_eq!(
        value["location"]["sha256"],
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    );
    assert!(
        ChunkMetadata::decode(&encoded)
            .unwrap()
            .matches_location(&loc)
            .unwrap()
    );
    let changed = location(Some("题".repeat(5999)));
    assert!(!metadata.matches_location(&changed).unwrap());
}
#[test]
fn index_codec_null_empty_heading_and_inline_page_are_distinct() {
    let none = location(None);
    let metadata =
        ChunkMetadata::new(Uuid::new_v4(), 1, Uuid::new_v4(), "b".repeat(64), &none).unwrap();
    assert!(
        !metadata
            .matches_location(&location(Some(String::new())))
            .unwrap()
    );
    let mut changed = none.clone();
    changed.page = Some(1);
    assert!(!metadata.matches_location(&changed).unwrap());
    assert!(metadata.matches_location(&none).unwrap());
}
#[test]
fn index_codec_rejects_unknown_schema_codec_fields_hashes_and_oversize() {
    let metadata = ChunkMetadata::new(
        Uuid::new_v4(),
        1,
        Uuid::new_v4(),
        "c".repeat(64),
        &location(None),
    )
    .unwrap();
    let valid: serde_json::Value = serde_json::from_str(&metadata.encode().unwrap()).unwrap();
    for (path, replacement) in [
        ("schema_version", serde_json::json!(1)),
        ("document_version", serde_json::json!(0)),
        ("content_hash", serde_json::json!("C".repeat(64))),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut v = valid.clone();
        v[path] = replacement;
        assert!(ChunkMetadata::decode(&v.to_string()).is_err());
    }
    for (path, replacement) in [
        ("codec", serde_json::json!("other")),
        ("kind", serde_json::json!("file")),
        ("sha256", serde_json::json!("no")),
        ("end_char", serde_json::json!(9)),
    ] {
        let mut v = valid.clone();
        v["location"][path] = replacement;
        assert!(ChunkMetadata::decode(&v.to_string()).is_err());
    }
    let mut v = valid.clone();
    v["location"].as_object_mut().unwrap().remove("page");
    assert!(ChunkMetadata::decode(&v.to_string()).is_err());
    assert!(ChunkMetadata::decode(&" ".repeat(16385)).is_err());
}
