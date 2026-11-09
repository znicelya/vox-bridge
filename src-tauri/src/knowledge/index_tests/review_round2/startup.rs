use super::*;

async fn queued_cleaned_crash(temporary: bool) {
    let (h, good, target, imports) = native_target(1).await;
    let source = generation(&h).await;
    let original = survivor_bits(&h, &source);
    let (_, final_path, temporary_path) = original_file(&h, imports[0].id);
    let first = cancelled_delete(&h, target).await;
    let serial = h.service.core.serial.lock().await;
    let mut occupied = Vec::new();
    for label in ["occupied worker one", "occupied worker two"] {
        let blocker = h.import_text(label, label);
        let job = h
            .service
            .reindex_owned(blocker, Arc::new(()))
            .await
            .unwrap();
        h.wait_running(job.id).await;
        occupied.push(job.id);
    }
    let queued = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(
        h.h.store.get_job(queued.id).unwrap().unwrap().status,
        JobStatus::Queued
    );
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service
        .core
        .rebuild_generation(source.clone(), Arc::new(()))
        .await
        .unwrap();
    assert!(!final_path.exists() && !temporary_path.exists());
    assert!(!generation_path(&h, &source).exists());
    let certificate =
        h.h.store
            .transaction(|tx| completed::load(tx, queued.id))
            .unwrap()
            .unwrap();
    assert!(certificate.files_cleaned && certificate.acknowledged && !certificate.consumed);
    let publication =
        h.h.store
            .transaction(|tx| completed::read_publication(tx, certificate.header.publication))
            .unwrap();
    assert_eq!((publication.count, publication.batches), (2, 1));
    assert!(publication.source_removed);
    assert_eq!(
        (work_count(&h, first.id), work_count(&h, queued.id)),
        (0, 0)
    );
    assert_eq!(
        h.h.store.get_job(queued.id).unwrap().unwrap().status,
        JobStatus::Queued
    );
    // Join the actual index owner before the crash snapshot so its final clean
    // files and seal agree. Both real job workers still wait behind serial.
    h.index.shutdown().await.unwrap();
    let snapshot = h.h.temp_dir.path().join("round2-crash-snapshot");
    std::fs::create_dir(&snapshot).unwrap();
    let connection =
        rusqlite::Connection::open(h.h.temp_dir.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute(
            "VACUUM main INTO ?1",
            [snapshot.join("vox-bridge.sqlite").to_str().unwrap()],
        )
        .unwrap();
    drop(connection);
    let keys = journal_keys(&h.h.store);
    // Snapshotting the whole genuine database preserves Queued and the real
    // receipts without forging flags/status. Graceful teardown affects only
    // the original database, never the independent crash snapshot.
    h.h.jobs.request_shutdown().unwrap();
    drop(serial);
    h.h.jobs.shutdown().await.unwrap();
    for id in occupied {
        assert_eq!(
            h.h.store.get_job(id).unwrap().unwrap().status,
            JobStatus::Cancelled
        );
    }
    let recreated = if temporary {
        temporary_path
    } else {
        final_path
    };
    std::fs::write(&recreated, b"recreated certified original bytes").unwrap();
    let store = Arc::new(crate::storage::LocalStore::open(&snapshot).unwrap());
    let restarted = Reopened::new(&h, store);
    assert_eq!(
        restarted.store.get_job(queued.id).unwrap().unwrap().status,
        JobStatus::Queued
    );
    // Match application composition: register fresh handlers, then run the
    // actual scheduler startup reconciliation before owned knowledge work.
    restarted.jobs.reconcile().unwrap();
    assert_eq!(
        restarted.store.get_job(queued.id).unwrap().unwrap().status,
        JobStatus::Interrupted
    );
    let after_startup = journal_keys(&restarted.store);
    let full_journal_retained = keys.iter().all(|key| after_startup.contains(key));
    restarted.index.start_owned(Arc::new(())).await.unwrap();
    let rejected = restarted.service.reconcile_owned(Arc::new(())).await;
    let after_rejection = journal_keys(&restarted.store);
    let bytes = std::fs::read(&recreated).unwrap();
    std::fs::remove_file(&recreated).unwrap();
    restarted
        .service
        .reconcile_owned(Arc::new(()))
        .await
        .unwrap();
    let collected = journal_keys(&restarted.store).is_empty();
    let replacement = current_generation(&restarted.store);
    let actual_bits = survivor_bits(&h, &replacement);
    let actual_calls = h.embeddings.calls.load(Ordering::SeqCst);
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
    restarted.close().await;
    assert!(
        full_journal_retained,
        "startup terminal consumption cannot collect a genuine prior file receipt while a certified path exists; before={keys:?}, after={after_startup:?}"
    );
    assert!(
        matches!(
            rejected,
            Err(AppError::Intelligence(
                IntelligenceErrorCode::IndexRepairRequired
            ))
        ),
        "owned cleanup must reject recreated certified bytes: {rejected:?}"
    );
    assert!(keys.iter().all(|key| after_rejection.contains(key)));
    assert_eq!(bytes, b"recreated certified original bytes");
    assert!(
        collected,
        "the complete closed cohort must collect after a fresh owned absence check"
    );
    assert_eq!(actual_bits, original);
    assert_eq!(actual_calls, calls);
}

#[tokio::test]
async fn knowledge_review_round2_queued_startup_retains_recreated_final_path() {
    queued_cleaned_crash(false).await;
}

#[tokio::test]
async fn knowledge_review_round2_queued_startup_retains_recreated_temporary_path() {
    queued_cleaned_crash(true).await;
}

#[tokio::test]
async fn knowledge_review_round2_successful_direct_collects_locally_with_active_source() {
    let (h, good, target, imports) = native_target(1).await;
    let source = generation(&h).await;
    let original = survivor_bits(&h, &source);
    let (_, final_path, temporary_path) = original_file(&h, imports[0].id);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
    assert!(!final_path.exists() && !temporary_path.exists());
    let serial = h.service.core.serial.lock().await;
    h.service
        .core
        .resume_completions(Arc::new(()))
        .await
        .unwrap();
    drop(serial);
    assert!(
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .is_none()
    );
    assert!(
        generation_path(&h, &source).exists(),
        "a successful Direct Delete can collect after owned file checks while its source remains active"
    );
    assert_eq!(work_count(&h, job.id), 0);
    assert_eq!(survivor_bits(&h, &source), original);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.close().await;
}
