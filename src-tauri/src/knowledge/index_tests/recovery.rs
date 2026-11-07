use super::*;

async fn interrupted_delete(h: &KnowledgeHarness, id: Uuid) -> Uuid {
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    let job = h.service.delete_owned(id, Arc::new(())).await.unwrap();
    assert_ne!(h.wait(job.id).await.status, JobStatus::Succeeded);
    job.id
}
fn pointer(h: &KnowledgeHarness) -> (String, String) {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT identity,active_generation FROM \
                    knowledge_index_generations WHERE \
                    active_generation IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap()
}
fn exists(h: &KnowledgeHarness, id: Uuid) -> bool {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                [id.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}
#[tokio::test]
async fn knowledge_dirty_delete_recovers_survivor_bits_locally_and_reopen_is_idempotent() {
    let h = KnowledgeHarness::new().await;
    let deleted = h.import_text("delete", "24V");
    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
    let survivor = h.import_text("survive", "48V");
    assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
    let (key, old) = pointer(&h);
    let source = h.h.temp_dir.path().join("indexes").join(&key).join(&old);
    let before = std::fs::read(source.join("vectors.bin")).unwrap();
    let survivor_bits = before[24 + 128 * 4..24 + 2 * 128 * 4].to_vec();
    let job = interrupted_delete(&h, deleted).await;
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(
        !exists(&h, deleted),
        "witnessed deletion must finish locally"
    );
    let (_, new) = pointer(&h);
    assert_ne!(new, old);
    assert!(!source.exists());
    let after = std::fs::read(
        h.h.temp_dir
            .path()
            .join("indexes")
            .join(&key)
            .join(new)
            .join("vectors.bin"),
    )
    .unwrap();
    assert_eq!(&after[24..24 + 128 * 4], &survivor_bits);
    let work: u32 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
                    [job.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(work, 0);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, survivor);
    h.index.shutdown().await.unwrap();
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!exists(&h, deleted));
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_dirty_delete_last_document_publishes_empty_destination() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("last", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let (_, old) = pointer(&h);
    interrupted_delete(&h, id).await;
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!exists(&h, id));
    assert_ne!(pointer(&h).1, old);
    assert!(h.search("24V").await.unwrap().hits.is_empty());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_dirty_delete_changed_vector_witness_preserves_bytes_and_deleting_work() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("tamper", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let (key, old) = pointer(&h);
    let path =
        h.h.temp_dir
            .path()
            .join("indexes")
            .join(key)
            .join(&old)
            .join("vectors.bin");
    let job = interrupted_delete(&h, id).await;
    h.close().await;
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[24] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(exists(&h, id));
    assert_eq!(pointer(&h).1, old);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    let (status, work): (String, u32) =
        h.h.store
            .transaction(|tx| {
                Ok((
                    tx.query_row(
                        "SELECT status FROM documents WHERE id=?1",
                        [id.to_string()],
                        |r| r.get(0),
                    )?,
                    tx.query_row(
                        "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
                        [job.to_string()],
                        |r| r.get(0),
                    )?,
                ))
            })
            .unwrap();
    assert_eq!(status, "deleting");
    assert_eq!(work, 1);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_dirty_delete_destination_persist_crash_keeps_source_and_retry_finishes() {
    let h = KnowledgeHarness::new().await;
    let deleted = h.import_text("delete", "24V");
    let survivor = h.import_text("survive", "48V");
    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
    let (key, old) = pointer(&h);
    let source = h.h.temp_dir.path().join("indexes").join(&key).join(&old);
    interrupted_delete(&h, deleted).await;
    h.close().await;
    let bytes = std::fs::read(source.join("vectors.bin")).unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(pointer(&h).1, old);
    assert!(exists(&h, deleted));
    assert_eq!(std::fs::read(source.join("vectors.bin")).unwrap(), bytes);
    h.index.shutdown().await.unwrap();
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!exists(&h, deleted));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
    let count = std::fs::read_dir(h.h.temp_dir.path().join("indexes").join(key))
        .unwrap()
        .count();
    assert_eq!(
        count, 1,
        "abandoned destination and retired source are acknowledged locally"
    );
}
#[tokio::test]
async fn knowledge_dirty_delete_pointer_crash_resumes_retirement_and_work_ack() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("last", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let (key, old) = pointer(&h);
    let source = h.h.temp_dir.path().join("indexes").join(key).join(&old);
    let job = interrupted_delete(&h, id).await;
    h.close().await;
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.crash_at(CommitPoint::AfterPointerPublication);
    assert!(h.service.reconcile_owned(Arc::new(())).await.is_err());
    assert!(!exists(&h, id));
    assert_ne!(pointer(&h).1, old);
    assert!(source.exists());
    h.index.shutdown().await.unwrap();
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(
        !source.exists(),
        "retired source cleanup must resume after pointer publication"
    );
    let count: u32 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
                    [job.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(count, 0);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_dirty_delete_reader_source_lock_is_exclusive_and_release_is_owned() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("lock", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let (key, old) = pointer(&h);
    interrupted_delete(&h, id).await;
    let evidence = h
        .index
        .recovery_evidence(&key, Uuid::parse_str(&old).unwrap())
        .unwrap();
    let target = h
        .index
        .create(
            crate::knowledge::index::Generation {
                generation: Uuid::new_v4(),
                ..evidence.source().clone()
            },
            Arc::new(()),
        )
        .unwrap();
    assert!(
        matches!(
            h.index.begin_recovery(&target, evidence.clone()).await,
            Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
        ),
        "live engine retains source lock"
    );
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    let target = h
        .index
        .create(
            crate::knowledge::index::Generation {
                generation: Uuid::new_v4(),
                ..evidence.source().clone()
            },
            Arc::new(()),
        )
        .unwrap();
    let reader = h.index.begin_recovery(&target, evidence).await.unwrap();
    let path =
        h.h.temp_dir
            .path()
            .join("indexes")
            .join(key)
            .join(old)
            .join("srvdb.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(h.index.copy_recovery(&reader, 0, 201).await.is_err());
    let copied = h.index.copy_recovery(&reader, 0, 200).await.unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].0, 0);
    h.index.end_recovery(reader).await.unwrap();
    assert!(lock.try_lock().is_ok());
    drop(lock);
    h.index.shutdown().await.unwrap();
}

#[tokio::test]
async fn knowledge_local_copy_freezes_recovery_associations_before_vector_copy() {
    for mutation in ["sha", "parser", "import", "location", "mapping"] {
        let h = KnowledgeHarness::new().await;
        let deleted = h.import_text("delete", "24V");
        let survivor = h.import_text("survive", "48V");
        assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
        assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
        let (_, old) = pointer(&h);
        interrupted_delete(&h, deleted).await;
        h.close().await;
        h.index.start_owned(Arc::new(())).await.unwrap();
        let store = h.h.store.clone();
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        h.service
            .at_checkpoint(CommitPoint::BeforeLocalCopy, move || {
                super::publication::reassociate(&store, survivor, mutation)
            });
        h.service.reconcile_owned(Arc::new(())).await.unwrap();
        assert!(
            exists(&h, deleted),
            "pre-copy {mutation} association must reject deletion publication"
        );
        assert_eq!(pointer(&h).1, old, "{mutation}");
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        h.index.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn knowledge_dirty_delete_rejects_unrecognized_or_symlink_source_entries_without_opening_them()
 {
    for entry in ["unexpected", "symlink"] {
        let h = KnowledgeHarness::new().await;
        let id = h.import_text("source", "24V");
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        let (key, old) = pointer(&h);
        interrupted_delete(&h, id).await;
        h.close().await;
        let source = h.h.temp_dir.path().join("indexes").join(key).join(&old);
        if entry == "unexpected" {
            std::fs::write(source.join("unrecognized"), b"preserve").unwrap();
        } else {
            std::fs::remove_file(source.join("metadata.db")).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink("vectors.bin", source.join("metadata.db")).unwrap();
        }
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        h.index.start_owned(Arc::new(())).await.unwrap();
        h.service.reconcile_owned(Arc::new(())).await.unwrap();
        assert!(exists(&h, id), "{entry}");
        assert_eq!(pointer(&h).1, old);
        assert!(source.exists());
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        h.index.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn knowledge_delete_rejects_inconsistent_payload_document_fence() {
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("target", "24V");
    let foreign = h.import_text("foreign", "48V");
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET status='deleting' WHERE id IN (?1,?2)",
                rusqlite::params![target.to_string(), foreign.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let document =
        h.h.store
            .transaction(|tx| crate::knowledge::service::state::fence(tx, foreign))
            .unwrap();
    let input = crate::knowledge::service::DeleteInput {
        document_id: target,
        document_version: 1,
        document,
        operation_id: Uuid::new_v4(),
        identity: None,
        generation: None,
    };
    assert!(
        h.service
            .core
            .delete(&input, Arc::new(()), None)
            .await
            .is_err()
    );
    assert!(exists(&h, target));
    assert!(exists(&h, foreign));
    h.close().await;
}

#[tokio::test]
async fn knowledge_delete_resume_advances_past_full_page_of_invalid_payload_associations() {
    let h = KnowledgeHarness::new().await;
    for number in 1..=201u128 {
        let document_id = Uuid::from_u128(number);
        let job = Uuid::new_v4();
        let document = crate::knowledge::index::DocumentFence {
            id: document_id,
            version: 1,
            sha256: "a".repeat(64),
            parser: "fixture".into(),
            import_job: None,
        };
        let input = crate::knowledge::service::DeleteInput {
            document_id,
            document_version: if number == 201 { 1 } else { 2 },
            document,
            operation_id: job,
            identity: None,
            generation: None,
        };
        h.h.store.transaction(|tx| {
            tx.execute("INSERT INTO documents(id,sha256,uri,title,created_at,origin,kind,status,parser_identity) VALUES (?1,?2,'','pending',1,'native','text','deleting','fixture')",rusqlite::params![document_id.to_string(),"a".repeat(64)])?;
            crate::intelligence::store::admit_job(tx,crate::intelligence::store::JobDraft { id:job,kind:JobKind::DeleteDocument,session_id:None,object_id:Some(document_id),admission:JobAdmission::Manual{exclusion_key:format!("document:{document_id}")},payload:serde_json::json!({"version":1,"profiles":[],"input":input}),initial_failure:None })?;
            tx.execute("UPDATE intelligence_jobs SET status='succeeded' WHERE id=?1",[job.to_string()])?;Ok(())
        }).unwrap();
    }
    h.service.core.resume_deletes(Arc::new(())).await.unwrap();
    assert!(
        !exists(&h, Uuid::from_u128(201)),
        "invalid retained associations must not block later valid deletion pages"
    );
    assert!(exists(&h, Uuid::from_u128(1)));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}
