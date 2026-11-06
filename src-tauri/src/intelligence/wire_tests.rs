use super::types::*;
use serde_json::json;
use uuid::Uuid;
#[test]
fn intelligence_wire_contract_real_dtos_match_shared_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/intelligence-wire.json"
    ))
    .unwrap();
    let profiles = [
        ServiceKind::Llm,
        ServiceKind::Embedding,
        ServiceKind::Tavily,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, kind)| ModelProfileView {
        profile: ModelProfile {
            id: Uuid::from_u128(i as u128 + 1),
            kind,
            name: format!("Fixture {}", ["llm", "embedding", "tavily"][i]),
            base_url: "http://127.0.0.1:12345/v1".into(),
            model: (kind != ServiceKind::Tavily).then(|| "fixture-model".into()),
            timeout_ms: 1000,
            max_context_tokens: (kind == ServiceKind::Llm).then_some(4096),
            max_output_tokens: (kind == ServiceKind::Llm).then_some(512),
            dimensions: (kind == ServiceKind::Embedding).then_some(128),
        },
        credential_configured: true,
        last_probe: None,
    })
    .collect::<Vec<_>>();
    let settings = IntelligenceSettings {
        summary_profile_id: Some(Uuid::from_u128(1)),
        embedding_profile_id: Some(Uuid::from_u128(2)),
        tavily_profile_id: Some(Uuid::from_u128(3)),
        automatic_summary: true,
    };
    let pairs = [
        (JobKind::Import, JobStatus::Queued, None),
        (JobKind::Reindex, JobStatus::Running, None),
        (JobKind::DeleteDocument, JobStatus::Succeeded, None),
        (
            JobKind::Summary,
            JobStatus::Failed,
            Some(IntelligenceErrorCode::InvalidResponse),
        ),
        (
            JobKind::Research,
            JobStatus::Cancelled,
            Some(IntelligenceErrorCode::Cancelled),
        ),
        (
            JobKind::ResearchOverview,
            JobStatus::Interrupted,
            Some(IntelligenceErrorCode::Interrupted),
        ),
        (JobKind::Summary, JobStatus::Succeeded, None),
    ];
    let rows = pairs
        .into_iter()
        .enumerate()
        .map(|(i, (kind, status, error))| JobView {
            id: Uuid::from_u128(i as u128 + 10),
            kind,
            status,
            session_id: (kind == JobKind::Summary).then(|| Uuid::from_u128(30)),
            object_id: Some(Uuid::from_u128(i as u128 + 50)),
            completed_units: 0,
            total_units: None,
            error,
            created_at_ms: 1_730_000_000_123,
            updated_at_ms: 1_730_000_000_456,
        })
        .collect::<Vec<_>>();
    let probes = vec![
        ConnectionProbe {
            checked_at_ms: 1_730_000_000_123,
            ok: true,
            latency_ms: Some(12),
            error: None,
        },
        ConnectionProbe {
            checked_at_ms: 1_730_000_000_456,
            ok: false,
            latency_ms: None,
            error: Some(IntelligenceErrorCode::Timeout),
        },
    ];
    let events = vec![
        IntelligenceEvent::JobChanged {
            job: rows[2].clone(),
        },
        IntelligenceEvent::ResyncNeeded { missed_events: 17 },
    ];
    use IntelligenceErrorCode::*;
    let errors = [
        Configuration,
        Busy,
        Cancelled,
        Interrupted,
        Timeout,
        Network,
        Authentication,
        RateLimited,
        ResponseTooLarge,
        InvalidResponse,
        ContextTooLarge,
        UnsupportedDocument,
        ParseFailed,
        ResourceLimit,
        IndexRepairRequired,
        NoFinalText,
        Storage,
    ];
    assert_eq!(
        json!({"profiles":profiles,"settings":settings,"managedDataLocation":ManagedDataLocation{data_dir:"/fixture/empty-library".into()},"page":JobPage{rows,next_cursor:None},"probes":probes,"events":events,"errorCodes":errors}),
        fixture
    );
}
