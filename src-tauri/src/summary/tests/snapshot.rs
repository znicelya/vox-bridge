use crate::{
    intelligence::{jobs::OperationLease, test_support::IntelligenceHarness, types::*},
    storage::StoredTranscriptSegment,
    summary::{SummaryJobRequest, job_proof::SummaryJobProof, snapshot::FinalSnapshot},
};
use rusqlite::params;
use std::{fs::File, sync::Arc};
use uuid::Uuid;

pub(super) fn fixture() -> (IntelligenceHarness, Uuid, SummaryJobProof, OperationLease) {
    let h = IntelligenceHarness::new();
    let session = h.session();
    let job = h
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: session,
            profile: Some(crate::intelligence::test_support::test_llm_profile()),
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    let proof = h
        .store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='running' WHERE id=?1",
                [job.id.to_string()],
            )?;
            let raw: String = tx.query_row(
                "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                [job.id.to_string()],
                |r| r.get(0),
            )?;
            SummaryJobProof::claim_in_tx(tx, job.id, &serde_json::from_str(&raw).unwrap())
        })
        .unwrap();
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(h.temp_dir.path().join("snapshot-process.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    (h, session, proof, Arc::new(file))
}
fn append(
    h: &IntelligenceHarness,
    session: Uuid,
    direction: &str,
    sequence: i64,
    text: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    h.store
        .append_final_segment(&StoredTranscriptSegment {
            id,
            session_id: session,
            direction: direction.into(),
            sequence,
            occurred_at: -7,
            language: "legacy-language".into(),
            text: text.into(),
            is_final: true,
            details: None,
            occurred_at_ms: None,
        })
        .unwrap();
    id
}
async fn capture(
    h: &IntelligenceHarness,
    proof: &SummaryJobProof,
    owner: &OperationLease,
) -> FinalSnapshot {
    let (_cancel, rx) = tokio::sync::watch::channel(false);
    FinalSnapshot::capture(h.store.clone(), proof.clone(), owner.clone(), rx)
        .await
        .unwrap()
}
#[tokio::test]
async fn summary_snapshot_pins_successive_finals_and_preserves_cross_direction_insertion() {
    let (h, session, proof, owner) = fixture();
    let a = append(&h, session, "source_to_target", 9, "第一条");
    let other = h.session();
    append(&h, other, "source_to_target", 0, "不得进入");
    let b = append(&h, session, "target_to_source", 1, "第二条");
    let first = capture(&h, &proof, &owner).await;
    let late = append(&h, session, "unknown-legacy-direction", -2, "后来😀");
    let next = capture(&h, &proof, &owner).await;
    assert_eq!(
        first.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![a, b]
    );
    assert_eq!(
        next.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![a, b, late]
    );
    assert_eq!(first.input_final_count, 2);
    assert_eq!(next.input_final_count, 3);
    assert_ne!(first.input_hash, next.input_hash);
    assert!(first.rows[0].occurred_at_ms.is_none());
    assert!(first.rows[0].kind.is_none());
    assert_eq!(next.rows[2].direction, "unknown-legacy-direction");
    assert_eq!(next.rows[2].sequence, -2);
}

#[tokio::test]
async fn summary_snapshot_hash_binds_all_safe_identity_without_raw_metadata() {
    let (h, session, proof, owner) = fixture();
    let id = append(&h, session, "source_to_target", 0, "客户泵😀\n\"");
    let details = serde_json::json!({"kind":"source","provider":"fixture","provider_config_id":"00000000-0000-0000-0000-000000000002","stream_epoch":7,"sentence_id":"native-42","local_correlation_id":9,"provider_timestamp_ms":10,"start_ms":11,"end_ms":12,"api_key":"synthetic-key-sentinel","audio_path":"synthetic-audio-sentinel","log_id":"synthetic-raw-sentinel"});
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE translation_segments SET details_json=?2,occurred_at_ms=123 WHERE id=?1",
                params![id.to_string(), details.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let first = capture(&h, &proof, &owner).await;
    let literal = serde_json::json!([
        1,
        id.to_string(),
        "source_to_target",
        0,
        -7,
        123,
        "source",
        "legacy-language",
        "客户泵😀\n\"",
        [
            "fixture",
            "00000000-0000-0000-0000-000000000002",
            7,
            "native-42",
            9,
            10,
            11,
            12
        ]
    ]);
    // Explicit document ordering is independently hand-defined here.
    let expected = format!(
        "{{\"format\":\"voxbridge.summary-input.v1\",\"session_id\":\"{session}\",\"rows\":[{literal}]}}"
    );
    use sha2::Digest;
    assert_eq!(
        first.input_hash,
        format!("{:x}", sha2::Sha256::digest(expected.as_bytes()))
    );
    assert_eq!(first.normalized_bytes, expected.len());
    for (field, value) in [
        ("provider", serde_json::json!("changed")),
        (
            "provider_config_id",
            serde_json::json!("00000000-0000-0000-0000-000000000003"),
        ),
        ("stream_epoch", serde_json::json!(8)),
        ("sentence_id", serde_json::json!("native-43")),
        ("local_correlation_id", serde_json::json!(10)),
        ("provider_timestamp_ms", serde_json::json!(11)),
        ("start_ms", serde_json::json!(12)),
        ("end_ms", serde_json::json!(13)),
    ] {
        let mut changed = details.clone();
        changed[field] = value;
        h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE translation_segments SET details_json=?2 WHERE id=?1",
                    params![id.to_string(), changed.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        assert_ne!(
            capture(&h, &proof, &owner).await.input_hash,
            first.input_hash,
            "identity omitted {field}"
        );
    }
}

#[tokio::test]
async fn summary_snapshot_row100001_and_normalized_16mib_overflow_reject_whole_input() {
    let (h, session, proof, owner) = fixture();
    h.store.transaction(|tx|{tx.execute("WITH RECURSIVE n(x) AS(SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO translation_segments(id,session_id,direction,sequence,occurred_at,language,text,is_final) SELECT printf('%08x-0000-0000-0000-000000000000',x),?1,'',x,0,'','',1 FROM n",[session.to_string()])?;Ok(())}).unwrap();
    let full = capture(&h, &proof, &owner).await;
    assert_eq!(full.rows.len(), 100000);
    assert!(full.normalized_bytes <= 16 * 1024 * 1024);
    append(&h, session, "", 100001, "");
    let (_cancel, rx) = tokio::sync::watch::channel(false);
    assert!(matches!(
        FinalSnapshot::capture(h.store.clone(), proof, owner, rx).await,
        Err(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::ContextTooLarge
        ))
    ));
    let (h, session, proof, owner) = fixture();
    append(&h, session, "unknown", 0, &"\"".repeat(8 * 1024 * 1024));
    let (_cancel, rx) = tokio::sync::watch::channel(false);
    assert!(matches!(
        FinalSnapshot::capture(h.store.clone(), proof, owner, rx).await,
        Err(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::ContextTooLarge
        ))
    ));
}

#[tokio::test]
async fn summary_snapshot_separate_metadata_limit_and_present_invalid_native_types_fail() {
    let (h, session, proof, owner) = fixture();
    let id = append(&h, session, "", 0, "");
    for (raw, want) in [
        (
            format!("{{\"ignored\":\"{}\"}}", "x".repeat(16 * 1024 * 1024)),
            IntelligenceErrorCode::ResourceLimit,
        ),
        (r#"{"kind":12}"#.into(), IntelligenceErrorCode::Storage),
        (
            r#"{"stream_epoch":-1}"#.into(),
            IntelligenceErrorCode::Storage,
        ),
    ] {
        h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE translation_segments SET details_json=?2 WHERE id=?1",
                    params![id.to_string(), raw],
                )?;
                Ok(())
            })
            .unwrap();
        let (_cancel, rx) = tokio::sync::watch::channel(false);
        let result =
            FinalSnapshot::capture(h.store.clone(), proof.clone(), owner.clone(), rx).await;
        assert!(matches!(result,Err(crate::error::AppError::Intelligence(code)) if code==want));
    }
}
#[tokio::test]
async fn summary_snapshot_oversize_direction_and_language_are_whole_input_context_failures() {
    for column in ["direction", "language"] {
        let (h, session, proof, owner) = fixture();
        let id = append(&h, session, "", 0, "");
        h.store
            .transaction(|tx| {
                tx.execute(
                    &format!("UPDATE translation_segments SET {column}=?2 WHERE id=?1"),
                    params![id.to_string(), "x".repeat(16 * 1024 * 1024 + 1)],
                )?;
                Ok(())
            })
            .unwrap();
        let (_cancel, rx) = tokio::sync::watch::channel(false);
        let result = FinalSnapshot::capture(h.store.clone(), proof, owner, rx).await;
        assert!(
            matches!(
                result,
                Err(crate::error::AppError::Intelligence(
                    IntelligenceErrorCode::ContextTooLarge
                ))
            ),
            "oversize {column} must fail the complete input context"
        );
    }
}

#[test]
fn summary_snapshot_bundled_sqlite_uses_session_rowid_index_without_scan_or_sort() {
    let (h, session, _, _) = fixture();
    h.store.transaction(|tx|{
        for sql in ["EXPLAIN QUERY PLAN SELECT MAX(rowid) FROM translation_segments WHERE session_id=?1 AND is_final=1","EXPLAIN QUERY PLAN SELECT rowid,id,session_id,direction,sequence,occurred_at,language,text,is_final,details_json,occurred_at_ms FROM translation_segments WHERE session_id=?1 AND is_final=1 AND rowid<=999999 ORDER BY rowid LIMIT 100001"] {
            let plans=tx.prepare(sql)?.query_map([session.to_string()],|r|r.get::<_,String>(3))?.collect::<Result<Vec<_>,_>>()?;
            assert!(plans.iter().any(|p|p.contains("summary_snapshot_session")),"{plans:?}");
            assert!(plans.iter().all(|p|!p.contains("TEMP B-TREE")&&!p.starts_with("SCAN translation_segments")),"{plans:?}");
        }Ok(())
    }).unwrap();
}
