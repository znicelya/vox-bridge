use super::*;

fn seed_job(
    h: &KnowledgeHarness,
    id: Uuid,
    kind: JobKind,
    object: Uuid,
    payload: serde_json::Value,
) {
    h.h.store
        .transaction(|tx| {
            crate::intelligence::store::admit_job(
                tx,
                crate::intelligence::store::JobDraft {
                    id,
                    kind,
                    session_id: None,
                    object_id: Some(object),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{object}"),
                    },
                    payload,
                    initial_failure: None,
                },
            )?;
            Ok(())
        })
        .unwrap();
}

fn status(h: &KnowledgeHarness, id: Uuid) -> String {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT status FROM documents WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

#[tokio::test]
async fn knowledge_review_startup_requires_bounded_matching_reindex_ownership() {
    let mut actual = Vec::new();
    for mutation in [
        "valid",
        "wrong_document",
        "missing_document",
        "oversize",
        "malformed",
        "wrong_kind",
        "profile_kind",
        "duplicate_profile",
        "exclusion",
        "noncanonical_uuid",
    ] {
        let h = KnowledgeHarness::new().await;
        h.h.jobs.shutdown().await.unwrap();
        let id = h.import_text("owned", "24V");
        let foreign = h.import_text("foreign", "48V");
        let profile =
            h.h.config
                .selected_embedding_snapshot()
                .await
                .unwrap()
                .unwrap();
        let base = serde_json::json!({"version":1,"profiles":[profile.clone()],
            "input":{"document_id":id,"document_version":1}});
        let job = Uuid::new_v4();
        seed_job(&h, job, JobKind::Reindex, id, base.clone());
        let mut payload = base;
        match mutation {
            "wrong_document" => payload["input"]["document_id"] = serde_json::json!(foreign),
            "missing_document" => {
                payload["input"]
                    .as_object_mut()
                    .unwrap()
                    .remove("document_id");
            }
            "profile_kind" => payload["profiles"] = serde_json::json!([test_llm_profile()]),
            "duplicate_profile" => {
                payload["profiles"] = serde_json::json!([profile.clone(), profile])
            }
            "noncanonical_uuid" => {
                payload["input"]["document_id"] = serde_json::json!(id.to_string().to_uppercase())
            }
            _ => {}
        }
        let raw = match mutation {
            "oversize" => format!("{}{}", payload, " ".repeat(65537)),
            "malformed" => "{".into(),
            _ => payload.to_string(),
        };
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE intelligence_jobs SET payload_json=?1 WHERE id=?2",
                    rusqlite::params![raw, job.to_string()],
                )?;
                if mutation == "wrong_kind" {
                    tx.execute(
                        "UPDATE intelligence_jobs SET kind='delete_document' WHERE id=?1",
                        [job.to_string()],
                    )?;
                }
                if mutation == "exclusion" {
                    tx.execute(
                        "UPDATE intelligence_jobs SET exclusion_key=?1 WHERE id=?2",
                        rusqlite::params![format!("document:{foreign}"), job.to_string()],
                    )?;
                }
                tx.execute(
                    "UPDATE documents SET status='indexing' WHERE id=?1",
                    [id.to_string()],
                )?;
                tx.execute(
                    "UPDATE document_chunks SET index_state='indexing' WHERE document_id=?1",
                    [id.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        let result = h.service.reconcile_owned(Arc::new(())).await;
        actual.push((
            mutation,
            result.is_ok(),
            status(&h, id),
            h.embeddings.calls.load(Ordering::SeqCst) == calls,
        ));
        assert_eq!(status(&h, foreign), "parsed");
        h.index.shutdown().await.unwrap();
    }
    assert_eq!(
        actual,
        vec![
            ("valid", true, "indexing".into(), true),
            ("wrong_document", true, "repair_required".into(), true),
            ("missing_document", true, "repair_required".into(), true),
            ("oversize", true, "repair_required".into(), true),
            ("malformed", true, "repair_required".into(), true),
            ("wrong_kind", true, "repair_required".into(), true),
            ("profile_kind", true, "repair_required".into(), true),
            ("duplicate_profile", true, "repair_required".into(), true),
            ("exclusion", true, "repair_required".into(), true),
            ("noncanonical_uuid", true, "repair_required".into(), true),
        ]
    );
}

#[tokio::test]
async fn knowledge_review_runtime_reindex_cannot_mutate_a_foreign_document() {
    let h = KnowledgeHarness::new().await;
    h.h.jobs.shutdown().await.unwrap();
    let owned = h.import_text("owned", "24V");
    let foreign = h.import_text("foreign", "48V");
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let job = Uuid::new_v4();
    seed_job(
        &h,
        job,
        JobKind::Reindex,
        owned,
        serde_json::json!({
            "version":1,"profiles":[profile],"input":{"document_id":foreign,"document_version":1}
        }),
    );
    h.h.jobs.start().unwrap();
    let result = h.wait(job).await;
    assert_eq!(result.status, JobStatus::Failed);
    assert_eq!(result.error, Some(IntelligenceErrorCode::Configuration));
    assert_eq!(status(&h, owned), "parsed");
    assert_eq!(status(&h, foreign), "parsed");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_runtime_delete_rejects_consistent_input_for_the_wrong_job_object() {
    let h = KnowledgeHarness::new().await;
    h.h.jobs.shutdown().await.unwrap();
    let owned = h.import_text("owned", "24V");
    let foreign = h.import_text("foreign", "48V");
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let input = |document_id, operation_id| crate::knowledge::service::DeleteInput {
        document_id,
        document_version: 1,
        document: h
            .h
            .store
            .transaction(|tx| crate::knowledge::service::state::fence(tx, document_id))
            .unwrap(),
        operation_id,
        identity: None,
        generation: None,
    };
    seed_job(
        &h,
        first,
        JobKind::DeleteDocument,
        owned,
        serde_json::json!({
            "version":1,"profiles":[],"input":input(owned, first)
        }),
    );
    seed_job(
        &h,
        second,
        JobKind::DeleteDocument,
        foreign,
        serde_json::json!({
            "version":1,"profiles":[],"input":input(foreign, second)
        }),
    );
    let replacement = serde_json::json!({"version":1,"profiles":[],"input":input(foreign, first)});
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET payload_json=?1 WHERE id=?2",
                rusqlite::params![replacement.to_string(), first.to_string()],
            )?;
            // The other real deletion remains interrupted while the bad live job runs.
            tx.execute(
                "UPDATE intelligence_jobs SET status='interrupted' WHERE id=?1",
                [second.to_string()],
            )?;
            tx.execute(
                "UPDATE documents SET status='deleting' WHERE id IN (?1,?2)",
                rusqlite::params![owned.to_string(), foreign.to_string()],
            )?;
            tx.execute(
                "UPDATE document_chunks SET index_state='deleting' WHERE document_id IN (?1,?2)",
                rusqlite::params![owned.to_string(), foreign.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    h.h.jobs.start().unwrap();
    let result = h.wait(first).await;
    assert_eq!(result.status, JobStatus::Failed);
    assert_eq!(result.error, Some(IntelligenceErrorCode::Configuration));
    assert_eq!(status(&h, owned), "deleting");
    assert_eq!(status(&h, foreign), "deleting");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_stale_terminal_job_cannot_overwrite_a_newer_repair_reason() {
    let h = KnowledgeHarness::new().await;
    h.h.jobs.shutdown().await.unwrap();
    let document = h.import_text("owned", "24V");
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let job = Uuid::new_v4();
    seed_job(
        &h,
        job,
        JobKind::Reindex,
        document,
        serde_json::json!({
            "version":1,"profiles":[profile],"input":{"document_id":document,"document_version":1}
        }),
    );
    h.h.store.transaction(|tx| {
        tx.execute("UPDATE documents SET version=2,status='repair_required' WHERE id=?1", [document.to_string()])?;
        tx.execute("UPDATE document_chunks SET document_version=2,index_state='repair_required' WHERE document_id=?1", [document.to_string()])?;
        tx.execute("INSERT INTO knowledge_repairs(document_id,reason) VALUES (?1,'newer_association')", [document.to_string()])?;
        Ok(())
    }).unwrap();
    h.h.jobs.start().unwrap();
    assert_ne!(h.wait(job).await.status, JobStatus::Succeeded);
    let reason: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT reason FROM knowledge_repairs WHERE document_id=?1",
                    [document.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(reason, "newer_association");
    assert_eq!(status(&h, document), "repair_required");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_ready_publication_rechecks_the_durable_job_object() {
    let h = KnowledgeHarness::new().await;
    let owned = h.import_text("owned", "24V");
    let foreign = h.import_text("foreign", "48V");
    let store = h.h.store.clone();
    h.service.at_checkpoint(CommitPoint::BeforeReady, move || {
        store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE intelligence_jobs SET object_id=?1,exclusion_key=?2 WHERE \
                kind='reindex' AND status='running' AND object_id=?3",
                    rusqlite::params![
                        foreign.to_string(),
                        format!("document:{foreign}"),
                        owned.to_string()
                    ],
                )?;
                Ok(())
            })
            .unwrap();
    });
    assert_ne!(h.index(owned).await.status, JobStatus::Succeeded);
    assert_ne!(status(&h, owned), "ready");
    assert_eq!(status(&h, foreign), "parsed");
    let published: u32 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM knowledge_generation_mappings WHERE published=1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(published, 0);
    h.close().await;
}
