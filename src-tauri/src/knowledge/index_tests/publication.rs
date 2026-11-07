use super::*;
fn document_chunks(h: &KnowledgeHarness, count: u32) -> Uuid {
    let id = h.import_text("paged", "first");
    h.h.store
        .transaction(|tx| {
            for ordinal in 1..count {
                let text = format!("chunk {ordinal}");
                let location = TextLocation {
                    page: None,
                    paragraph: ordinal + 1,
                    heading: Some(format!("heading {ordinal}")),
                    start_char: 0,
                    end_char: text.chars().count() as u32,
                };
                tx.execute(
                    "INSERT INTO document_chunks(id,document_id,\
                    chunk_index,text,location_json,content_hash,\
                    document_version,index_state) VALUES (?1,?2,\
                    ?3,?4,?5,?6,1,'parsed')",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        id.to_string(),
                        ordinal,
                        text,
                        serde_json::to_string(&location).unwrap(),
                        crate::knowledge::chunk_hash(&text)
                    ],
                )?;
            }
            Ok(())
        })
        .unwrap();
    id
}
#[tokio::test]
async fn knowledge_publication_real_multi_page_ready_and_dense_rebuild() {
    let h = KnowledgeHarness::new().await;
    let id = document_chunks(&h, 241);
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let ready: u32 =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*) FROM document_chunks WHERE \
                    document_id=?1 AND index_state='ready'",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(ready, 241);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.rebuild_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    let (count, last): (u32, String) =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT count(*),max(CAST(vector_id AS \
                    INTEGER)) FROM document_chunks WHERE \
                    document_id=?1 AND index_state='ready'",
                    [id.to_string()],
                    |r| Ok((r.get(0)?, r.get::<_, u32>(1)?.to_string())),
                )?)
            })
            .unwrap();
    assert_eq!((count, last), (241, "240".into()));
    h.close().await;
}
#[tokio::test]
async fn knowledge_publication_rolls_back_every_page_on_document_location_pointer_or_evidence_change()
 {
    for mutation in [
        "version",
        "sha",
        "parser",
        "import",
        "location",
        "pointer",
        "evidence",
        "missing_evidence",
        "binding",
    ] {
        let h = KnowledgeHarness::new().await;
        let id = document_chunks(&h, 241);
        let store = h.h.store.clone();
        h.service.at_checkpoint(CommitPoint::BeforeReady, move || {
            store
                .transaction(|tx| {
                    match mutation {
                        "version" => {
                            tx.execute(
                                "UPDATE documents SET version=2 WHERE id=?1",
                                [id.to_string()],
                            )?;
                        }
                        "sha" => {
                            tx.execute(
                                "UPDATE documents SET sha256=?1 WHERE id=?2",
                                rusqlite::params!["b".repeat(64), id.to_string()],
                            )?;
                        }
                        "parser" => {
                            tx.execute(
                                "UPDATE documents SET parser_identity='different' WHERE id=?1",
                                [id.to_string()],
                            )?;
                        }
                        "import" => {
                            tx.execute(
                                "UPDATE documents SET import_job_id=(SELECT \
                    id FROM intelligence_jobs WHERE object_id=?1 \
                    AND kind='reindex' ORDER BY created_at_ms \
                    DESC LIMIT 1) WHERE id=?1",
                                [id.to_string()],
                            )?;
                        }
                        "location" => {
                            tx.execute(
                                "UPDATE document_chunks SET \
                    location_json=json_set(location_json,\
                    '$.heading','changed') WHERE document_id=?1 \
                    AND chunk_index=240",
                                [id.to_string()],
                            )?;
                        }
                        "pointer" => {
                            tx.execute(
                                "UPDATE knowledge_index_generations SET active_generation=?1",
                                [Uuid::new_v4().to_string()],
                            )?;
                        }
                        "evidence" => {
                            tx.execute(
                                "UPDATE knowledge_index_work_items SET \
                    document_fence_json='{}' WHERE ordinal=240",
                                [],
                            )?;
                        }
                        "missing_evidence" => {
                            tx.execute(
                                "UPDATE knowledge_index_work_items SET \
                    document_fence_json=NULL WHERE ordinal=240",
                                [],
                            )?;
                        }
                        "binding" => {
                            tx.execute(
                                "UPDATE knowledge_index_generations SET semantic_key=?1",
                                ["c".repeat(64)],
                            )?;
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                })
                .unwrap();
        });
        assert_ne!(h.index(id).await.status, JobStatus::Succeeded, "{mutation}");
        let (published, ready, dirty): (u32, u32, u32) =
            h.h.store
                .transaction(|tx| {
                    Ok((
                        tx.query_row(
                            "SELECT count(*) FROM knowledge_generation_mappings WHERE published=1",
                            [],
                            |r| r.get(0),
                        )?,
                        tx.query_row(
                            "SELECT count(*) FROM document_chunks WHERE index_state='ready'",
                            [],
                            |r| r.get(0),
                        )?,
                        tx.query_row(
                            "SELECT count(*) FROM knowledge_index_generation_seals WHERE dirty=1",
                            [],
                            |r| r.get(0),
                        )?,
                    ))
                })
                .unwrap();
        assert_eq!((published, ready, dirty), (0, 0, 1), "{mutation}");
        h.close().await;
    }
}
#[tokio::test]
async fn knowledge_reindex_every_commit_boundary_excludes_unpublished_and_explicit_retry_succeeds()
{
    for point in [
        CommitPoint::AfterSqliteIntent,
        CommitPoint::AfterVectorMutation,
        CommitPoint::AfterVectorPersist,
        CommitPoint::BeforeReady,
    ] {
        let h = KnowledgeHarness::new().await;
        let id = h.import_text("crash", "24V");
        h.service.crash_at(point);
        assert_ne!(h.index(id).await.status, JobStatus::Succeeded);
        h.close().await;
        h.index.start_owned(Arc::new(())).await.unwrap();
        let before = h.embeddings.calls.load(Ordering::SeqCst);
        assert!(h.search("24V").await.is_err());
        assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), before);
        h.h.jobs.start().unwrap();
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
        h.close().await;
    }
}
#[tokio::test]
async fn knowledge_reconcile_tombstone_pressure_rebuilds_locally_without_explicit_flag() {
    let h = KnowledgeHarness::new().await;
    let deleted = h.import_text("delete", "24V");
    let survivor = h.import_text("survive", "48V");
    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(survivor).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(deleted).await.status, JobStatus::Succeeded);
    let job = h.service.delete_owned(deleted, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let old: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    let (new, vector): (String, String) =
        h.h.store
            .transaction(|tx| {
                Ok((
                    tx.query_row(
                        "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                        [],
                        |r| r.get(0),
                    )?,
                    tx.query_row(
                        "SELECT vector_id FROM document_chunks WHERE document_id=?1",
                        [survivor.to_string()],
                        |r| r.get(0),
                    )?,
                ))
            })
            .unwrap();
    assert_ne!(
        new, old,
        "capacity consumed by retired/tombstoned records needs local replacement"
    );
    assert_eq!(vector, "0");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("48V").await.unwrap().hits[0].document_id, survivor);
    h.close().await;
}
#[tokio::test]
async fn knowledge_reconcile_clean_last_document_deletion_rebuilds_to_empty_generation() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("last", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let job = h.service.delete_owned(id, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    let count: u64 =
        h.h.store
            .transaction(|tx| {
                let mut q = tx.prepare(
                    "SELECT s.seal_json FROM \
                    knowledge_index_generation_seals s JOIN \
                    knowledge_index_generations g ON \
                    g.identity=s.identity AND \
                    g.active_generation=s.generation WHERE \
                    s.dirty=0",
                )?;
                let mut rows = q.query([])?;
                let row = rows.next()?.unwrap();
                Ok(
                    crate::knowledge::index::Seal::decode(crate::knowledge::service::state::text(
                        row, 0, 65536,
                    )?)?
                    .stored_count,
                )
            })
            .unwrap();
    assert_eq!(
        count, 0,
        "empty library should reclaim fully tombstoned capacity"
    );
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}
#[tokio::test]
async fn knowledge_rebuild_rejects_late_uncovered_survivor_chunk_without_switching_pointer() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("survivor", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let old: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    let store = h.h.store.clone();
    h.service
        .at_checkpoint(CommitPoint::AfterVectorPersist, move || {
            store
                .transaction(|tx| {
                    let (identity, generation): (String, String) = tx.query_row(
                        "SELECT identity,active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    let chunk = Uuid::new_v4();
                    let location = TextLocation {
                        page: None,
                        paragraph: 2,
                        heading: None,
                        start_char: 0,
                        end_char: 3,
                    };
                    let hash = crate::knowledge::chunk_hash("48V");
                    tx.execute(
                        "INSERT INTO document_chunks(id,document_id,\
                    chunk_index,text,location_json,content_hash,\
                    document_version,index_state,\
                    embedding_identity,vector_id) VALUES (?1,?2,\
                    1,'48V',?3,?4,1,'ready',?5,'1')",
                        rusqlite::params![
                            chunk.to_string(),
                            id.to_string(),
                            serde_json::to_string(&location).unwrap(),
                            hash,
                            identity
                        ],
                    )?;
                    tx.execute(
                        "INSERT INTO \
                    knowledge_generation_mappings(identity,\
                    generation,chunk_id,document_id,\
                    document_version,vector_id,content_hash,\
                    published) VALUES (?1,?2,?3,?4,1,'1',?5,1)",
                        rusqlite::params![
                            identity,
                            generation,
                            chunk.to_string(),
                            id.to_string(),
                            hash
                        ],
                    )?;
                    Ok(())
                })
                .unwrap();
        });
    assert!(
        h.service.rebuild_owned(Arc::new(())).await.is_err(),
        "every current survivor chunk must be covered before swap"
    );
    let current: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(current, old);
    h.close().await;
}

pub(super) fn reassociate(store: &crate::storage::LocalStore, id: Uuid, mutation: &str) {
    store
        .transaction(|tx| {
            match mutation {
                "sha" => {
                    tx.execute(
                        "UPDATE documents SET sha256=?1 WHERE id=?2",
                        rusqlite::params!["b".repeat(64), id.to_string()],
                    )?;
                }
                "parser" => {
                    tx.execute(
                        "UPDATE documents SET parser_identity='changed' WHERE id=?1",
                        [id.to_string()],
                    )?;
                }
                "import" => {
                    tx.execute(
                        "UPDATE documents SET import_job_id=(SELECT \
                    id FROM intelligence_jobs WHERE object_id=?1 \
                    AND kind='reindex' ORDER BY created_at_ms \
                    DESC LIMIT 1) WHERE id=?1",
                        [id.to_string()],
                    )?;
                }
                "location" => {
                    tx.execute(
                        "UPDATE document_chunks SET \
                    location_json=json_set(location_json,\
                    '$.heading','changed') WHERE document_id=?1",
                        [id.to_string()],
                    )?;
                }
                "mapping" => {
                    tx.execute(
                        "UPDATE document_chunks SET vector_id='17' WHERE document_id=?1",
                        [id.to_string()],
                    )?;
                    tx.execute(
                        "UPDATE knowledge_generation_mappings SET \
                    vector_id='17' WHERE document_id=?1 AND \
                    published=1",
                        [id.to_string()],
                    )?;
                }
                _ => unreachable!(),
            }
            Ok(())
        })
        .unwrap();
}
#[tokio::test]
async fn knowledge_local_copy_freezes_rebuild_associations_before_vector_copy() {
    for mutation in ["sha", "parser", "import", "location", "mapping"] {
        let h = KnowledgeHarness::new().await;
        let id = h.import_text("survivor", "24V");
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        let old: String =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                        [],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        let store = h.h.store.clone();
        h.service
            .at_checkpoint(CommitPoint::BeforeLocalCopy, move || {
                reassociate(&store, id, mutation)
            });
        assert!(
            h.service.rebuild_owned(Arc::new(())).await.is_err(),
            "pre-copy {mutation} association must stay frozen"
        );
        let current: String =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                        [],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        assert_eq!(current, old, "{mutation}");
        h.close().await;
    }
}

#[tokio::test]
async fn knowledge_rebuild_failed_target_keeps_source_clean_searchable_and_reopenable() {
    for point in [
        CommitPoint::BeforeLocalCopy,
        CommitPoint::AfterVectorPersist,
    ] {
        let h = KnowledgeHarness::new().await;
        let id = h.import_text("source", "24V");
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        let old: String =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT active_generation FROM \
                    knowledge_index_generations WHERE \
                    state='active'",
                        [],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        h.service.crash_at(point);
        assert!(h.service.rebuild_owned(Arc::new(())).await.is_err());
        let clean: bool =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT dirty=0 AND seal_json IS NOT NULL \
                    FROM knowledge_index_generation_seals WHERE \
                    generation=?1",
                        [&old],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        assert!(
            clean,
            "failed target must complete the source refresh after failure"
        );
        assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
        h.close().await;
        h.index.start_owned(Arc::new(())).await.unwrap();
        assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
        h.index.shutdown().await.unwrap();
    }
}
