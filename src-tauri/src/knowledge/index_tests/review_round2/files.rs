use super::*;

fn retained_state(
    h: &KnowledgeHarness,
    target: Uuid,
    source: &crate::knowledge::index::Generation,
    operation: Uuid,
) -> serde_json::Value {
    h.h.store.transaction(|tx| {
        let raw: String = tx.query_row(
            "SELECT json_object(\
            'document',(SELECT json_array(version,sha256,parser_identity,managed_path,status,import_job_id) FROM documents WHERE id=?1),\
            'imports',(SELECT json_group_array(json_array(document_version,job_id,parser_identity,sha256,file_id)) FROM (SELECT * FROM knowledge_document_imports WHERE document_id=?1 ORDER BY document_version)),\
            'files',(SELECT json_group_array(json_array(file_id,relative_name,document_id,document_version,job_id,state)) FROM (SELECT * FROM knowledge_managed_files f WHERE document_id=?1 OR EXISTS(SELECT 1 FROM intelligence_jobs j WHERE j.id=f.job_id AND j.kind='import' AND j.object_id=?1) ORDER BY file_id)),\
            'mappings',(SELECT json_group_array(json_array(vector_id,document_id,document_version,chunk_id,content_hash,published)) FROM (SELECT * FROM knowledge_generation_mappings WHERE identity=?2 AND generation=?3 ORDER BY vector_id,document_id,document_version,chunk_id)),\
            'work',(SELECT json_group_array(json_array(ordinal,identity,generation,vector_id,document_id,document_version,chunk_id,content_hash,document_fence_json,location_sha256,previous_vector_id)) FROM (SELECT * FROM knowledge_index_work_items WHERE operation_id=?4 ORDER BY ordinal)),\
            'pointer',(SELECT active_generation FROM knowledge_index_generations WHERE identity=?2),\
            'dirty',(SELECT dirty FROM knowledge_index_generation_seals WHERE identity=?2 AND generation=?3))",
            rusqlite::params![target.to_string(), source.key().unwrap(), source.generation.to_string(), operation.to_string()],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&raw).unwrap())
    }).unwrap()
}

async fn historical_owner_rejection(fresh: bool) {
    let (h, good, target, imports) = native_target(2).await;
    assert_ne!(imports[0].id, imports[1].id);
    let source = generation(&h).await;
    let source_path = generation_path(&h, &source);
    let original = survivor_bits(&h, &source);
    let (historical, historical_path, historical_tmp) = original_file(&h, imports[0].id);
    let (_, current_path, current_tmp) = original_file(&h, imports[1].id);
    assert!(historical_path.is_file() && current_path.is_file());
    let files_before = (
        std::fs::read(&historical_path).unwrap(),
        std::fs::read(&current_path).unwrap(),
    );
    let prior = if fresh {
        Some(cancelled_delete(&h, target).await)
    } else {
        None
    };
    assert_eq!(h.h.store.transaction(|tx| Ok(tx.execute(
        "UPDATE knowledge_managed_files SET document_id=NULL WHERE file_id=?1 AND document_id=?2 AND document_version=1 AND job_id=?3",
        rusqlite::params![historical, target.to_string(), imports[0].id.to_string()],
    )?)).unwrap(), 1, "the only corruption is the historical nullable document link");
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let mut serial = Some(h.service.core.serial.lock().await);
    let job = match prior {
        Some(job) => job,
        None => h.service.delete_owned(target, Arc::new(())).await.unwrap(),
    };
    let before = retained_state(&h, target, &source, job.id);
    let before_bytes = std::fs::read(source_path.join("vectors.bin")).unwrap();
    let failed = if fresh {
        h.service
            .core
            .rebuild_generation(source.clone(), Arc::new(()))
            .await
            .is_err()
    } else {
        h.wait_running(job.id).await;
        drop(serial.take());
        h.wait(job.id).await.status == JobStatus::Failed
    };
    // Fresh publication has no handler competing for the held serial owner.
    // Live deletion releases it above before joining its real terminal result.
    let after = retained_state(&h, target, &source, job.id);
    let after_bytes = std::fs::read(source_path.join("vectors.bin")).ok();
    let after_files = (
        std::fs::read(&historical_path).ok(),
        std::fs::read(&current_path).ok(),
    );
    let no_certificate =
        h.h.store
            .transaction(|tx| {
                Ok(!tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM intelligence_meta WHERE key=?1)",
                    [format!("knowledge.delete.v1.operation.{}", job.id)],
                    |row| row.get::<_, bool>(0),
                )?)
            })
            .unwrap();
    drop(serial.take());
    // Capture every rejection assertion before removing the localized mutation.
    // In RED the old producer may already have deleted the domain. Remove that
    // deliberately omitted test file only for joined fixture teardown; the
    // captured before/after assertions below still report the real violation.
    if exists(&h, target) {
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE knowledge_managed_files SET document_id=?2 WHERE file_id=?1",
                    rusqlite::params![historical, target.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
    } else {
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "DELETE FROM knowledge_managed_files WHERE file_id=?1",
                    [historical.clone()],
                )?;
                Ok(())
            })
            .unwrap();
        std::fs::remove_file(&historical_path).unwrap();
        if historical_tmp.exists() {
            std::fs::remove_file(&historical_tmp).unwrap();
        }
    }
    if fresh {
        // Rejected publication retains its unpublished destination. Join the
        // old owner before local recovery so that both engine slots are free.
        h.close().await;
        h.index.start_owned(Arc::new(())).await.unwrap();
        h.h.jobs.start().unwrap();
    }
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(work_count(&h, job.id), 0);
    assert!(
        !historical_path.exists()
            && !historical_tmp.exists()
            && !current_path.exists()
            && !current_tmp.exists()
    );
    let replacement = generation(&h).await;
    let recovered_bits = survivor_bits(&h, &replacement);
    let recovered_calls = h.embeddings.calls.load(Ordering::SeqCst);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    h.close().await;
    assert!(
        failed,
        "localized historical ownership loss must reject the real producer"
    );
    assert_eq!(
        after, before,
        "preflight/publication must retain the complete native domain/import/file/work/mapping/source association before any deletion, fresh={fresh}"
    );
    assert_eq!(
        after_bytes,
        Some(before_bytes),
        "the destructive source operation must not run before complete original file ownership is proved"
    );
    assert_eq!(after_files, (Some(files_before.0), Some(files_before.1)));
    assert!(
        no_certificate,
        "an incomplete original file set must never be certified"
    );
    assert_eq!(recovered_bits, original);
    assert_eq!(recovered_calls, calls);
}

#[tokio::test]
async fn knowledge_review_round2_historical_native_owner_loss_rejects_live_delete() {
    historical_owner_rejection(false).await;
}

#[tokio::test]
async fn knowledge_review_round2_historical_native_owner_loss_rejects_fresh_publication() {
    historical_owner_rejection(true).await;
}
