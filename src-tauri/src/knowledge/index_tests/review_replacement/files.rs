use super::flags::{has_certificate, managed_file};
use super::*;

async fn native_imported() -> (KnowledgeHarness, JobView) {
    let h = KnowledgeHarness::with_auto(true).await;
    let documents = crate::knowledge::documents::DocumentService::new(
        h.h.store.clone(),
        h.h.config.clone(),
        h.h.jobs.clone(),
        Arc::new(FixtureParser),
        h.h.temp_dir.path().to_path_buf(),
    );
    let original = h.h.temp_dir.path().join("owned.txt");
    std::fs::write(&original, b"original native file").unwrap();
    let imported = documents
        .import_owned(original, Arc::new(()))
        .await
        .unwrap();
    assert_eq!(h.wait(imported.id).await.status, JobStatus::Succeeded);
    let target = imported.object_id.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let settled: bool = h.h.store.transaction(|tx| Ok(tx.query_row(
                "SELECT status='ready' AND NOT EXISTS(SELECT 1 FROM intelligence_jobs \
                    WHERE object_id=?1 AND status IN ('queued','running')) FROM documents WHERE id=?1",
                [target.to_string()], |r| r.get(0),
            )?)).unwrap();
            if settled { break }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    (h, imported)
}

#[tokio::test]
async fn knowledge_review_native_imported_managed_path_deletes_locally() {
    let (h, imported) = native_imported().await;
    let target = imported.object_id.unwrap();
    let (file, name): (String, String) =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
            "SELECT file_id,relative_name FROM knowledge_managed_files WHERE document_id=?1",
            [target.to_string()], |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
            })
            .unwrap();
    let directory = h.h.temp_dir.path().join("knowledge").join("documents");
    let final_path = directory.join(name);
    let temporary_path = directory.join(format!("{file}.tmp"));
    assert!(final_path.is_file());
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    let finished = h.wait(job.id).await;
    assert_eq!(
        finished.status,
        JobStatus::Succeeded,
        "the real NativePath producer must be accepted by deletion"
    );
    assert!(!exists(&h, target));
    assert_eq!(work_count(&h, job.id), 0);
    assert!(!final_path.exists() && !temporary_path.exists());
    let remaining: u64 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM knowledge_managed_files WHERE job_id=?1",
                    [imported.id.to_string()],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(remaining, 0);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_managed_full_path_drift_is_rejected_before_native_delete() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let (file, final_path, temp_path) = managed_file(&h, target);
    let foreign = h.h.temp_dir.path().join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let drifted = foreign.join(format!("{file}.txt"));
    std::fs::write(&drifted, b"foreign original path bytes").unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET managed_path=?2 WHERE id=?1",
                rusqlite::params![
                    target.to_string(),
                    crate::knowledge::store::encode_path(&drifted)?
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(
        h.wait(job.id).await.status,
        JobStatus::Failed,
        "a matching basename cannot authorize a different full path"
    );
    assert!(exists(&h, target));
    assert_eq!(work_count(&h, job.id), 1);
    let dirty: bool =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
        "SELECT dirty FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2",
        rusqlite::params![source.key().unwrap(), source.generation.to_string()], |r| r.get(0),
    )?)
            })
            .unwrap();
    assert!(
        !dirty,
        "full cleanup ownership must be checked before an engine intent"
    );
    assert!(source_path.exists() && final_path.exists() && temp_path.exists());
    assert_eq!(
        std::fs::read(&drifted).unwrap(),
        b"foreign original path bytes"
    );
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_managed_path_change_before_publication_retains_original_work() {
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("target", "24V");
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let (file, final_path, temp_path) = managed_file(&h, target);
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET managed_path=?2 WHERE id=?1",
                rusqlite::params![
                    target.to_string(),
                    crate::knowledge::store::encode_path(&final_path)?
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let foreign = h.h.temp_dir.path().join(format!("{file}.txt"));
    std::fs::write(&foreign, b"different original path").unwrap();
    let store = h.h.store.clone();
    let wrong = foreign.clone();
    h.service
        .at_checkpoint(CommitPoint::AfterVectorPersist, move || {
            store
                .transaction(|tx| {
                    tx.execute(
                        "UPDATE documents SET managed_path=?2 WHERE id=?1",
                        rusqlite::params![
                            target.to_string(),
                            crate::knowledge::store::encode_path(&wrong)?
                        ],
                    )?;
                    Ok(())
                })
                .unwrap();
        });
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(
        h.wait(job.id).await.status,
        JobStatus::Failed,
        "publication must recheck the entire original path association"
    );
    assert!(exists(&h, target));
    assert_eq!(work_count(&h, job.id), 1);
    assert!(!has_certificate(&h, job.id));
    assert!(source_path.exists() && final_path.exists() && temp_path.exists() && foreign.exists());
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET managed_path=?2 WHERE id=?1",
                rusqlite::params![
                    target.to_string(),
                    crate::knowledge::store::encode_path(&final_path)?
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!exists(&h, target));
    assert!(!final_path.exists() && !temp_path.exists());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(std::fs::read(foreign).unwrap(), b"different original path");
    h.index.shutdown().await.unwrap();
}

#[tokio::test]
async fn knowledge_review_cleanup_rejects_additional_file_owned_by_original_import_job() {
    let (h, imported) = native_imported().await;
    let target = imported.object_id.unwrap();
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let extra = Uuid::new_v4();
    let path =
        h.h.temp_dir
            .path()
            .join("knowledge")
            .join("documents")
            .join(format!("{extra}.txt"));
    std::fs::write(&path, b"unlisted target-owned bytes").unwrap();
    let store = h.h.store.clone();
    h.service.at_checkpoint(CommitPoint::AfterDeletePublication, move || {
        store.transaction(|tx| {
            tx.execute("INSERT INTO knowledge_managed_files(file_id,relative_name,document_version,job_id,state,created_at_ms) \
                VALUES (?1,?2,1,?3,'delete_pending',1)", rusqlite::params![extra.to_string(), format!("{extra}.txt"), imported.id.to_string()])?;
            Ok(())
        }).unwrap();
    });
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(
        h.wait(job.id).await.status,
        JobStatus::Failed,
        "a new target-owned file cannot be cleaned as an uncertified prefix"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"unlisted target-owned bytes"
    );
    assert!(has_certificate(&h, job.id) && source_path.exists());
    assert_eq!(work_count(&h, job.id), 1);
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_managed_files WHERE file_id=?1",
                [extra.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    std::fs::remove_file(path).unwrap();
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_owned_managed_cleanup_consumer_cannot_bypass_certified_file_set() {
    let (h, imported) = native_imported().await;
    let target = imported.object_id.unwrap();
    let extra = Uuid::new_v4();
    let path =
        h.h.temp_dir
            .path()
            .join("knowledge")
            .join("documents")
            .join(format!("{extra}.txt"));
    std::fs::write(&path, b"additional original-owner bytes").unwrap();
    let store = h.h.store.clone();
    h.service.at_checkpoint(CommitPoint::AfterDeletePublication, move || {
        store.transaction(|tx| {
            tx.execute("INSERT INTO knowledge_managed_files(file_id,relative_name,document_version,job_id,state,created_at_ms) \
                VALUES (?1,?2,1,?3,'delete_pending',1)", rusqlite::params![extra.to_string(), format!("{extra}.txt"), imported.id.to_string()])?;
            Ok(())
        }).unwrap();
    });
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    assert!(has_certificate(&h, job.id));
    assert!(
        crate::knowledge::documents::cleanup_managed(
            h.h.store.clone(),
            h.h.temp_dir.path().to_path_buf(),
            Arc::new(())
        )
        .await
        .is_err(),
        "the actual managed cleanup consumer must guard the complete certified owner set"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"additional original-owner bytes"
    );
    assert_eq!(work_count(&h, job.id), 1);
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_managed_files WHERE file_id=?1",
                [extra.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    std::fs::remove_file(path).unwrap();
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_real_managed_cleanup_failure_keeps_source_work_and_retries() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let (file, final_path, temp_path) = managed_file(&h, target);
    h.h.store
        .transaction(|tx| {
            tx.execute_batch(&format!(
                "CREATE TRIGGER retained_cleanup_failure BEFORE DELETE ON knowledge_managed_files \
            WHEN OLD.file_id='{file}' BEGIN SELECT RAISE(ABORT,'fixture cleanup failure'); END;"
            ))?;
            Ok(())
        })
        .unwrap();
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    assert!(!exists(&h, target));
    assert!(
        !final_path.exists() && !temp_path.exists(),
        "actual unlink ran before the durable cleanup acknowledgment failed"
    );
    assert!(source_path.exists() && has_certificate(&h, job.id));
    assert_eq!(work_count(&h, job.id), 1);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.h.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER retained_cleanup_failure")?;
            Ok(())
        })
        .unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(work_count(&h, job.id), 0);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.close().await;
}
