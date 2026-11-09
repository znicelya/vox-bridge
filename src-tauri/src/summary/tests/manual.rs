use super::harness::*;
use crate::{app::CredentialChange, intelligence::types::*};
use std::sync::atomic::Ordering;
use uuid::Uuid;

#[tokio::test]
async fn manual_entered_llm_retains_raw_and_exact_supplied_map_coverage_before_parse() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&[]).await;
    let a = h.insert_final(session, "source_to_target", 0, "客户询问24V泵");
    let b = h.insert_final(session, "target_to_source", 0, "答复\"可用\"\n😀");
    h.llm.push("{invalid");
    let job = h.generate(session, None).await.unwrap();
    h.llm_entered().await;
    let done = h.wait(job.id).await;
    let (call_count, request) = {
        let calls = h.llm.calls.lock().unwrap();
        (
            calls.len(),
            serde_json::from_str::<serde_json::Value>(&calls[0].1.user).unwrap(),
        )
    };
    let raw = h.raw_responses(job.object_id.unwrap()).unwrap();
    let coverage = h.coverage(job.object_id.unwrap()).unwrap();
    h.close().await;
    assert_eq!(call_count, 1, "actual recording LLM must have entered once");
    let inputs = request["inputs"].as_array().unwrap();
    assert_eq!(
        inputs.len(),
        2,
        "the actual request must supply every final row"
    );
    assert_eq!(inputs[0]["segment_id"], a.to_string());
    assert_eq!(inputs[1]["segment_id"], b.to_string());
    assert_eq!(inputs[1]["text"], "答复\"可用\"\n😀");
    assert_eq!(
        raw,
        vec!["{invalid"],
        "entered invalid response must be durable before strict parsing"
    );
    let mut expected = vec![(a.to_string(), 0, 1), (b.to_string(), 0, 1)];
    expected.sort();
    assert_eq!(coverage, expected);
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error, Some(IntelligenceErrorCode::InvalidResponse));
}

#[tokio::test]
async fn manual_retry_preserves_prior_output_and_monotonic_versions() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["客户询问24V泵"]).await;
    h.llm.push(VALID);
    let first = h.generate(session, None).await.unwrap();
    let first_done = h.wait(first.id).await;
    h.llm.push("{invalid");
    let second = h.generate(session, None).await.unwrap();
    let second_done = h.wait(second.id).await;
    let versions = h.native_versions(session).unwrap();
    let raw = h.raw_responses(second.object_id.unwrap()).unwrap();
    h.close().await;
    assert_eq!(first_done.status, JobStatus::Succeeded);
    assert_eq!(second_done.status, JobStatus::Failed);
    assert_eq!(
        second_done.error,
        Some(IntelligenceErrorCode::InvalidResponse)
    );
    assert_ne!(first.id, second.id);
    assert_eq!(versions[0].version, 1);
    assert_eq!(versions[1].version, 2);
    assert_eq!(versions[0].content.as_ref().unwrap().summary, "24V需求");
    assert!(versions[1].content.is_none());
    assert_eq!(raw, vec!["{invalid"]);
}

#[tokio::test]
async fn manual_concurrent_generation_admits_one_native_version() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["客户询问24V泵"]).await;
    h.llm.block.store(true, Ordering::SeqCst);
    let (a, b) = tokio::join!(h.generate(session, None), h.generate(session, None));
    let admitted = match (&a, &b) {
        (Ok(job), Err(_)) | (Err(_), Ok(job)) => Some(job.clone()),
        _ => None,
    };
    if let Some(job) = &admitted {
        h.llm_entered().await;
        h.llm.release.notify_one();
        h.wait(job.id).await;
    }
    let count = h.native_versions(session).unwrap().len();
    h.close().await;
    assert!(
        admitted.is_some(),
        "same-session unfinished manual work must be excluded"
    );
    assert_eq!(count, 1);
}

#[tokio::test]
async fn manual_retry_after_cancelled_and_interrupted_is_new_version() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["客户询问24V泵"]).await;
    h.llm.block.store(true, Ordering::SeqCst);
    let first = h.generate(session, None).await.unwrap();
    h.llm_entered().await;
    h.jobs.cancel(first.id).await.unwrap();
    h.llm.release.notify_one();
    let cancelled = h.wait(first.id).await;
    h.llm.block.store(false, Ordering::SeqCst);
    let next = h.generate(session, None).await.unwrap();
    let success = h.wait(next.id).await;
    h.jobs.shutdown().await.unwrap();
    // Genuine inert/fully joined scheduler recovery, followed by a new generation.
    let pending = h.create_stored(session, false).unwrap();
    h.jobs.reconcile().unwrap();
    let interrupted = h.store.get_job(pending.id).unwrap().unwrap();
    h.service
        .reconcile_local_owned(h.process.clone(), 1, h.jobs.reap_port())
        .await
        .unwrap();
    h.jobs.start_owned(h.process.clone()).unwrap();
    let retry = h.generate(session, None).await.unwrap();
    let retried = h.wait(retry.id).await;
    let versions = h.native_versions(session).unwrap();
    h.close().await;
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    assert_eq!(success.status, JobStatus::Succeeded);
    assert_eq!(interrupted.status, JobStatus::Interrupted);
    assert_eq!(retried.status, JobStatus::Succeeded);
    assert_eq!(
        versions.iter().map(|r| r.version).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(versions[0].status, JobStatus::Cancelled);
    assert_eq!(versions[2].status, JobStatus::Interrupted);
}

#[tokio::test]
async fn manual_explicit_model_switch_and_missing_selection_do_not_rewrite_history() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["完整文本"]).await;
    let first = h.generate(session, None).await.unwrap();
    let done = h.wait(first.id).await;
    let mut second = h.profile.clone();
    second.id = Uuid::new_v4();
    second.model = Some("second-model".into());
    h.config
        .save(SaveModelProfile {
            profile: second.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-second".into(),
            },
        })
        .await
        .unwrap();
    let changed = h.generate(session, Some(second.id)).await.unwrap();
    let changed_done = h.wait(changed.id).await;
    h.config
        .save_settings(IntelligenceSettings {
            automatic_summary: false,
            ..Default::default()
        })
        .await
        .unwrap();
    let absent = h.generate(session, None).await.unwrap();
    let absent_done = h.wait(absent.id).await;
    let versions = h.native_versions(session).unwrap();
    h.close().await;
    assert_eq!(done.status, JobStatus::Succeeded);
    assert_eq!(changed_done.status, JobStatus::Succeeded);
    assert_eq!(
        absent_done.error,
        Some(IntelligenceErrorCode::Configuration)
    );
    assert_eq!(versions[0].model.as_deref(), Some("fixture-model"));
    assert_eq!(versions[1].model.as_deref(), Some("second-model"));
    assert_eq!(h.llm.calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn manual_no_finals_fails_locally_and_later_final_uses_a_new_snapshot() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&[]).await;
    let first = h.generate(session, None).await.unwrap();
    let empty = h.wait(first.id).await;
    h.insert_final(session, "target_to_source", 0, "后来完整文本");
    let second = h.generate(session, None).await.unwrap();
    let done = h.wait(second.id).await;
    let versions = h.native_versions(session).unwrap();
    h.close().await;
    assert_eq!(empty.error, Some(IntelligenceErrorCode::NoFinalText));
    assert_eq!(done.status, JobStatus::Succeeded);
    assert_eq!(versions[1].input_final_count, Some(1));
    assert_eq!(h.llm.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn manual_delete_waits_for_actual_entered_llm_and_late_result_never_repopulates() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["客户询问24V泵"]).await;
    h.llm.block.store(true, Ordering::SeqCst);
    let job = h.generate(session, None).await.unwrap();
    h.llm_entered().await;
    let id = job.object_id.unwrap();
    let deletion = h.delete(id);
    tokio::pin!(deletion);
    let observed = tokio::time::timeout(std::time::Duration::from_millis(50), &mut deletion).await;
    let waited = observed.is_err();
    let fenced = h.native_versions(session).unwrap();
    h.llm.release.notify_one();
    let result = match observed {
        Err(_) => deletion.await,
        Ok(result) => result,
    };
    let remaining = h.native_versions(session).unwrap();
    let raw = h.raw_responses(id).unwrap();
    h.close().await;
    assert!(
        waited,
        "terminal status cannot substitute for actual work completion"
    );
    assert!(
        result.is_ok(),
        "owned deletion must complete after actual work release: {result:?}"
    );
    assert_eq!(fenced.len(), 1);
    assert!(fenced[0].deleting);
    assert!(remaining.is_empty());
    assert!(raw.is_empty());
    assert!(h.store.get_job(job.id).unwrap().is_some());
}
