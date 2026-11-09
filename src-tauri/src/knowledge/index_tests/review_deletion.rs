use super::*;

pub(super) async fn generation(h: &KnowledgeHarness) -> crate::knowledge::index::Generation {
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    crate::knowledge::service::state::binding(&h.h.store, &profile)
        .unwrap()
        .unwrap()
        .generation
}
pub(super) fn generation_path(
    h: &KnowledgeHarness,
    g: &crate::knowledge::index::Generation,
) -> std::path::PathBuf {
    h.h.temp_dir
        .path()
        .join("indexes")
        .join(g.key().unwrap())
        .join(g.generation.to_string())
}
pub(super) fn exists(h: &KnowledgeHarness, document: Uuid) -> bool {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                [document.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}
pub(super) fn work_count(h: &KnowledgeHarness, operation: Uuid) -> u32 {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
                [operation.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

async fn rejects_work_before_mutation(mutation: &str, resume: bool) {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let count = if mutation == "foreign_first" { 1 } else { 241 };
    let target = super::publication::document_chunks(&h, count);
    for _ in 0..count {
        let mut vector = vec![0.0; 128];
        vector[1] = 1.0;
        h.embeddings.vectors.lock().unwrap().push_back(vector);
    }
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let source = generation(&h).await;
    let path = generation_path(&h, &source);
    let vectors = std::fs::read(path.join("vectors.bin")).unwrap();
    let metadata = std::fs::read(path.join("metadata.db")).unwrap();
    let serial = h.service.core.serial.lock().await;
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    h.h.store.transaction(|tx| {
            match mutation {
                "foreign_first" | "foreign_late" => {
                    tx.execute("UPDATE knowledge_index_work_items SET vector_id='0' WHERE operation_id=?1 AND ordinal=?2",
                        rusqlite::params![job.id.to_string(), if mutation == "foreign_first" { 0 } else { 200 }])?;
                }
                "association_late" => {
                    tx.execute("UPDATE knowledge_index_work_items SET document_id='malformed' WHERE operation_id=?1 AND ordinal=200", [job.id.to_string()])?;
                }
                "fence_late" => {
                    tx.execute("UPDATE knowledge_index_work_items SET document_fence_json='{}' WHERE operation_id=?1 AND ordinal=200", [job.id.to_string()])?;
                }
                "missing_late" => {
                    tx.execute("DELETE FROM knowledge_index_work_items WHERE operation_id=?1 AND ordinal=200", [job.id.to_string()])?;
                }
                _ => unreachable!(),
            }
            Ok(())
        }).unwrap();
    drop(serial);
    let result = h.wait(job.id).await;
    assert_eq!(result.status, JobStatus::Failed, "{mutation}");
    assert_eq!(
        result.error,
        Some(IntelligenceErrorCode::IndexRepairRequired),
        "{mutation}"
    );
    assert!(exists(&h, target));
    assert!(exists(&h, good));
    assert_eq!(
        work_count(&h, job.id),
        count - u32::from(mutation == "missing_late")
    );
    let dirty: u32 = h.h.store.transaction(|tx| Ok(tx.query_row(
            "SELECT dirty FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2",
            rusqlite::params![source.key().unwrap(), source.generation.to_string()], |r| r.get(0)
        )?)).unwrap();
    assert_eq!(
        dirty, 0,
        "proof must reject before a mutable Open intent: {mutation}"
    );
    assert_eq!(std::fs::read(path.join("vectors.bin")).unwrap(), vectors);
    assert_eq!(std::fs::read(path.join("metadata.db")).unwrap(), metadata);
    let hits = h.search("48V").await.unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert_eq!(hits.hits[0].document_id, good);
    assert_eq!(hits.hits[0].text, "48V");
    if resume {
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        h.service.core.resume_deletes(Arc::new(())).await.unwrap();
        let status: String =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT status FROM documents WHERE id=?1",
                        [good.to_string()],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        assert_eq!(
            status, "ready",
            "invalid clean-source proof must isolate its target"
        );
        h.service.reconcile_owned(Arc::new(())).await.unwrap();
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        assert!(exists(&h, target));
        assert_eq!(work_count(&h, job.id), count);
        assert_eq!(std::fs::read(path.join("vectors.bin")).unwrap(), vectors);
        let mappings: u32 = h.h.store.transaction(|tx| Ok(tx.query_row(
                "SELECT count(*) FROM knowledge_generation_mappings WHERE document_id=?1 AND generation=?2",
                rusqlite::params![target.to_string(), source.generation.to_string()], |r| r.get(0)
            )?)).unwrap();
        assert_eq!(
            mappings, count,
            "orphan cleanup must preserve deletion evidence"
        );
        assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, good);
    }
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_delete_rejects_first_page_foreign_vector_before_mutation() {
    rejects_work_before_mutation("foreign_first", false).await;
}

#[tokio::test]
async fn knowledge_review_delete_rejects_later_page_foreign_vector_before_mutation() {
    rejects_work_before_mutation("foreign_late", false).await;
}

#[tokio::test]
async fn knowledge_review_delete_rejects_later_page_foreign_association_before_mutation() {
    rejects_work_before_mutation("association_late", false).await;
}

#[tokio::test]
async fn knowledge_review_delete_rejects_later_page_malformed_fence_before_mutation() {
    rejects_work_before_mutation("fence_late", false).await;
}

#[tokio::test]
async fn knowledge_review_delete_rejects_missing_later_page_work_before_mutation() {
    rejects_work_before_mutation("missing_late", false).await;
}

#[tokio::test]
async fn knowledge_review_delete_resume_preserves_invalid_target_proof_and_clean_neighbors() {
    rejects_work_before_mutation("foreign_first", true).await;
}

#[tokio::test]
async fn knowledge_review_delete_keeps_historical_published_mapping_evidence_without_old_chunks() {
    let h = KnowledgeHarness::new().await;
    let target = h.import_text("historical", "24V");
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let mut neighbors = Vec::new();
    for _ in 0..3 {
        let good = h.import_text("neighbor", "48V");
        assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
        neighbors.push(good);
    }
    h.h.store.transaction(|tx| {
        tx.execute("DELETE FROM document_chunks WHERE document_id=?1", [target.to_string()])?;
        tx.execute("UPDATE documents SET version=2,sha256=?1,status='parsed' WHERE id=?2", rusqlite::params!["b".repeat(64), target.to_string()])?;
        tx.execute("INSERT INTO document_chunks(id,document_id,chunk_index,text,location_json,content_hash,document_version,index_state) \
            VALUES (?1,?2,0,'36V',?3,?4,2,'parsed')", rusqlite::params![
            Uuid::new_v4().to_string(), target.to_string(),
            serde_json::json!({"page":null,"paragraph":1,"heading":null,"start_char":0,"end_char":3}).to_string(),
            crate::knowledge::chunk_hash("36V")
        ])?;
        Ok(())
    }).unwrap();
    assert_eq!(h.index(target).await.status, JobStatus::Succeeded);
    let historical: u32 = h.h.store.transaction(|tx| Ok(tx.query_row(
        "SELECT count(*) FROM knowledge_generation_mappings WHERE document_id=?1 AND document_version=1 AND published=1", [target.to_string()], |r| r.get(0)
    )?)).unwrap();
    assert_eq!(historical, 1);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
    assert!(!exists(&h, target));
    assert_eq!(work_count(&h, job.id), 0);
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    let hits = h.search("48V").await.unwrap();
    let mut actual = hits
        .hits
        .iter()
        .map(|hit| hit.document_id)
        .collect::<Vec<_>>();
    actual.sort();
    neighbors.sort();
    assert_eq!(actual, neighbors);
    h.close().await;
}
