use super::*;
use crate::knowledge::index::delete_completion as completed;
use std::path::PathBuf;

pub(super) fn operation_key(id: Uuid) -> String {
    format!("knowledge.delete.v1.operation.{id}")
}
pub(super) fn has_certificate(h: &KnowledgeHarness, id: Uuid) -> bool {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM intelligence_meta WHERE key=?1)",
                [operation_key(id)],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}
pub(super) fn managed_file(h: &KnowledgeHarness, target: Uuid) -> (Uuid, PathBuf, PathBuf) {
    let id = Uuid::new_v4();
    let directory = h.h.temp_dir.path().join("knowledge").join("documents");
    std::fs::create_dir_all(&directory).unwrap();
    let final_path = directory.join(format!("{id}.txt"));
    let temp_path = directory.join(format!("{id}.tmp"));
    std::fs::write(&final_path, b"original final bytes").unwrap();
    std::fs::write(&temp_path, b"original temporary bytes").unwrap();
    h.h.store.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_managed_files(file_id,relative_name,document_id,document_version,state,created_at_ms) \
            VALUES (?1,?2,?3,1,'retained',1)", rusqlite::params![id.to_string(), format!("{id}.txt"), target.to_string()])?;
        Ok(())
    }).unwrap();
    (id, final_path, temp_path)
}
fn restore_file(h: &KnowledgeHarness, id: Uuid) {
    h.h.store.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_managed_files(file_id,relative_name,document_version,state,created_at_ms) \
            VALUES (?1,?2,1,'delete_pending',1)", rusqlite::params![id.to_string(), format!("{id}.txt")])?;
        Ok(())
    }).unwrap();
}

#[tokio::test]
async fn knowledge_review_files_cleaned_flag_cannot_hide_retained_final_or_temp_bytes() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let (file, final_path, temp_path) = managed_file(&h, target);
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    let original =
        h.h.store
            .transaction(|tx| Ok(completed::load(tx, job.id)?.unwrap()))
            .unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_managed_files WHERE file_id=?1",
                [file.to_string()],
            )?;
            let mut changed = original.clone();
            changed.files_cleaned = true;
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![
                    operation_key(job.id),
                    serde_json::to_string(&changed).unwrap()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    assert!(
        h.service.reconcile_owned(Arc::new(())).await.is_err(),
        "a replaced cleanup flag is not an unlink receipt"
    );
    assert_eq!(std::fs::read(&final_path).unwrap(), b"original final bytes");
    assert_eq!(
        std::fs::read(&temp_path).unwrap(),
        b"original temporary bytes"
    );
    assert!(source_path.exists());
    assert_eq!(work_count(&h, job.id), 1);
    assert!(has_certificate(&h, job.id));
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![
                    operation_key(job.id),
                    serde_json::to_string(&original).unwrap()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    restore_file(&h, file);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!final_path.exists() && !temp_path.exists());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_zero_work_terminal_consumption_requires_independent_actual_file_cleanup()
{
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("parsed target", "24V");
    let (file, final_path, temp_path) = managed_file(&h, target);
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    assert_eq!(work_count(&h, job.id), 0);
    let original =
        h.h.store
            .transaction(|tx| Ok(completed::load(tx, job.id)?.unwrap()))
            .unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_managed_files WHERE file_id=?1",
                [file.to_string()],
            )?;
            let mut changed = original.clone();
            changed.files_cleaned = true;
            changed.acknowledged = true;
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![
                    operation_key(job.id),
                    serde_json::to_string(&changed).unwrap()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    // This is the production domain hook used inside the scheduler's terminal
    // transaction. It may retain invalid proof or report its error, but cannot
    // consume fabricated cleanup while the original generated paths survive.
    let _ = h.h.store.transaction(|tx| {
        completed::consume(tx, job.id, JobStatus::Failed, false, &h.h.jobs.reap_port())
    });
    assert!(
        has_certificate(&h, job.id),
        "terminal SQL state alone cannot collect unfinished physical cleanup"
    );
    assert_eq!(std::fs::read(&final_path).unwrap(), b"original final bytes");
    assert_eq!(
        std::fs::read(&temp_path).unwrap(),
        b"original temporary bytes"
    );
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![
                    operation_key(job.id),
                    serde_json::to_string(&original).unwrap()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    restore_file(&h, file);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!final_path.exists() && !temp_path.exists());
    assert!(!has_certificate(&h, job.id));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_completed_file_paths_are_rechecked_before_source_retirement() {
    for surviving_path in ["final", "temp"] {
        let h = KnowledgeHarness::new().await;
        let good = h.import_text("neighbor", "48V");
        assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
        let target = h.import_text("target", "24V");
        queue_axis(&h, 1, 1);
        assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
        let source = generation(&h).await;
        let source_path = generation_path(&h, &source);
        let (_, final_path, temp_path) = managed_file(&h, target);
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
        assert!(!final_path.exists() && !temp_path.exists());
        let certificate =
            h.h.store
                .transaction(|tx| Ok(completed::load(tx, job.id)?.unwrap()))
                .unwrap();
        assert!(certificate.files_cleaned && certificate.acknowledged && certificate.consumed);
        let path = if surviving_path == "final" {
            &final_path
        } else {
            &temp_path
        };
        std::fs::write(path, b"bytes surviving a prior cleanup marker").unwrap();
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        assert!(
            h.service.rebuild_owned(Arc::new(())).await.is_err(),
            "the original {surviving_path} path must be absent before retirement"
        );
        assert_eq!(
            std::fs::read(path).unwrap(),
            b"bytes surviving a prior cleanup marker"
        );
        assert!(source_path.exists());
        assert!(has_certificate(&h, job.id));
        std::fs::remove_file(path).unwrap();
        h.service.reconcile_owned(Arc::new(())).await.unwrap();
        assert!(!source_path.exists());
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        h.close().await;
    }
}
