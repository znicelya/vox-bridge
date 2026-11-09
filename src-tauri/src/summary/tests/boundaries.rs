use super::{harness::VALID, snapshot::fixture};
use crate::{
    error::{AppError, AppResult},
    intelligence::{test_support::IntelligenceHarness, types::*},
    llm::LlmResponse,
    storage::{StoredTranscriptSegment, SummaryVersion},
    summary::{
        SummaryJobRequest,
        content::SummaryContent,
        planner::{self, SummaryNode},
        snapshot::FinalSnapshot,
        store,
    },
};
use rusqlite::{Transaction, params, types::Value};
use uuid::Uuid;

const FIELDS: &[&str] = &[
    "kind",
    "object",
    "session",
    "exclusion",
    "permanent",
    "input_summary",
    "input_session",
    "input_extra",
    "payload_bytes",
    "profile_id",
    "profile_kind",
    "profile_name",
    "profile_url",
    "profile_model",
    "profile_timeout",
    "profile_context",
    "profile_output",
    "profile_dimensions",
    "profile_count",
    "profile_absent",
    "native_profile",
    "native_model",
    "native_session",
    "native_version",
    "native_job",
    "native_ms",
    "witness_missing",
    "witness_bad",
    "witness_upper",
];
fn mutate(
    tx: &Transaction<'_>,
    job: Uuid,
    native: Uuid,
    payload: &serde_json::Value,
    field: &str,
    foreign: Uuid,
    foreign_session: Uuid,
) -> AppResult<()> {
    let mut changed = payload.clone();
    let mut payload_changed = false;
    match field {
        "kind" => {
            tx.execute(
                "UPDATE intelligence_jobs SET kind='research' WHERE id=?1",
                [job.to_string()],
            )?;
        }
        "object" => {
            tx.execute(
                "UPDATE intelligence_jobs SET object_id=?2 WHERE id=?1",
                params![job.to_string(), foreign.to_string()],
            )?;
        }
        "session" => {
            tx.execute(
                "UPDATE intelligence_jobs SET session_id=?2 WHERE id=?1",
                params![job.to_string(), foreign_session.to_string()],
            )?;
        }
        "exclusion" => {
            tx.execute(
                "UPDATE intelligence_jobs SET exclusion_key=?2 WHERE id=?1",
                params![job.to_string(), format!("summary:{foreign_session}")],
            )?;
        }
        "permanent" => {
            tx.execute(
                "UPDATE intelligence_jobs SET permanent_key=?2 WHERE id=?1",
                params![job.to_string(), format!("auto-summary:{foreign_session}")],
            )?;
        }
        "payload_bytes" => {
            tx.execute(
                "UPDATE intelligence_jobs SET payload_json=payload_json || ' ' WHERE id=?1",
                [job.to_string()],
            )?;
        }
        "native_profile" => {
            tx.execute(
                "UPDATE session_summaries SET profile_id=?2 WHERE id=?1",
                params![native.to_string(), Uuid::new_v4().to_string()],
            )?;
        }
        "native_model" => {
            tx.execute(
                "UPDATE session_summaries SET model='changed' WHERE id=?1",
                [native.to_string()],
            )?;
        }
        "native_session" => {
            tx.execute(
                "UPDATE session_summaries SET session_id=?2 WHERE id=?1",
                params![native.to_string(), foreign_session.to_string()],
            )?;
        }
        "native_version" => {
            tx.execute(
                "UPDATE session_summaries SET version=2 WHERE id=?1",
                [native.to_string()],
            )?;
        }
        "native_job" | "native_ms" | "witness_missing" | "witness_upper" => {
            tx.pragma_update(None, "ignore_check_constraints", true)?;
            let column = match field {
                "native_job" => "job_id",
                "native_ms" => "created_at_ms",
                _ => "job_payload_hash",
            };
            if field == "witness_upper" {
                tx.execute(
                    "UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",
                    params![native.to_string(), "A".repeat(64)],
                )?;
            } else {
                tx.execute(
                    &format!("UPDATE session_summaries SET {column}=NULL WHERE id=?1"),
                    [native.to_string()],
                )?;
            }
        }
        "witness_bad" => {
            tx.execute(
                "UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",
                params![native.to_string(), "b".repeat(64)],
            )?;
        }
        _ => {
            payload_changed = true;
            match field {
                "input_summary" => changed["input"]["summary_id"] = foreign.to_string().into(),
                "input_session" => {
                    changed["input"]["session_id"] = foreign_session.to_string().into()
                }
                "input_extra" => changed["input"]["extra"] = "untrusted".into(),
                "profile_id" => changed["profiles"][0]["id"] = Uuid::new_v4().to_string().into(),
                "profile_kind" => changed["profiles"][0]["kind"] = "embedding".into(),
                "profile_name" => changed["profiles"][0]["name"] = "changed".into(),
                "profile_url" => {
                    changed["profiles"][0]["base_url"] = "http://127.0.0.1:23456/v1".into()
                }
                "profile_model" => changed["profiles"][0]["model"] = "changed".into(),
                "profile_timeout" => changed["profiles"][0]["timeout_ms"] = 2000.into(),
                "profile_context" => changed["profiles"][0]["max_context_tokens"] = 8192.into(),
                "profile_output" => changed["profiles"][0]["max_output_tokens"] = 1024.into(),
                "profile_dimensions" => changed["profiles"][0]["dimensions"] = 128.into(),
                "profile_count" => {
                    let first = changed["profiles"][0].clone();
                    changed["profiles"].as_array_mut().unwrap().push(first);
                }
                "profile_absent" => changed["profiles"] = serde_json::json!([]),
                _ => unreachable!(),
            }
        }
    }
    if payload_changed {
        tx.execute(
            "UPDATE intelligence_jobs SET payload_json=?2 WHERE id=?1",
            params![job.to_string(), changed.to_string()],
        )?;
    }
    Ok(())
}
fn values(tx: &Transaction<'_>, table: &str, id: Uuid) -> AppResult<Vec<Value>> {
    let mut query = tx.prepare(&format!("SELECT * FROM {table} WHERE id=?1"))?;
    let count = query.column_count();
    let mut rows = query.query([id.to_string()])?;
    let row = rows.next()?.unwrap();
    Ok((0..count)
        .map(|index| row.get(index))
        .collect::<rusqlite::Result<Vec<_>>>()?)
}
fn foreign_state(
    h: &IntelligenceHarness,
    native: Uuid,
    legacy: Uuid,
    job: Uuid,
) -> Vec<Vec<Value>> {
    h.store
        .transaction(|tx| {
            Ok(vec![
                values(tx, "session_summaries", native)?,
                values(tx, "session_summaries", legacy)?,
                values(tx, "intelligence_jobs", job)?,
            ])
        })
        .unwrap()
}

// These exercise the actual snapshot and publication consumers of the carried
// capability. Dropping a fresh check at any boundary would allow a localized
// durable association/profile mutation to publish under the old capability.
#[tokio::test]
async fn summary_boundaries_each_domain_and_full_profile_mutation_fail_closed_without_foreign_or_legacy_effects()
 {
    for boundary in ["snapshot", "precall", "raw", "root"] {
        for field in FIELDS {
            let (h, session, proof, owner) = fixture();
            let native = proof.summary_id();
            h.store
                .append_final_segment(&StoredTranscriptSegment {
                    id: Uuid::new_v4(),
                    session_id: session,
                    direction: "source_to_target".into(),
                    sequence: 0,
                    occurred_at: 1,
                    language: "zh".into(),
                    text: "全部输入".into(),
                    is_final: true,
                    details: None,
                    occurred_at_ms: None,
                })
                .unwrap();
            h.store
                .transaction(|tx| {
                    tx.execute(
                        "UPDATE session_summaries SET status='running' WHERE id=?1",
                        [native.to_string()],
                    )?;
                    Ok(())
                })
                .unwrap();
            let (_keep, cancel) = tokio::sync::watch::channel(false);
            let snapshot = FinalSnapshot::capture(
                h.store.clone(),
                proof.clone(),
                owner.clone(),
                cancel.clone(),
            )
            .await
            .unwrap();
            let maps = planner::plan_maps(&snapshot, proof.profile().unwrap()).unwrap();
            assert_eq!(maps.requests.len(), 1);
            let response = LlmResponse {
                content: VALID.into(),
                response_hash: store::hash(VALID.as_bytes()),
            };
            if boundary == "root" {
                h.store
                    .transaction(|tx| {
                        store::retain_response_in_tx(
                            tx,
                            &proof,
                            &maps.manifest,
                            &maps.requests[0],
                            &response,
                        )
                    })
                    .unwrap();
            }
            let foreign_session = h.session();
            h.store.transaction(|tx| {tx.execute("INSERT INTO summary_version_counters VALUES(?1,6) ON CONFLICT(session_id) DO UPDATE SET last_version=6",[foreign_session.to_string()])?;Ok(())}).unwrap();
            let foreign = h
                .store
                .create_summary_job(SummaryJobRequest {
                    session_id: foreign_session,
                    profile: Some(crate::intelligence::test_support::test_llm_profile()),
                    automatic: false,
                    initial_failure: None,
                })
                .unwrap();
            // Release the foreign exclusion through the actual registered
            // Summary terminal hook before testing a foreign exclusion lure.
            let foreign_service = crate::summary::SummaryService::new(
                h.store.clone(),
                h.config.clone(),
                h.jobs.clone(),
                std::sync::Arc::new(crate::llm::OpenAiCompatibleLlm::new(
                    h.config.clone(),
                    h.http.clone(),
                )),
            );
            foreign_service.register_handler().unwrap();
            h.jobs.cancel(foreign.id).await.unwrap();
            let legacy = Uuid::new_v4();
            h.store
                .create_summary_version(&SummaryVersion {
                    id: legacy,
                    session_id: foreign_session,
                    version: -7,
                    created_at: -99,
                    status: "legacy-original".into(),
                    structured_json: r#"{"text":"foreign legacy sentinel"}"#.into(),
                })
                .unwrap();
            let before = foreign_state(&h, foreign.object_id.unwrap(), legacy, foreign.id);
            let payload = h
                .store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                        [proof.job_id().to_string()],
                        |row| row.get::<_, String>(0),
                    )?)
                })
                .unwrap();
            h.store
                .transaction(|tx| {
                    mutate(
                        tx,
                        proof.job_id(),
                        native,
                        &serde_json::from_str(&payload).unwrap(),
                        field,
                        foreign.object_id.unwrap(),
                        foreign_session,
                    )
                })
                .unwrap();
            let prior = h
                .store
                .transaction(|tx| {
                    Ok((
                        values(tx, "session_summaries", native)?,
                        values(tx, "intelligence_jobs", proof.job_id())?,
                    ))
                })
                .unwrap();
            let result = match boundary {
                "snapshot" => {
                    FinalSnapshot::capture(h.store.clone(), proof.clone(), owner.clone(), cancel)
                        .await
                        .map(|_| ())
                }
                "precall" => h
                    .store
                    .transaction(|tx| proof.running_non_deleting_in_tx(tx)),
                "raw" => h.store.transaction(|tx| {
                    store::retain_response_in_tx(
                        tx,
                        &proof,
                        &maps.manifest,
                        &maps.requests[0],
                        &response,
                    )
                }),
                "root" => h.store.transaction(|tx| {
                    store::publish_root_in_tx(
                        tx,
                        &proof,
                        &maps.manifest,
                        &SummaryNode {
                            content: SummaryContent::parse(VALID).unwrap(),
                            coverage: 0..maps.manifest.parts.len(),
                        },
                    )
                }),
                _ => unreachable!(),
            };
            assert!(
                matches!(
                    result,
                    Err(AppError::Intelligence(IntelligenceErrorCode::Storage))
                ),
                "{boundary} authorized localized {field} drift"
            );
            assert_eq!(
                foreign_state(&h, foreign.object_id.unwrap(), legacy, foreign.id),
                before,
                "{boundary}/{field} changed a lure"
            );
            assert_eq!(
                h.store
                    .transaction(|tx| Ok((
                        values(tx, "session_summaries", native)?,
                        values(tx, "intelligence_jobs", proof.job_id())?
                    )))
                    .unwrap(),
                prior,
                "failed {boundary}/{field} had a domain effect"
            );
            let (raw, coverage) = h
                .store
                .transaction(|tx| {
                    Ok((
                        tx.query_row(
                            "SELECT count(*) FROM summary_responses WHERE summary_id=?1",
                            [native.to_string()],
                            |r| r.get::<_, i64>(0),
                        )?,
                        tx.query_row(
                            "SELECT count(*) FROM summary_coverage WHERE summary_id=?1",
                            [native.to_string()],
                            |r| r.get::<_, i64>(0),
                        )?,
                    ))
                })
                .unwrap();
            assert_eq!(
                (raw, coverage),
                if boundary == "root" { (1, 1) } else { (0, 0) }
            );
            assert!(h.http.requests.lock().unwrap().is_empty());
        }
    }
}
