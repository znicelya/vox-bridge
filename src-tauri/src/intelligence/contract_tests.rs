use super::types::*;
use crate::{commands::command_error, error::AppError};
#[test]
fn stable_intelligence_error_envelopes_match_wire_subcodes() {
    use IntelligenceErrorCode::*;
    let cases = [
        (
            Configuration,
            "configuration",
            "Service configuration is invalid or incomplete.",
        ),
        (
            Busy,
            "busy",
            "Intelligence work is busy or the profile is in use.",
        ),
        (Cancelled, "cancelled", "Intelligence work was cancelled."),
        (
            Interrupted,
            "interrupted",
            "Intelligence work was interrupted.",
        ),
        (Timeout, "timeout", "Service request timed out."),
        (Network, "network", "Service connection failed."),
        (
            Authentication,
            "authentication",
            "Service authentication failed.",
        ),
        (RateLimited, "rate_limited", "Service rate limit reached."),
        (
            ResponseTooLarge,
            "response_too_large",
            "Service response exceeded the size limit.",
        ),
        (
            InvalidResponse,
            "invalid_response",
            "Service response was invalid.",
        ),
        (
            ContextTooLarge,
            "context_too_large",
            "Input exceeded the model context limit.",
        ),
        (
            UnsupportedDocument,
            "unsupported_document",
            "Document format is unsupported.",
        ),
        (ParseFailed, "parse_failed", "Document could not be parsed."),
        (
            ResourceLimit,
            "resource_limit",
            "Intelligence resource limit reached.",
        ),
        (
            IndexRepairRequired,
            "index_repair_required",
            "Document index requires repair.",
        ),
        (
            NoFinalText,
            "no_final_text",
            "No final transcript text is available.",
        ),
        (
            Storage,
            "storage",
            "Intelligence data could not be stored or read.",
        ),
    ];
    for (code, name, message) in cases {
        assert_eq!(serde_json::to_value(code).unwrap(), name);
        assert_eq!(code.as_str(), name);
        assert_eq!(AppError::Intelligence(code).to_string(), message);
        assert_eq!(
            command_error(AppError::Intelligence(code)),
            format!("[INTELLIGENCE_ERROR][{name}] {message}")
        );
    }
    assert_eq!(
        command_error(AppError::Configuration("fixture".into())),
        "[CONFIGURATION_ERROR] configuration error: fixture"
    );
    assert_eq!(
        command_error(AppError::InvalidRequest("fixture".into())),
        "[INVALID_REQUEST] invalid request: fixture"
    );
}
#[test]
fn job_and_probe_wire_values_use_tagged_events_epoch_milliseconds_and_safe_metadata() {
    let id = uuid::Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
    let job = JobView {
        id,
        kind: JobKind::ResearchOverview,
        status: JobStatus::Failed,
        session_id: None,
        object_id: Some(id),
        completed_units: 2,
        total_units: Some(3),
        error: Some(IntelligenceErrorCode::RateLimited),
        created_at_ms: 1_799_000_001_001,
        updated_at_ms: 1_799_000_001_123,
    };
    let expected = serde_json::json!({"kind":"job_changed","job":{"id":id,"kind":"research_overview","status":"failed","session_id":null,"object_id":id,"completed_units":2,"total_units":3,"error":"rate_limited","created_at_ms":1799000001001_i64,"updated_at_ms":1799000001123_i64}});
    let event = IntelligenceEvent::JobChanged { job: job.clone() };
    assert_eq!(serde_json::to_value(&event).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<IntelligenceEvent>(expected).unwrap(),
        event
    );
    assert_eq!(
        serde_json::to_value(IntelligenceEvent::ResyncNeeded { missed_events: 4 }).unwrap(),
        serde_json::json!({"kind":"resync_needed","missed_events":4})
    );
    let probe = ConnectionProbe {
        checked_at_ms: 1799000001123,
        ok: false,
        latency_ms: None,
        error: Some(IntelligenceErrorCode::Timeout),
    };
    assert_eq!(
        serde_json::to_value(&probe).unwrap(),
        serde_json::json!({"checked_at_ms":1799000001123_i64,"ok":false,"latency_ms":null,"error":"timeout"})
    );
    let page = JobPage {
        rows: vec![job],
        next_cursor: Some(9),
    };
    assert_eq!(
        serde_json::from_value::<JobPage>(serde_json::to_value(&page).unwrap()).unwrap(),
        page
    );
    assert_eq!(
        serde_json::to_value(RecordOrigin::Native).unwrap(),
        "native"
    );
    assert_eq!(
        serde_json::to_value(RecordOrigin::Legacy).unwrap(),
        "legacy"
    );
    assert!(
        serde_json::from_value::<IntelligenceSettings>(serde_json::json!({}))
            .unwrap()
            .automatic_summary
    );
}
#[test]
fn credential_input_cannot_supply_a_private_reference() {
    let mut value = serde_json::to_value(super::test_support::test_llm_profile()).unwrap();
    value["credential_reference"] = serde_json::json!("foreign");
    assert!(
        serde_json::from_value::<SaveModelProfile>(
            serde_json::json!({"profile":value,"credential":{"action":"keep"}})
        )
        .is_err()
    );
}
