//! Real-index regressions for the independent Task 5C review findings.
use super::*;

async fn mixed_documents() -> (KnowledgeHarness, Uuid, Uuid) {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("good 型号", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let broken = super::publication::document_chunks(&h, 2);
    let mut first = vec![0.0; 128];
    first[0] = 0.8;
    first[1] = 0.6;
    let mut second = vec![0.0; 128];
    second[1] = 1.0;
    h.embeddings.vectors.lock().unwrap().extend([first, second]);
    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
    (h, good, broken)
}

fn remove_last_mapping(store: &crate::storage::LocalStore, document: Uuid) {
    let removed = store
        .transaction(|tx| {
            Ok(tx.execute(
                "DELETE FROM knowledge_generation_mappings WHERE \
                    document_id=?1 AND chunk_id IN (SELECT id \
                    FROM document_chunks WHERE document_id=?1 \
                    AND chunk_index=1)",
                [document.to_string()],
            )?)
        })
        .unwrap();
    assert_eq!(removed, 1);
}

fn document_status(h: &KnowledgeHarness, document: Uuid) -> String {
    h.h.store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT status FROM documents WHERE id=?1",
                [document.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

async fn assert_healthy_neighbor(h: &KnowledgeHarness, good: Uuid, broken: Uuid) {
    let result = h.search("voltage").await.unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].document_id, good);
    assert_eq!(result.hits[0].title, "good 型号");
    assert_eq!(result.hits[0].text, "48V");
    assert_eq!(result.unavailable_documents, 1);
    assert_eq!(document_status(h, good), "ready");
    assert_eq!(document_status(h, broken), "repair_required");
}

#[tokio::test]
async fn knowledge_review_missing_mapping_excludes_the_entire_document() {
    let (h, good, broken) = mixed_documents().await;
    remove_last_mapping(&h.h.store, broken);
    assert_healthy_neighbor(&h, good, broken).await;
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_healthy_neighbor(&h, good, broken).await;
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_reconcile_audits_missing_mappings_from_current_chunks() {
    let (h, good, broken) = mixed_documents().await;
    remove_last_mapping(&h.h.store, broken);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(document_status(&h, broken), "repair_required");
    assert_healthy_neighbor(&h, good, broken).await;
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_final_search_recheck_excludes_a_newly_incomplete_document() {
    let (h, good, broken) = mixed_documents().await;
    let store = h.h.store.clone();
    h.service
        .at_checkpoint(CommitPoint::BeforeSearchRecheck, move || {
            remove_last_mapping(&store, broken);
        });
    assert_healthy_neighbor(&h, good, broken).await;
    h.close().await;
}

#[tokio::test]
async fn knowledge_review_oversized_title_repairs_its_owner_and_preserves_neighbors() {
    // The second title fits the scalar bound but exceeds 32,768 UTF-8 bytes.
    for title in ["a".repeat(32769), "电".repeat(10923)] {
        let (h, good, broken) = mixed_documents().await;
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE documents SET title=?1 WHERE id=?2",
                    rusqlite::params![title, broken.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        assert_healthy_neighbor(&h, good, broken).await;
        let calls = h.embeddings.calls.load(Ordering::SeqCst);
        h.service.reconcile_owned(Arc::new(())).await.unwrap();
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
        h.close().await;
    }
}
