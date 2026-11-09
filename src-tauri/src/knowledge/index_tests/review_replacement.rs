use super::review_deletion::{exists, generation, generation_path, work_count};
use super::*;
mod authority;
mod cohorts;
mod files;
mod flags;
mod lifetime;
mod settlement;

fn axis(position: usize) -> Vec<f32> {
    let mut vector = vec![0.0; 128];
    vector[position] = 1.0;
    vector
}
fn queue_axis(h: &KnowledgeHarness, count: u32, position: usize) {
    for _ in 0..count {
        h.embeddings
            .vectors
            .lock()
            .unwrap()
            .push_back(axis(position))
    }
}

fn duplicate_attempts(h: &KnowledgeHarness, first: Uuid, copies: u32) -> Vec<Uuid> {
    h.h.store.transaction(|tx| {
        let (raw, document, exclusion, created): (String, String, String, i64) = tx.query_row(
            "SELECT payload_json,object_id,exclusion_key,created_at_ms FROM intelligence_jobs WHERE id=?1",
            [first.to_string()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let mut out = vec![first];
        for offset in 1..=copies {
            let id = Uuid::new_v4();
            let mut payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
            payload["input"]["operation_id"] = serde_json::json!(id.to_string());
            tx.execute(
                "INSERT INTO intelligence_jobs(id,kind,status,object_id,exclusion_key,payload_json,\
                 error_code,created_at_ms,updated_at_ms) VALUES (?1,'delete_document','failed',\
                 ?2,?3,?4,'index_repair_required',?5,?5)",
                rusqlite::params![id.to_string(), document, exclusion, payload.to_string(), created + i64::from(offset)],
            )?;
            tx.execute(
                "INSERT INTO knowledge_index_work_items(operation_id,ordinal,identity,generation,vector_id,\
                 document_id,document_version,chunk_id,content_hash,document_fence_json,location_sha256,previous_vector_id) \
                 SELECT ?1,ordinal,identity,generation,vector_id,document_id,document_version,chunk_id,content_hash,\
                 document_fence_json,location_sha256,previous_vector_id FROM knowledge_index_work_items WHERE operation_id=?2",
                rusqlite::params![id.to_string(), first.to_string()],
            )?;
            out.push(id);
        }
        Ok(out)
    }).unwrap()
}

#[tokio::test]
async fn knowledge_review_recovery_completes_more_than_200_retained_duplicate_operations() {
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("target", "24V");
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let good = h.import_text("neighbor", "48V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Failed);
    let attempts = duplicate_attempts(&h, job.id, 201);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    let recovered = h.service.reconcile_owned(Arc::new(())).await;
    // A failed observer must not drop this fixture's files while actual local
    // recovery still owns them. Join the existing serial owner before asserting.
    if recovered.is_err() {
        let joined = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            h.service.core.serial.lock(),
        )
        .await
        .unwrap();
        drop(joined);
    }
    recovered.unwrap();
    assert!(!exists(&h, target));
    assert!(exists(&h, good));
    for id in attempts {
        assert_eq!(
            work_count(&h, id),
            0,
            "every retained operation must be acknowledged"
        )
    }
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.embeddings.vectors.lock().unwrap().push_back(axis(1));
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.index.shutdown().await.unwrap();
}

#[tokio::test]
async fn knowledge_review_managed_metadata_loss_keeps_bytes_and_settlement_until_actual_cleanup() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let file = Uuid::new_v4();
    let name = format!("{file}.txt");
    let directory = h.h.temp_dir.path().join("knowledge").join("documents");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(&name);
    std::fs::write(&path, b"retained customer product source").unwrap();
    h.h.store.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_managed_files(file_id,relative_name,document_id,document_version,state,created_at_ms) \
            VALUES (?1,?2,?3,1,'retained',1)", rusqlite::params![file.to_string(), name, target.to_string()])?;
        Ok(())
    }).unwrap();
    let store = h.h.store.clone();
    h.service
        .at_checkpoint(CommitPoint::AfterDeletePublication, move || {
            store
                .transaction(|tx| {
                    tx.execute(
                        "DELETE FROM knowledge_managed_files WHERE file_id=?1",
                        [file.to_string()],
                    )?;
                    Ok(())
                })
                .unwrap();
        });
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(
        h.wait(job.id).await.status,
        JobStatus::Failed,
        "row absence is not actual unlink completion"
    );
    assert!(!exists(&h, target));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"retained customer product source"
    );
    assert_eq!(work_count(&h, job.id), 1);
    assert!(source_path.exists());
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    assert!(h.service.reconcile_owned(Arc::new(())).await.is_err());
    assert!(path.exists() && source_path.exists());
    h.h.store.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_managed_files(file_id,relative_name,document_version,state,created_at_ms) \
            VALUES (?1,?2,1,'delete_pending',1)", rusqlite::params![file.to_string(), name])?;
        Ok(())
    }).unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!path.exists());
    assert_eq!(work_count(&h, job.id), 0);
    // A settled terminal Failed operation must not pin this still-active source
    // forever when a later complete rebuild retires it.
    h.service.rebuild_owned(Arc::new(())).await.unwrap();
    assert!(!source_path.exists());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn knowledge_review_automatic_rebuild_completes_delete_admitted_before_later_source_page() {
    let h = KnowledgeHarness::new().await;
    let good = super::publication::document_chunks(&h, 241);
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("later page target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let removed = super::publication::document_chunks(&h, 300);
    queue_axis(&h, 300, 2);
    assert_eq!(h.index(removed).await.status, JobStatus::Succeeded);
    let removed_job = h.service.delete_owned(removed, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(removed_job.id).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let path = generation_path(&h, &source);
    let before = std::fs::read(path.join("vectors.bin")).unwrap();
    let bits = before[24..24 + 241 * 128 * 4].to_vec();
    let entered = Arc::new(tokio::sync::Notify::new());
    let signal = entered.clone();
    let (release, waiting) = std::sync::mpsc::channel();
    h.service
        .at_checkpoint(CommitPoint::BeforeLocalCopy, move || {
            signal.notify_one();
            waiting
                .recv_timeout(std::time::Duration::from_secs(15))
                .unwrap();
        });
    let added = h.import_text("new consumer", "36V");
    let reindex = h.service.reindex_owned(added, Arc::new(())).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(15), entered.notified())
        .await
        .unwrap();
    let delete = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert!(matches!(
        h.h.store.get_job(delete.id).unwrap().unwrap().status,
        JobStatus::Queued | JobStatus::Running
    ));
    assert_eq!(work_count(&h, delete.id), 1);
    release.send(()).unwrap();
    assert_eq!(h.wait(reindex.id).await.status, JobStatus::Succeeded);
    assert_eq!(h.wait(delete.id).await.status, JobStatus::Succeeded);
    assert!(!exists(&h, target));
    assert!(exists(&h, good));
    assert_eq!(work_count(&h, delete.id), 0);
    assert!(!path.exists());
    let replacement = generation(&h).await;
    let after = std::fs::read(generation_path(&h, &replacement).join("vectors.bin")).unwrap();
    assert_eq!(&after[24..24 + 241 * 128 * 4], bits.as_slice());
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert!(!exists(&h, target));
    h.index.shutdown().await.unwrap();
}

async fn two_retained_deletes(h: &KnowledgeHarness) -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
    let first = h.import_text("first target", "24V");
    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
    let second = h.import_text("second target", "36V");
    queue_axis(h, 1, 1);
    assert_eq!(h.index(second).await.status, JobStatus::Succeeded);
    let good = h.import_text("neighbor", "48V");
    queue_axis(h, 1, 2);
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let serial = h.service.core.serial.lock().await;
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    let a = h.service.delete_owned(first, Arc::new(())).await.unwrap();
    let b = h.service.delete_owned(second, Arc::new(())).await.unwrap();
    drop(serial);
    assert_eq!(h.wait(a.id).await.status, JobStatus::Failed);
    assert_eq!(h.wait(b.id).await.status, JobStatus::Failed);
    assert_eq!(work_count(h, a.id), 1);
    assert_eq!(work_count(h, b.id), 1);
    (first, second, good, a.id, b.id)
}
async fn recover_two(crash_after_pointer: bool) {
    let h = KnowledgeHarness::new().await;
    let (first, second, good, a, b) = two_retained_deletes(&h).await;
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let key = crate::knowledge::identity::EmbeddingIdentity::from_profile(&profile, 128)
        .unwrap()
        .key()
        .unwrap();
    let old: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT active_generation FROM knowledge_index_generations WHERE identity=?1",
                    [&key],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    let source = h.h.temp_dir.path().join("indexes").join(&key).join(&old);
    let before = std::fs::read(source.join("vectors.bin")).unwrap();
    let bits = before[24 + 2 * 128 * 4..24 + 3 * 128 * 4].to_vec();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    if crash_after_pointer {
        h.service.crash_at(CommitPoint::AfterPointerPublication);
        assert!(h.service.reconcile_owned(Arc::new(())).await.is_err());
        assert!(
            !exists(&h, first) && !exists(&h, second),
            "publication must complete the entire source cohort"
        );
        assert!(source.exists());
        assert_eq!((work_count(&h, a), work_count(&h, b)), (1, 1));
        h.index.shutdown().await.unwrap();
        h.index.start_owned(Arc::new(())).await.unwrap();
    }
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!exists(&h, first) && !exists(&h, second));
    assert!(exists(&h, good));
    assert!(!source.exists());
    assert_eq!((work_count(&h, a), work_count(&h, b)), (0, 0));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    let replacement = generation(&h).await;
    let after = std::fs::read(generation_path(&h, &replacement).join("vectors.bin")).unwrap();
    assert_eq!(&after[24..24 + 128 * 4], bits.as_slice());
    h.embeddings.vectors.lock().unwrap().push_back(axis(2));
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.index.shutdown().await.unwrap();
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!((work_count(&h, a), work_count(&h, b)), (0, 0));
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_review_recovery_completes_two_retained_same_source_deletes() {
    recover_two(false).await;
}
#[tokio::test]
async fn knowledge_review_recovery_resumes_both_document_gone_work_sets_after_pointer_crash() {
    recover_two(true).await;
}

#[tokio::test]
async fn knowledge_review_fresh_swap_rejects_unstaged_native_target_record() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let serial = h.service.core.serial.lock().await;
    let ticket = h.index.open_existing(source.clone(), Arc::new(())).unwrap();
    h.index
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let records = h.index.scan_records(&ticket, 0, 200).await.unwrap();
    let receipt = h.index.persist(&ticket).await.unwrap();
    let maintenance = h.index.begin_maintenance(&ticket).await.unwrap();
    let vectors = h.index.copy_vectors(&maintenance, 0, 200).await.unwrap();
    let destination = crate::knowledge::index::Generation {
        generation: Uuid::new_v4(),
        ..source.clone()
    };
    let staging = h.index.create(destination, Arc::new(())).unwrap();
    h.index
        .enqueue_open(&staging)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let metadata = records[0].metadata.as_ref().unwrap();
    let fence =
        h.h.store
            .transaction(|tx| crate::knowledge::service::state::fence(tx, good))
            .unwrap();
    let id = h
        .index
        .add(&staging, vec![(vectors[0].1.clone(), metadata.clone())])
        .await
        .unwrap()[0];
    h.service
        .core
        .stage_rebuild(&staging, 0, metadata, &fence, 0, id)
        .unwrap();
    // A real native record without matching staged evidence cannot be hidden by
    // SQL-only target absence or by the caller's survivor count.
    h.index
        .add(
            &staging,
            vec![(vectors[1].1.clone(), records[1].metadata.clone().unwrap())],
        )
        .await
        .unwrap();
    h.index.end_maintenance(maintenance).await.unwrap();
    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
    h.index.acknowledge(&ticket, ack).await.unwrap();
    let deletion = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    let persisted = h.index.persist(&staging).await.unwrap();
    let result = h
        .index
        .publish_plan(
            &staging,
            &persisted,
            crate::knowledge::index::PublicationPlan::Swap {
                operation: staging.operation_id(),
                work: staging.operation_id(),
                source: source.generation,
                count: 1,
                cleanup_directory: crate::knowledge::index::delete_completion::resolve_directory(
                    h.h.temp_dir.path().to_path_buf(),
                    Arc::new(()),
                )
                .await
                .unwrap(),
            },
        )
        .await;
    assert!(
        result.is_err(),
        "the actual owner receipt contains an unstaged live target"
    );
    let active: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT active_generation FROM knowledge_index_generations WHERE identity=?1",
                    [source.key().unwrap()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(active, source.generation.to_string());
    assert!(exists(&h, target));
    assert_eq!(work_count(&h, deletion.id), 1);
    assert!(generation_path(&h, &source).exists());
    drop(serial);
    assert_eq!(h.wait(deletion.id).await.status, JobStatus::Succeeded);
    h.close().await;
}
