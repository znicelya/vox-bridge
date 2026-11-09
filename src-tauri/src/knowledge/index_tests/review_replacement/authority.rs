use super::flags::has_certificate;
use super::*;
use crate::knowledge::index::delete_completion as completed;

#[tokio::test]
async fn knowledge_review_retention_authority_source_removed_flag_cannot_authorize_sql_cleanup() {
    let h = KnowledgeHarness::new().await;
    let (_, _, _, first, second) = two_retained_deletes(&h).await;
    let source = h.h.store.transaction(|tx| {
        let raw: String = tx.query_row("SELECT s.seal_json FROM knowledge_index_generation_seals s \
            JOIN knowledge_index_generations g ON g.identity=s.identity AND g.active_generation=s.generation \
            WHERE s.dirty=1", [], |row| row.get(0))?;
        Ok(crate::knowledge::index::Seal::decode(&raw)?.descriptor)
    }).unwrap();
    let source_path = generation_path(&h, &source);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    h.service.crash_at(CommitPoint::AfterPointerPublication);
    assert!(h.service.reconcile_owned(Arc::new(())).await.is_err());
    h.index.shutdown().await.unwrap();
    h.index.start_owned(Arc::new(())).await.unwrap();
    let publication_id =
        h.h.store
            .transaction(|tx| Ok(completed::load(tx, first)?.unwrap().header.publication))
            .unwrap();
    for operation in [first, second] {
        let receipt = completed::check_files(
            h.h.store.clone(),
            h.h.temp_dir.path().to_path_buf(),
            operation,
            Arc::new(()),
            true,
        )
        .await
        .unwrap()
        .unwrap();
        h.h.store
            .transaction(|tx| {
                completed::mark_files_cleaned(tx, operation, &receipt)?;
                completed::acknowledge(tx, operation, 0)
            })
            .unwrap();
    }
    h.h.store
        .transaction(|tx| completed::settle_local(tx, publication_id, &h.h.jobs.reap_port()))
        .unwrap();
    let original =
        h.h.store
            .transaction(|tx| completed::read_publication(tx, publication_id))
            .unwrap();
    assert!(!original.source_removed && source_path.exists());
    let publication_key = format!("knowledge.delete.v1.publication.{publication_id}");
    h.h.store
        .transaction(|tx| {
            let mut forged = original.clone();
            forged.source_removed = true;
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![publication_key, serde_json::to_string(&forged).unwrap()],
            )?;
            Ok(())
        })
        .unwrap();
    let rejected = h.service.core.remove_generation_rows(&source).is_err();
    let retained_seal: bool = h.h.store.transaction(|tx| Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2)",
        rusqlite::params![source.key().unwrap(), source.generation.to_string()], |row| row.get(0),
    )?)).unwrap();
    let retained_mappings: u64 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
        "SELECT count(*) FROM knowledge_generation_mappings WHERE identity=?1 AND generation=?2",
        rusqlite::params![source.key().unwrap(), source.generation.to_string()], |row| row.get(0),
    )?)
            })
            .unwrap();
    let retained_bytes = source_path.exists();
    let retained_proof = has_certificate(&h, first) && has_certificate(&h, second);
    // Restore the deliberately changed flag, then use the real removal owner
    // to finish both the expected and the initially unsafe fixture branches.
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
                rusqlite::params![publication_key, serde_json::to_string(&original).unwrap()],
            )?;
            Ok(())
        })
        .unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert!(!source_path.exists());
    assert!(!has_certificate(&h, first) && !has_certificate(&h, second));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
    assert!(
        rejected,
        "one mutable source_removed replacement is not an actual owner removal receipt"
    );
    assert!(retained_seal && retained_bytes && retained_proof);
    assert_eq!(
        retained_mappings, 1,
        "SQL cleanup must reject before its first destructive page"
    );
}
