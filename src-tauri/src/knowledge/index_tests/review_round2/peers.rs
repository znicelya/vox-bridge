use super::*;

#[tokio::test]
async fn knowledge_review_round2_duplicate_padded_fence_finishes_direct_delete() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let original = survivor_bits(&h, &source);
    let first = cancelled_delete(&h, target).await;
    assert_eq!(work_count(&h, first.id), 1);
    pad_original_fence(&h, first.id);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let serial = h.service.core.serial.lock().await;
    let second = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_ne!(
        first.id, second.id,
        "both operations come from real admission"
    );
    assert_eq!(work_count(&h, second.id), 1);
    drop(serial);
    let finished = h.wait(second.id).await;
    let remaining = (work_count(&h, first.id), work_count(&h, second.id));
    if finished.status == JobStatus::Succeeded {
        assert!(!exists(&h, target));
        assert_eq!(remaining, (0, 0));
        let publication =
            h.h.store
                .transaction(|tx| {
                    let certificate = completed::load(tx, first.id)?.unwrap();
                    completed::read_publication(tx, certificate.header.publication)
                })
                .unwrap();
        assert_eq!((publication.count, publication.batches), (2, 1));
        assert_eq!(survivor_bits(&h, &source), original);
        h.service.rebuild_owned(Arc::new(())).await.unwrap();
        assert!(!generation_path(&h, &source).exists());
        let replacement = generation(&h).await;
        assert_eq!(survivor_bits(&h, &replacement), original);
        for operation in [first.id, second.id] {
            assert!(
                h.h.store
                    .transaction(|tx| completed::load(tx, operation))
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    }
    h.close().await;
    assert_eq!(
        finished.status,
        JobStatus::Succeeded,
        "valid padded duplicate must acknowledge the exact common mapping and both work sets; actual error={:?}, remaining={remaining:?}",
        finished.error
    );
}

#[tokio::test]
async fn knowledge_review_round2_duplicate_padded_fence_finishes_pointer_restart() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let original = survivor_bits(&h, &source);
    let first = cancelled_delete(&h, target).await;
    pad_original_fence(&h, first.id);
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    let second = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(h.wait(second.id).await.status, JobStatus::Failed);
    assert_eq!(
        (work_count(&h, first.id), work_count(&h, second.id)),
        (1, 1)
    );
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    let pointer = Reopened::new(&h, h.h.store.clone());
    pointer.jobs.reconcile().unwrap();
    pointer.index.start_owned(Arc::new(())).await.unwrap();
    pointer
        .service
        .crash_at(CommitPoint::AfterPointerPublication);
    assert!(pointer.service.reconcile_owned(Arc::new(())).await.is_err());
    assert!(!exists(&h, target));
    assert!(generation_path(&h, &source).exists());
    for operation in [first.id, second.id] {
        let certificate = pointer
            .store
            .transaction(|tx| completed::load(tx, operation))
            .unwrap()
            .unwrap();
        assert_eq!(certificate.header.input.generation, Some(source.generation));
        assert!(!certificate.acknowledged);
    }
    pointer.close().await;
    let restarted = Reopened::new(&h, h.h.store.clone());
    restarted.jobs.reconcile().unwrap();
    restarted.index.start_owned(Arc::new(())).await.unwrap();
    let finished = restarted.service.reconcile_owned(Arc::new(())).await;
    let remaining = (work_count(&h, first.id), work_count(&h, second.id));
    if finished.is_ok() {
        assert_eq!(remaining, (0, 0));
        assert!(!generation_path(&h, &source).exists());
        for operation in [first.id, second.id] {
            assert!(
                restarted
                    .store
                    .transaction(|tx| completed::load(tx, operation))
                    .unwrap()
                    .is_none()
            );
        }
        let replacement = current_generation(&restarted.store);
        assert_eq!(survivor_bits(&h, &replacement), original);
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        assert_eq!(
            restarted
                .service
                .search_owned(
                    KnowledgeSearch {
                        query: "48V".into(),
                        selected_segment_ids: vec![],
                        session_id: None,
                        limit: 5,
                    },
                    Arc::new(())
                )
                .await
                .unwrap()
                .hits[0]
                .document_id,
            good
        );
    }
    restarted.close().await;
    assert!(
        finished.is_ok(),
        "valid padded duplicates must finish exact acknowledgment after a genuine pointer/cleanup restart: {finished:?}, work={remaining:?}"
    );
}

#[tokio::test]
async fn knowledge_review_round2_raw_fence_drift_after_certificate_is_rejected() {
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("target", "24V");
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let first = cancelled_delete(&h, target).await;
    pad_original_fence(&h, first.id);
    h.service.crash_at(CommitPoint::AfterDeletePublication);
    let second = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(second.id).await.status, JobStatus::Failed);
    assert!(
        h.h.store
            .transaction(|tx| completed::load(tx, first.id))
            .unwrap()
            .is_some()
    );
    let original: String = h.h.store.transaction(|tx| Ok(tx.query_row(
        "SELECT document_fence_json FROM knowledge_index_work_items WHERE operation_id=?1 AND ordinal=0",
        [first.id.to_string()], |row| row.get(0),
    )?)).unwrap();
    h.h.store.transaction(|tx| {
        tx.execute("UPDATE knowledge_index_work_items SET document_fence_json=?2 WHERE operation_id=?1 AND ordinal=0",
            rusqlite::params![first.id.to_string(), original.trim_end()])?;
        Ok(())
    }).unwrap();
    let rejected = h.h.store.transaction(|tx| completed::load(tx, first.id));
    let recovery_rejected = h.service.reconcile_owned(Arc::new(())).await.is_err();
    let remaining = (work_count(&h, first.id), work_count(&h, second.id));
    h.h.store.transaction(|tx| {
        tx.execute("UPDATE knowledge_index_work_items SET document_fence_json=?2 WHERE operation_id=?1 AND ordinal=0",
            rusqlite::params![first.id.to_string(), original])?;
        Ok(())
    }).unwrap();
    h.close().await;
    assert!(
        matches!(
            rejected,
            Err(AppError::Intelligence(
                IntelligenceErrorCode::IndexRepairRequired
            ))
        ),
        "post-certificate raw evidence remains immutable even when its typed association is equal"
    );
    assert!(recovery_rejected);
    assert_eq!(remaining, (1, 1));
}
