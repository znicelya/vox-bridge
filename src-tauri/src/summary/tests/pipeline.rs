use super::harness::*;
use crate::intelligence::types::*;
use sha2::{Digest, Sha256};
use std::time::Duration;
fn large_summary(len: usize) -> String {
    serde_json::json!({"summary":"x".repeat(len),"participants":[],"companies":[],"requirements":[],"action_items":[],"risks":[],"terms":[]}).to_string()
}
#[tokio::test]
async fn summary_pipeline_all_maps_and_invalid_reduce_keep_raw_hash_and_complete_actual_coverage() {
    let h = SummaryHarness::new().await;
    let input = "x".repeat(1400);
    let session = h.completed_session(&[&input, &input, &input]).await;
    for _ in 0..3 {
        h.llm.push(VALID);
    }
    h.llm.push("{invalid-reduce");
    let job = h.generate(session, None).await.unwrap();
    let done = h.wait(job.id).await;
    let raw = h.raw_responses(job.object_id.unwrap()).unwrap();
    let coverage = h.coverage(job.object_id.unwrap()).unwrap();
    let versions = h.native_versions(session).unwrap();
    let phases = h
        .store
        .transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT ordinal,phase FROM summary_responses WHERE summary_id=?1 ORDER BY ordinal",
            )?;
            Ok(q.query_map([job.object_id.unwrap().to_string()], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap();
    h.close().await;
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error, Some(IntelligenceErrorCode::InvalidResponse));
    assert_eq!(raw, vec![VALID, VALID, VALID, "{invalid-reduce"]);
    assert_eq!(coverage.len(), 3);
    assert_eq!(
        phases,
        vec![
            (0, "map".into()),
            (1, "map".into()),
            (2, "map".into()),
            (3, "reduce".into())
        ]
    );
    assert!(versions[0].content.is_none());
    assert_eq!(
        versions[0].raw_response_hash,
        Some(format!("{:x}", Sha256::digest(b"{invalid-reduce")))
    );
}
#[tokio::test]
async fn summary_pipeline_success_reconstructs_every_unicode_part_and_retains_exact_map_tuples() {
    let h = SummaryHarness::new().await;
    let original = "客户😀e\u{301}\"\\\n".repeat(350);
    let session = h.completed_session(&[&original, "尾部"]).await;
    let job = h.generate(session, None).await.unwrap();
    let done = h.wait(job.id).await;
    let coverage = h.coverage(job.object_id.unwrap()).unwrap();
    let (actual, text) = {
        let calls = h.llm.calls.lock().unwrap();
        let mut supplied = Vec::new();
        let mut text = String::new();
        for (_, call) in calls.iter() {
            assert!(
                serde_json::to_string(&call.system).unwrap().len()
                    + serde_json::to_string(&call.user).unwrap().len()
                    + 1024
                    <= 3584
            );
            let value: serde_json::Value = serde_json::from_str(&call.user).unwrap();
            if value["phase"] == "map" {
                for input in value["inputs"].as_array().unwrap() {
                    supplied.push((
                        input["segment_id"].as_str().unwrap().to_owned(),
                        input["part_index"].as_u64().unwrap() as u32,
                        input["part_count"].as_u64().unwrap() as u32,
                    ));
                    if input["text"] != "尾部" {
                        text.push_str(input["text"].as_str().unwrap());
                    }
                }
            }
        }
        supplied.sort();
        (supplied, text)
    };
    h.close().await;
    assert_eq!(done.status, JobStatus::Succeeded);
    assert_eq!(actual, coverage);
    assert_eq!(text, original);
    assert!(actual.len() > 2);
}
#[tokio::test]
async fn summary_pipeline_middle_part_omission_cannot_succeed_with_equal_distinct_segment_count() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&[&"x".repeat(5000)]).await;
    h.store.transaction(|tx| {tx.execute_batch("CREATE TRIGGER omit_middle AFTER INSERT ON summary_coverage WHEN NEW.part_index=2 BEGIN DELETE FROM summary_coverage WHERE summary_id=NEW.summary_id AND segment_id=NEW.segment_id AND part_index=1; END;")?;Ok(())}).unwrap();
    let job = h.generate(session, None).await.unwrap();
    let done = h.wait(job.id).await;
    let coverage = h.coverage(job.object_id.unwrap()).unwrap();
    let versions = h.native_versions(session).unwrap();
    h.close().await;
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error, Some(IntelligenceErrorCode::Storage));
    assert!(versions[0].content.is_none());
    assert_eq!(versions[0].input_final_count, Some(1));
    assert!(coverage.len() >= 2);
    assert!(coverage.iter().all(|(_, part, _)| *part != 1));
    assert!(
        coverage
            .iter()
            .all(|(id, _, count)| id == &coverage[0].0 && *count >= 3)
    );
}
#[tokio::test]
async fn summary_pipeline_growing_children_fail_before_no_pair_dispatch_without_trimming() {
    let h = SummaryHarness::new().await;
    let input = "x".repeat(1400);
    let session = h.completed_session(&[&input, &input]).await;
    let child = large_summary(1500);
    h.llm.push(&child);
    h.llm.push(&child);
    let job = h.generate(session, None).await.unwrap();
    let done = h.wait(job.id).await;
    let raw = h.raw_responses(job.object_id.unwrap()).unwrap();
    let versions = h.native_versions(session).unwrap();
    h.close().await;
    assert_eq!(done.error, Some(IntelligenceErrorCode::ContextTooLarge));
    assert_eq!(raw, vec![child.clone(), child]);
    assert_eq!(h.llm.calls.lock().unwrap().len(), 2);
    assert!(versions[0].content.is_none());
}
#[tokio::test]
async fn summary_pipeline_64_maps_and_whole_snapshot_overflow_make_zero_calls() {
    for overflow in [false, true] {
        let h = SummaryHarness::new().await;
        let texts = if overflow {
            vec!["x".repeat(16 * 1024 * 1024)]
        } else {
            vec!["x".repeat(1400); 64]
        };
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let session = h.completed_session(&refs).await;
        let job = h.generate(session, None).await.unwrap();
        let done = h.wait(job.id).await;
        let versions = h.native_versions(session).unwrap();
        h.close().await;
        assert_eq!(done.error, Some(IntelligenceErrorCode::ContextTooLarge));
        assert!(h.llm.calls.lock().unwrap().is_empty());
        assert!(versions[0].content.is_none());
        if overflow {
            assert!(versions[0].input_hash.is_none());
            assert!(versions[0].input_final_count.is_none());
        } else {
            assert_eq!(versions[0].input_final_count, Some(64));
        }
    }
}
#[tokio::test]
async fn summary_pipeline_single_raw_and_total_raw_breaches_keep_prior_committed_rows_only() {
    for aggregate in [false, true] {
        let h = SummaryHarness::new().await;
        let text = "x".repeat(1400);
        let refs = vec![text.as_str(); if aggregate { 20 } else { 2 }];
        let session = h.completed_session(&refs).await;
        if aggregate {
            let raw = format!("{VALID}{}", " ".repeat(2 * 1024 * 1024 - VALID.len()));
            for _ in 0..20 {
                h.llm.push(&raw);
            }
        } else {
            h.llm.push(VALID);
            h.llm.push(&"x".repeat(2 * 1024 * 1024 + 1));
        }
        let job = h.generate(session, None).await.unwrap();
        // This case persists up to 32 MiB in a debug build. Its fixture wait
        // must allow shared test load; it is not a product request deadline.
        let done = h.wait_with_timeout(job.id, Duration::from_secs(30)).await;
        let raw = h.raw_responses(job.object_id.unwrap()).unwrap();
        let coverage = h.coverage(job.object_id.unwrap()).unwrap();
        let bytes = h
            .store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT raw_response_bytes FROM session_summaries WHERE id=?1",
                    [job.object_id.unwrap().to_string()],
                    |r| r.get::<_, u64>(0),
                )?)
            })
            .unwrap();
        let versions = h.native_versions(session).unwrap();
        h.close().await;
        assert_eq!(done.error, Some(IntelligenceErrorCode::ResponseTooLarge));
        assert!(versions[0].content.is_none());
        assert_eq!(raw.len(), if aggregate { 16 } else { 1 });
        assert_eq!(coverage.len(), raw.len());
        assert_eq!(h.llm.calls.lock().unwrap().len(), raw.len() + 1);
        assert_eq!(bytes, raw.iter().map(|r| r.len() as u64).sum::<u64>());
    }
}
#[tokio::test]
async fn summary_pipeline_storage_failures_preserve_prior_response_and_prevent_root() {
    for root in [false, true] {
        let h = SummaryHarness::new().await;
        let text = "x".repeat(1400);
        let session = h
            .completed_session(&if root {
                vec![text.as_str()]
            } else {
                vec![text.as_str(), text.as_str()]
            })
            .await;
        h.store.transaction(|tx| {tx.execute_batch(if root {"CREATE TRIGGER fail_root BEFORE UPDATE ON session_summaries WHEN NEW.structured_json<>'null' BEGIN SELECT RAISE(ABORT,'injected root failure'); END;"}else {"CREATE TRIGGER fail_second BEFORE INSERT ON summary_responses WHEN NEW.ordinal=1 BEGIN SELECT RAISE(ABORT,'injected retention failure'); END;"})?;Ok(())}).unwrap();
        let job = h.generate(session, None).await.unwrap();
        let done = h.wait(job.id).await;
        let raw = h.raw_responses(job.object_id.unwrap()).unwrap();
        let coverage = h.coverage(job.object_id.unwrap()).unwrap();
        let versions = h.native_versions(session).unwrap();
        h.close().await;
        assert_eq!(done.status, JobStatus::Failed);
        assert_eq!(done.error, Some(IntelligenceErrorCode::Storage));
        assert_eq!(raw, vec![VALID]);
        assert_eq!(coverage.len(), 1);
        assert!(versions[0].content.is_none());
    }
}
