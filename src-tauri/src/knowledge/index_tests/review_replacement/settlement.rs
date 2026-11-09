use super::flags::{has_certificate, operation_key};
use super::*;
use crate::knowledge::index::delete_completion as completed;

async fn settled() -> (KnowledgeHarness, Uuid, crate::knowledge::index::Generation) {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    let input =
        h.h.store
            .transaction(|tx| Ok(completed::load(tx, job.id)?.unwrap().header.input))
            .unwrap();
    let serial = h.service.core.serial.lock().await;
    h.service
        .core
        .finish_completion(&input, Arc::new(()))
        .await
        .unwrap();
    drop(serial);
    let certificate =
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .unwrap();
    assert!(certificate.acknowledged && certificate.consumed);
    (h, job.id, source)
}

// Replaying every settled operation would redo its whole cohort and mutate the
// same acknowledged marker many times, making 202 peers multiplicative work.
#[tokio::test]
async fn knowledge_review_settlement_does_not_replay_proved_consumed_history() {
    let (h, operation, source) = settled().await;
    h.h.store
        .transaction(|tx| {
            tx.execute_batch(&format!(
                "CREATE TRIGGER reject_history_replay BEFORE UPDATE ON intelligence_meta \
            WHEN OLD.key='{}' BEGIN SELECT RAISE(ABORT,'settled history was replayed'); END;",
                operation_key(operation)
            ))?;
            Ok(())
        })
        .unwrap();
    let resumed = h.service.core.resume_completions(Arc::new(())).await;
    let retained = has_certificate(&h, operation) && generation_path(&h, &source).exists();
    h.h.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER reject_history_replay")?;
            Ok(())
        })
        .unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.rebuild_owned(Arc::new(())).await.unwrap();
    assert!(!has_certificate(&h, operation) && !generation_path(&h, &source).exists());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
    assert!(
        resumed.is_ok(),
        "fully proved consumed history must remain retirement authority without replaying cleanup"
    );
    assert!(retained);
}

// One mutable consumed flag, a missing/foreign receipt, or a syntactically
// valid changed immutable header cannot stand in for anchored actual proof.
#[tokio::test]
async fn knowledge_review_settlement_requires_independent_header_bound_consumption_receipt() {
    let (h, operation, source) = settled().await;
    let path = generation_path(&h, &source);
    let original_vectors = std::fs::read(path.join("vectors.bin")).unwrap();
    let certificate =
        h.h.store
            .transaction(|tx| completed::load(tx, operation))
            .unwrap()
            .unwrap();
    let receipt_key = format!("knowledge.delete.v1.consumed.{operation}");
    let receipt: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT value_json FROM intelligence_meta WHERE key=?1",
                    [&receipt_key],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
    let mut rejected = Vec::new();
    for mutation in ["flag", "missing_receipt", "foreign_receipt", "header"] {
        h.h.store
            .transaction(|tx| {
                let mut changed = certificate.clone();
                if mutation == "flag" {
                    changed.consumed = false
                }
                if mutation == "header" {
                    changed.header.work_root = "b".repeat(64)
                }
                tx.execute(
                    "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                    rusqlite::params![
                        operation_key(operation),
                        serde_json::to_string(&changed).unwrap()
                    ],
                )?;
                if mutation == "missing_receipt" {
                    tx.execute("DELETE FROM intelligence_meta WHERE key=?1", [&receipt_key])?;
                } else if mutation == "foreign_receipt" {
                    let mut changed: serde_json::Value = serde_json::from_str(&receipt).unwrap();
                    changed["operation"] = serde_json::json!(Uuid::new_v4().to_string());
                    tx.execute(
                        "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                        rusqlite::params![receipt_key, changed.to_string()],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        rejected.push(
            h.service
                .core
                .resume_completions(Arc::new(()))
                .await
                .is_err(),
        );
        assert!(path.exists() && has_certificate(&h, operation));
        assert_eq!(
            std::fs::read(path.join("vectors.bin")).unwrap(),
            original_vectors
        );
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                    rusqlite::params![
                        operation_key(operation),
                        serde_json::to_string(&certificate).unwrap()
                    ],
                )?;
                tx.execute(
                    "INSERT INTO intelligence_meta(key,value_json) VALUES (?1,?2) \
                ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
                    rusqlite::params![receipt_key, receipt],
                )?;
                Ok(())
            })
            .unwrap();
    }
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.rebuild_owned(Arc::new(())).await.unwrap();
    assert!(!has_certificate(&h, operation) && !path.exists());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
    assert_eq!(rejected, vec![true, true, true, true]);
}

// A fresh zero-record receipt must still account for every source delete and
// retire work idempotently; no placeholder vector or recovery embedding exists.
#[tokio::test]
async fn knowledge_review_settlement_two_retained_deletes_publish_empty_native_destination() {
    let h = KnowledgeHarness::new().await;
    let first_target = h.import_text("first target", "24V");
    assert_eq!(h.index(first_target).await.status, JobStatus::Succeeded);
    let second_target = h.import_text("second target", "48V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(second_target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    let first = h
        .service
        .delete_owned(first_target, Arc::new(()))
        .await
        .unwrap();
    assert_eq!(h.wait(first.id).await.status, JobStatus::Failed);
    let second = h
        .service
        .delete_owned(second_target, Arc::new(()))
        .await
        .unwrap();
    assert_eq!(h.wait(second.id).await.status, JobStatus::Failed);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    let stored = h.h.store.transaction(|tx| {
        let raw: String = tx.query_row("SELECT s.seal_json FROM knowledge_index_generation_seals s \
            JOIN knowledge_index_generations g ON g.identity=s.identity AND g.active_generation=s.generation \
            WHERE s.dirty=0", [], |row| row.get(0))?;
        Ok(crate::knowledge::index::Seal::decode(&raw)?.stored_count)
    }).unwrap();
    assert_eq!(stored, 0);
    assert!(!exists(&h, first_target) && !exists(&h, second_target));
    assert_eq!(
        (work_count(&h, first.id), work_count(&h, second.id)),
        (0, 0)
    );
    assert!(!has_certificate(&h, first.id) && !has_certificate(&h, second.id));
    assert!(!generation_path(&h, &source).exists());
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
}

// A replaced operation/page acknowledgement flag cannot make still-present
// original work disappear from the certified range or advance retirement.
#[tokio::test]
async fn knowledge_review_settlement_rejects_acknowledged_flags_with_original_work_present() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    let receipt = completed::check_files(
        h.h.store.clone(),
        h.h.temp_dir.path().to_path_buf(),
        job.id,
        Arc::new(()),
        true,
    )
    .await
    .unwrap()
    .unwrap();
    h.h.store
        .transaction(|tx| completed::mark_files_cleaned(tx, job.id, &receipt))
        .unwrap();
    let certificate =
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .unwrap();
    let page_key = format!("knowledge.delete.v1.page.{}.{:020}", job.id, 0);
    let original_page: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT value_json FROM intelligence_meta WHERE key=?1",
                    [&page_key],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
    let mut rejected = Vec::new();
    for mutation in ["operation_flag", "page_flag", "missing_page"] {
        h.h.store
            .transaction(|tx| {
                if mutation == "operation_flag" {
                    let mut changed = certificate.clone();
                    changed.acknowledged = true;
                    tx.execute(
                        "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                        rusqlite::params![
                            operation_key(job.id),
                            serde_json::to_string(&changed).unwrap()
                        ],
                    )?;
                } else if mutation == "page_flag" {
                    let mut changed: serde_json::Value =
                        serde_json::from_str(&original_page).unwrap();
                    changed["acknowledged"] = serde_json::json!(true);
                    tx.execute(
                        "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                        rusqlite::params![page_key, changed.to_string()],
                    )?;
                } else {
                    tx.execute("DELETE FROM intelligence_meta WHERE key=?1", [&page_key])?;
                }
                Ok(())
            })
            .unwrap();
        rejected.push(
            h.service
                .core
                .resume_completions(Arc::new(()))
                .await
                .is_err(),
        );
        assert_eq!(work_count(&h, job.id), 1);
        assert!(generation_path(&h, &source).exists());
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                    rusqlite::params![
                        operation_key(job.id),
                        serde_json::to_string(&certificate).unwrap()
                    ],
                )?;
                tx.execute(
                    "INSERT INTO intelligence_meta(key,value_json) VALUES (?1,?2) \
                ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
                    rusqlite::params![page_key, original_page],
                )?;
                Ok(())
            })
            .unwrap();
    }
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(work_count(&h, job.id), 0);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
    assert_eq!(rejected, vec![true, true, true]);
}

// Aborting GC on the second member must restore the first member's header,
// original pages and receipt atomically. A reopened consumed cohort whose
// physical source already ended still needs an explicit GC retry.
#[tokio::test]
async fn knowledge_review_settlement_atomic_cohort_gc_retries_after_source_removal() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let (_, final_path, temp_path) = super::flags::managed_file(&h, target);
    let serial = h.service.core.serial.lock().await;
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    let mut operations = duplicate_attempts(&h, job.id, 1);
    drop(serial);
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    let input =
        h.h.store
            .transaction(|tx| Ok(completed::load(tx, job.id)?.unwrap().header.input))
            .unwrap();
    let serial = h.service.core.serial.lock().await;
    h.service
        .core
        .finish_completion(&input, Arc::new(()))
        .await
        .unwrap();
    drop(serial);
    operations.sort();
    h.h.store
        .transaction(|tx| {
            tx.execute_batch(&format!(
                "CREATE TRIGGER reject_second_gc_member BEFORE DELETE ON intelligence_meta \
            WHEN OLD.key='{}' BEGIN SELECT RAISE(ABORT,'complete cohort GC rollback'); END;",
                operation_key(operations[1])
            ))?;
            Ok(())
        })
        .unwrap();
    let failed_gc = h.service.rebuild_owned(Arc::new(())).await.is_err();
    let publication =
        h.h.store
            .transaction(|tx| {
                let certificate = completed::load(tx, job.id)?.unwrap();
                completed::read_publication(tx, certificate.header.publication)
            })
            .unwrap();
    let whole_cohort = operations.iter().all(|operation| {
        h.h.store
            .transaction(|tx| {
                let value = completed::load(tx, *operation)?.unwrap();
                Ok(value.consumed
                    && value.acknowledged
                    && tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM intelligence_meta WHERE key=?1)",
                        [format!("knowledge.delete.v1.page.{operation}.{:020}", 0)],
                        |row| row.get::<_, bool>(0),
                    )?)
            })
            .unwrap()
    });
    assert!(publication.source_removed && !generation_path(&h, &source).exists());
    assert!(!final_path.exists() && !temp_path.exists());
    let publication_key = format!(
        "knowledge.delete.v1.publication.{}",
        publication.authority.id
    );
    h.h.store
        .transaction(|tx| {
            let mut changed = publication.clone();
            changed.source_removed = false;
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![publication_key, serde_json::to_string(&changed).unwrap()],
            )?;
            Ok(())
        })
        .unwrap();
    let removal_flag_rejected = h
        .service
        .core
        .resume_completions(Arc::new(()))
        .await
        .is_err();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![
                    publication_key,
                    serde_json::to_string(&publication).unwrap()
                ],
            )?;
            tx.execute_batch("DROP TRIGGER reject_second_gc_member")?;
            Ok(())
        })
        .unwrap();
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    // GC retry must inspect the actual certified paths again even after source
    // removal and a legitimate earlier cleanup/consumption receipt.
    std::fs::write(&temp_path, b"residual bytes before GC retry").unwrap();
    let residual_rejected = h.service.reconcile_owned(Arc::new(())).await.is_err();
    assert!(
        operations
            .iter()
            .all(|operation| has_certificate(&h, *operation))
    );
    assert_eq!(
        std::fs::read(&temp_path).unwrap(),
        b"residual bytes before GC retry"
    );
    std::fs::remove_file(&temp_path).unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    let collected = operations
        .iter()
        .all(|operation| !has_certificate(&h, *operation));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
    assert!(failed_gc && whole_cohort);
    assert!(
        removal_flag_rejected,
        "source removal flag must agree with its independent actual removal receipt"
    );
    assert!(
        residual_rejected,
        "actual original paths must remain absent before journal GC retries"
    );
    assert!(
        collected,
        "a fully consumed closed cohort must retry atomic GC after its source ended"
    );
}
