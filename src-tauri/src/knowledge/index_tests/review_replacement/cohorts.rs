use super::flags::has_certificate;
use super::*;
use crate::knowledge::index::delete_completion as completed;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn knowledge_review_direct_delete_certifies_202_exact_terminal_peers_and_retains_history() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let original = std::fs::read(source_path.join("vectors.bin")).unwrap();
    let survivor = original[24..24 + 128 * 4].to_vec();
    let entered = Arc::new(tokio::sync::Notify::new());
    let signal = entered.clone();
    let (release, waiting) = std::sync::mpsc::channel();
    h.service
        .at_checkpoint(CommitPoint::AfterDeletePublication, move || {
            signal.notify_one();
            waiting
                .recv_timeout(std::time::Duration::from_secs(15))
                .unwrap();
        });
    let serial = h.service.core.serial.lock().await;
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    let attempts = duplicate_attempts(&h, job.id, 201);
    h.h.store
        .transaction(|tx| {
            for (index, id) in attempts.iter().enumerate().skip(1) {
                let (status, error) = [
                    ("failed", "index_repair_required"),
                    ("cancelled", "cancelled"),
                    ("interrupted", "interrupted"),
                ][index % 3];
                tx.execute(
                    "UPDATE intelligence_jobs SET status=?2,error_code=?3 WHERE id=?1",
                    rusqlite::params![id.to_string(), status, error],
                )?;
            }
            Ok(())
        })
        .unwrap();
    drop(serial);
    tokio::time::timeout(std::time::Duration::from_secs(15), entered.notified())
        .await
        .unwrap();
    let first =
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .unwrap();
    let publication =
        h.h.store
            .transaction(|tx| completed::read_publication(tx, first.header.publication))
            .unwrap();
    assert_eq!(
        publication.authority.kind,
        completed::CompletionKind::Delete
    );
    assert_eq!((publication.count, publication.batches), (202, 2));
    assert_eq!(publication.authority.source, Some(source.generation));
    assert!(!exists(&h, target) && source_path.exists());
    for id in &attempts {
        let certificate =
            h.h.store
                .transaction(|tx| completed::load(tx, *id))
                .unwrap()
                .unwrap();
        assert_eq!(certificate.header.publication, first.header.publication);
        assert_eq!(certificate.header.shared_root, first.header.shared_root);
        assert_eq!(
            certificate.header.input.document,
            first.header.input.document
        );
        assert_eq!(work_count(&h, *id), 1);
        assert!(!certificate.acknowledged && !certificate.consumed);
    }
    release.send(()).unwrap();
    let released_at = std::time::Instant::now();
    use futures_util::FutureExt;
    let finished = std::panic::AssertUnwindSafe(h.wait(job.id))
        .catch_unwind()
        .await;
    if finished.is_err() {
        let joined = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            h.service.core.serial.lock(),
        )
        .await
        .unwrap();
        drop(joined);
        eprintln!(
            "202-peer actual serial end: elapsed={:?}, job={:?}",
            released_at.elapsed(),
            h.h.store
                .get_job(job.id)
                .unwrap()
                .map(|value| (value.status, value.error))
        );
        h.close().await;
    }
    eprintln!(
        "202-peer terminal observation: elapsed={:?}",
        released_at.elapsed()
    );
    assert_eq!(finished.unwrap().status, JobStatus::Succeeded);
    assert!(
        h.h.store
            .transaction(|tx| h.h.jobs.reap_port().prove(tx, job.id))
            .unwrap()
            .is_some(),
        "terminal observation must include the actual completed handler and every issued child scope"
    );
    for id in &attempts {
        let certificate =
            h.h.store
                .transaction(|tx| completed::load(tx, *id))
                .unwrap()
                .unwrap();
        assert!(certificate.acknowledged && certificate.consumed);
        assert_eq!(work_count(&h, *id), 0);
    }
    assert!(
        source_path.exists(),
        "settled Failed/Cancelled/Interrupted peers retain the complete Direct Delete cohort on its active source"
    );
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.rebuild_owned(Arc::new(())).await.unwrap();
    assert!(!source_path.exists());
    for id in attempts {
        assert!(!has_certificate(&h, id))
    }
    let replacement = generation(&h).await;
    let after = std::fs::read(generation_path(&h, &replacement).join("vectors.bin")).unwrap();
    assert_eq!(&after[24..24 + 128 * 4], survivor.as_slice());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_fresh_swap_certifies_valid_source_bound_zero_work_member() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let before = std::fs::read(generation_path(&h, &source).join("vectors.bin")).unwrap();
    let target = h.import_text("source bound without vectors", "24V");
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET index_identity=?2 WHERE id=?1",
                rusqlite::params![target.to_string(), source.key().unwrap()],
            )?;
            Ok(())
        })
        .unwrap();
    let serial = h.service.core.serial.lock().await;
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(work_count(&h, job.id), 0);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service
        .core
        .rebuild_generation(source.clone(), Arc::new(()))
        .await
        .unwrap();
    let certificate =
        h.h.store
            .transaction(|tx| completed::load(tx, job.id))
            .unwrap()
            .unwrap();
    assert_eq!((certificate.header.count, certificate.header.pages), (0, 0));
    assert!(certificate.acknowledged && !certificate.consumed);
    assert!(!exists(&h, target));
    assert!(!generation_path(&h, &source).exists());
    drop(serial);
    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
    let serial = h.service.core.serial.lock().await;
    h.service
        .core
        .resume_completions(Arc::new(()))
        .await
        .unwrap();
    drop(serial);
    assert!(!has_certificate(&h, job.id));
    let replacement = generation(&h).await;
    let after = std::fs::read(generation_path(&h, &replacement).join("vectors.bin")).unwrap();
    assert_eq!(&after[24..24 + 128 * 4], &before[24..24 + 128 * 4]);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_fresh_ready_replacement_completes_retained_source_delete() {
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("target", "24V");
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    let deletion = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(deletion.id).await.status, JobStatus::Failed);
    assert_eq!(work_count(&h, deletion.id), 1);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.h.jobs.start().unwrap();
    // With the retained source dirty, explicit reindex creates a fresh Ready
    // destination from this document's embeddings instead of opening it.
    let good = h.import_text("fresh Ready document", "48V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let replacement = generation(&h).await;
    assert_ne!(replacement.generation, source.generation);
    assert!(!exists(&h, target));
    assert_eq!(work_count(&h, deletion.id), 0);
    assert!(!generation_path(&h, &source).exists());
    assert!(!has_certificate(&h, deletion.id));
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.embeddings.vectors.lock().unwrap().push_back(axis(1));
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_retention_authority_fresh_swap_rejects_different_source_target_peer() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let target = h.import_text("target", "24V");
    queue_axis(&h, 1, 1);
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
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
    let foreign_source = crate::knowledge::index::Generation {
        generation: Uuid::new_v4(),
        ..source.clone()
    };
    let foreign_path = generation_path(&h, &foreign_source);
    let foreign = h
        .index
        .create(foreign_source.clone(), Arc::new(()))
        .unwrap();
    h.index
        .enqueue_open(&foreign)
        .unwrap()
        .observe()
        .await
        .unwrap();
    for (id, vector) in vectors {
        let actual = h
            .index
            .add(
                &foreign,
                vec![(vector, records[id as usize].metadata.clone().unwrap())],
            )
            .await
            .unwrap()[0];
        assert_eq!(actual, id);
    }
    h.index.end_maintenance(maintenance).await.unwrap();
    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
    h.index.acknowledge(&ticket, ack).await.unwrap();
    let persisted = h.index.persist(&foreign).await.unwrap();
    let ack = h.index.publish(&foreign, &persisted).await.unwrap();
    h.index.acknowledge(&foreign, ack).await.unwrap();
    h.h.store.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_generation_mappings(identity,generation,vector_id,document_id,document_version,chunk_id,content_hash,published) \
            SELECT identity,?1,vector_id,document_id,document_version,chunk_id,content_hash,published \
            FROM knowledge_generation_mappings WHERE identity=?2 AND generation=?3 AND document_id=?4",
            rusqlite::params![foreign_source.generation.to_string(), source.key().unwrap(), source.generation.to_string(), target.to_string()])?;
        Ok(())
    }).unwrap();
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    let peer = duplicate_attempts(&h, job.id, 1)[1];
    h.h.store
        .transaction(|tx| {
            let raw: String = tx.query_row(
                "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                [peer.to_string()],
                |row| row.get(0),
            )?;
            let mut payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
            payload["input"]["generation"] =
                serde_json::json!(foreign_source.generation.to_string());
            tx.execute(
                "UPDATE intelligence_jobs SET payload_json=?2 WHERE id=?1",
                rusqlite::params![peer.to_string(), payload.to_string()],
            )?;
            tx.execute(
                "UPDATE knowledge_index_work_items SET generation=?2 WHERE operation_id=?1",
                rusqlite::params![peer.to_string(), foreign_source.generation.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let rebuilt = h
        .service
        .core
        .rebuild_generation(source.clone(), Arc::new(()))
        .await;
    let rejected = rebuilt.is_err();
    let retained_target = exists(&h, target);
    let retained_work = (work_count(&h, job.id), work_count(&h, peer));
    let retained_bytes = source_path.exists() && foreign_path.exists();
    let same_pointer: bool =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
        "SELECT active_generation=?2 FROM knowledge_index_generations WHERE identity=?1",
        rusqlite::params![source.key().unwrap(), source.generation.to_string()], |row| row.get(0),
    )?)
            })
            .unwrap();
    // Remove only the deliberately introduced peer before joining the genuine
    // queued handler, so a failing test never tears down entered owner work.
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
                [peer.to_string()],
            )?;
            tx.execute(
                "DELETE FROM intelligence_jobs WHERE id=?1",
                [peer.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    drop(serial);
    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
    h.close().await;
    assert!(
        rejected,
        "fresh replacement cannot certify one source and strand the same target's other source-bound operation"
    );
    assert!(retained_target && retained_bytes && same_pointer);
    assert_eq!(retained_work, (1, 1));
}
