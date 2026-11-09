use crate::summary::content::SummaryContent;
use serde_json::{Value, json};
fn valid() -> Value {
    json!({"summary":"完整","participants":[],"companies":[],"requirements":[],"action_items":[],"risks":[],"terms":[]})
}
fn decode(value: &Value) -> Result<SummaryContent, serde_json::Error> {
    serde_json::from_str(&value.to_string())
}

#[test]
fn summary_content_rejects_scalar_and_array_overflow_without_truncation() {
    let mut content = valid();
    content["summary"] = "中".repeat(20_000).into();
    assert!(decode(&content).is_ok());
    content["summary"] = "中".repeat(20_001).into();
    assert!(
        decode(&content).is_err(),
        "summary cannot publish a 20001-scalar response"
    );
    for field in ["participants", "companies", "requirements", "risks"] {
        let mut content = valid();
        content[field] = json!(["😀".repeat(2_001)]);
        assert!(
            decode(&content).is_err(),
            "{field} strings must stay within 2000 scalars"
        );
        content[field] = json!(vec!["x"; 101]);
        assert!(
            decode(&content).is_err(),
            "{field} must reject the 101st item"
        );
    }
}

#[test]
fn summary_content_requires_exact_schema_types_and_rejects_unknown_nested_fields() {
    for field in [
        "summary",
        "participants",
        "companies",
        "requirements",
        "action_items",
        "risks",
        "terms",
    ] {
        let mut content = valid();
        content.as_object_mut().unwrap().remove(field);
        assert!(decode(&content).is_err());
    }
    for invalid in [
        json!({"summary":1}),
        json!({"participants":null}),
        json!({"action_items":[{"text":"x","owner":2,"due":null}]}),
        json!({"terms":[{"term":"x","meaning":"y","extra":"raw"}]}),
        json!({"extra":"raw"}),
    ] {
        let mut content = valid();
        content
            .as_object_mut()
            .unwrap()
            .extend(invalid.as_object().unwrap().clone());
        assert!(decode(&content).is_err());
    }
    let mut content = valid();
    content["action_items"] = json!([{"text":"x","owner":null,"due":null}]);
    assert!(decode(&content).is_ok());
    assert!(serde_json::from_str::<SummaryContent>("```json\n{}\n```").is_err());
    let duplicate = r#"{"summary":"x","summary":"y","participants":[],"companies":[],"requirements":[],"action_items":[],"risks":[],"terms":[]}"#;
    assert!(serde_json::from_str::<SummaryContent>(duplicate).is_err());
}

#[test]
fn summary_content_bounds_every_action_and_term_and_keeps_all_fields() {
    for field in ["text", "owner", "due"] {
        let mut action = json!({"text":"做","owner":null,"due":null});
        action[field] = "😀".repeat(2_001).into();
        let mut content = valid();
        content["action_items"] = json!([action]);
        assert!(decode(&content).is_err(), "unbounded action {field}");
    }
    for field in ["term", "meaning"] {
        let mut term = json!({"term":"泵","meaning":"输送"});
        term[field] = "中".repeat(2_001).into();
        let mut content = valid();
        content["terms"] = json!([term]);
        assert!(decode(&content).is_err(), "unbounded term {field}");
    }
    for (field, row) in [
        ("action_items", json!({"text":"做","owner":null,"due":null})),
        ("terms", json!({"term":"泵","meaning":"输送"})),
    ] {
        let mut content = valid();
        content[field] = Value::Array(vec![row; 101]);
        assert!(decode(&content).is_err());
    }
    let full = json!({"summary":"结论","participants":["张"],"companies":["公司"],"requirements":["24V"],"action_items":[{"text":"寄样品","owner":"李","due":"周五"}],"risks":["缺货"],"terms":[{"term":"流量","meaning":"每分钟升数"}]});
    assert_eq!(serde_json::to_value(decode(&full).unwrap()).unwrap(), full);
}

#[test]
fn summary_legacy_sanitizer_bounds_depth_nodes_and_decoded_bytes_and_rejects_raw() {
    for invalid in [
        r#"{"text":"keep","raw_response":"must-not-project"}"#.to_owned(),
        format!("{}0{}", "[".repeat(65), "]".repeat(65)),
        format!("[{}]", vec!["0"; 100_000].join(",")),
        serde_json::to_string(&"x".repeat(2 * 1024 * 1024 + 1)).unwrap(),
    ] {
        assert!(
            crate::export::safe_summary_json(&invalid).is_none(),
            "legacy boundary accepted an incomplete/unsafe projection"
        );
    }
    for raw in [
        r#"{"title":"旧标题","text":"旧正文"}"#,
        r#"{"text":"正文","api_key":"synthetic-secret","audio_path":"synthetic-audio"}"#,
    ] {
        let value = crate::export::safe_summary_json(raw).unwrap();
        assert!(value.get("api_key").is_none());
        assert!(value.get("audio_path").is_none());
        assert!(value["text"].is_string());
    }
}
