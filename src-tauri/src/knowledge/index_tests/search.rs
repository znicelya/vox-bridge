use super::*;
fn request(query: &str) -> KnowledgeSearch {
    KnowledgeSearch {
        query: query.into(),
        selected_segment_ids: vec![],
        session_id: None,
        limit: 5,
    }
}
#[tokio::test]
async fn knowledge_search_two_slots_retain_capacity_until_cancelled_entered_work_finishes() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    h.embeddings.block.store(true, Ordering::SeqCst);
    let service = h.service.clone();
    let first =
        tokio::spawn(async move { service.search_owned(request("first"), Arc::new(())).await });
    h.embeddings.entered.notified().await;
    let second = h.service.search_owned(request("second"), Arc::new(()));
    tokio::pin!(second);
    assert!(futures_util::poll!(&mut second).is_pending());
    let third = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        h.service.search_owned(request("third"), Arc::new(())),
    )
    .await;
    // Release entered work before asserting, including the intentional RED.
    h.embeddings.block.store(false, Ordering::SeqCst);
    h.embeddings.release.notify_waiters();
    let first = first.await.unwrap();
    let second = second.await;
    h.close().await;
    assert!(matches!(
        third,
        Ok(Err(AppError::Intelligence(IntelligenceErrorCode::Busy)))
    ));
    assert!(matches!(
        first,
        Err(AppError::Intelligence(IntelligenceErrorCode::Cancelled))
    ));
    assert!(second.is_ok());
}
#[tokio::test]
async fn knowledge_search_submitted_partial_and_multi_row_selection_is_authoritative() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let session = h.h.session();
    let foreign = h.h.session();
    let mut ids = Vec::new();
    for (sequence, text) in [
        (
            0,
            "A much longer transcript row containing few selected words and more content",
        ),
        (1, "A second longer row"),
    ] {
        let id = Uuid::new_v4();
        h.h.store
            .append_final_segment(&crate::storage::StoredTranscriptSegment {
                id,
                session_id: session,
                direction: "source_to_target".into(),
                sequence,
                occurred_at: 1,
                language: "en".into(),
                text: text.into(),
                is_final: true,
                details: None,
                occurred_at_ms: None,
            })
            .unwrap();
        ids.push(id);
    }
    let partial = KnowledgeSearch {
        query: "few selected words".into(),
        selected_segment_ids: vec![ids[0]],
        session_id: Some(session),
        limit: 5,
    };
    h.service.search_owned(partial, Arc::new(())).await.unwrap();
    assert_eq!(
        h.embeddings.requests.lock().unwrap().last().unwrap(),
        &["few selected words"]
    );
    let multi = KnowledgeSearch {
        query: "more content\nA second".into(),
        selected_segment_ids: ids.clone(),
        session_id: Some(session),
        limit: 5,
    };
    h.service.search_owned(multi, Arc::new(())).await.unwrap();
    assert_eq!(
        h.embeddings.requests.lock().unwrap().last().unwrap(),
        &["more content\nA second"]
    );
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let invalid = KnowledgeSearch {
        query: "few selected words".into(),
        selected_segment_ids: ids,
        session_id: Some(foreign),
        limit: 5,
    };
    assert!(matches!(
        h.service.search_owned(invalid, Arc::new(())).await,
        Err(AppError::Intelligence(IntelligenceErrorCode::Configuration))
    ));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}
#[tokio::test]
async fn knowledge_search_late_loss_of_last_ready_document_returns_repair() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let store = h.h.store.clone();
    *h.embeddings.after.lock().unwrap() = Some(Box::new(move || {
        store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE documents SET status='repair_required' WHERE id=?1",
                    [id.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
    }));
    assert!(matches!(
        h.search("24V").await,
        Err(AppError::Intelligence(
            IntelligenceErrorCode::IndexRepairRequired
        ))
    ));
    h.close().await;
}
#[tokio::test]
async fn knowledge_search_during_replacement_preserves_parsing_and_parsed_states() {
    let h = KnowledgeHarness::new().await;
    let good = h.import_text("good", "24V");
    let replacing = h.import_text("replace", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(replacing).await.status, JobStatus::Succeeded);
    for status in ["parsing", "parsed"] {
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE documents SET status=?1,version=2,index_identity=NULL WHERE id=?2",
                    rusqlite::params![status, replacing.to_string()],
                )?;
                tx.execute(
                    "UPDATE document_chunks SET index_state='stale' WHERE document_id=?1",
                    [replacing.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        let result = h.search("voltage").await.unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].document_id, good);
        let actual: String =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT status FROM documents WHERE id=?1",
                        [replacing.to_string()],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        assert_eq!(actual, status);
    }
    h.close().await;
}
#[tokio::test]
async fn knowledge_search_removes_earlier_hits_when_later_candidate_repairs_same_document() {
    let h = KnowledgeHarness::new().await;
    let broken = h.import_text("broken", "24V");
    let extra = Uuid::new_v4();
    let location = TextLocation {
        page: None,
        paragraph: 2,
        heading: None,
        start_char: 0,
        end_char: 3,
    };
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "INSERT INTO document_chunks(id,document_id,\
                    chunk_index,text,location_json,content_hash,\
                    document_version,index_state) VALUES (?1,?2,\
                    1,'48V',?3,?4,1,'parsed')",
                rusqlite::params![
                    extra.to_string(),
                    broken.to_string(),
                    serde_json::to_string(&location).unwrap(),
                    crate::knowledge::chunk_hash("48V")
                ],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
    let good = h.import_text("good", "96V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
        .unwrap()
        .unwrap();
    let key = binding.generation.key().unwrap();
    let generation = binding.generation.generation;
    let ticket = h
        .index
        .open_existing(binding.generation, Arc::new(()))
        .unwrap();
    h.index
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let mut vector = vec![0.0; 128];
    vector[0] = 0.9;
    vector[1] = (1.0f32 - 0.9 * 0.9).sqrt();
    let new = h
        .index
        .corrupt_metadata(&ticket, 1, vector, r#"{"schema_version":1}"#.into())
        .await
        .unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE document_chunks SET vector_id=?1 WHERE id=?2",
                rusqlite::params![new.to_string(), extra.to_string()],
            )?;
            tx.execute(
                "UPDATE knowledge_generation_mappings SET \
                    vector_id=?1 WHERE chunk_id=?2 AND \
                    identity=?3 AND generation=?4",
                rusqlite::params![
                    new.to_string(),
                    extra.to_string(),
                    key,
                    generation.to_string()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let receipt = h.index.persist(&ticket).await.unwrap();
    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
    h.index.acknowledge(&ticket, ack).await.unwrap();
    let result = h.search("voltage").await.unwrap();
    assert_eq!(
        result
            .hits
            .iter()
            .map(|hit| hit.document_id)
            .collect::<Vec<_>>(),
        vec![good],
        "the first good-looking chunk cannot survive repair of its document"
    );
    h.close().await;
}
#[tokio::test]
async fn knowledge_reconcile_malformed_metadata_repairs_only_current_inverse_owner() {
    let h = KnowledgeHarness::new().await;
    let broken = h.import_text("broken", "24V");
    let good = h.import_text("good", "48V");
    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
        .unwrap()
        .unwrap();
    let key = binding.generation.key().unwrap();
    let generation = binding.generation.generation;
    let ticket = h
        .index
        .open_existing(binding.generation, Arc::new(()))
        .unwrap();
    h.index
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let mut vector = vec![0.0; 128];
    vector[0] = 1.0;
    let new = h
        .index
        .corrupt_metadata(&ticket, 0, vector, r#"{"schema_version":1}"#.into())
        .await
        .unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE document_chunks SET vector_id=?1 WHERE document_id=?2",
                rusqlite::params![new.to_string(), broken.to_string()],
            )?;
            tx.execute(
                "UPDATE knowledge_generation_mappings SET \
                    vector_id=?1 WHERE document_id=?2 AND \
                    identity=?3 AND generation=?4",
                rusqlite::params![
                    new.to_string(),
                    broken.to_string(),
                    key,
                    generation.to_string()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let receipt = h.index.persist(&ticket).await.unwrap();
    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
    h.index.acknowledge(&ticket, ack).await.unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
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
        "one malformed row must not quarantine healthy neighbors"
    );
    assert_eq!(h.search("voltage").await.unwrap().hits[0].document_id, good);
    h.close().await;
}
#[tokio::test]
async fn knowledge_search_actual_escaped_byte_budget_preserves_prefix_without_filling_lower_hits() {
    let h = KnowledgeHarness::new().await;
    let mut ids = Vec::new();
    for n in 0..4 {
        let id = h.import_text(&format!("document {n}"), "24V");
        if n < 3 {
            // Legal canonical SQLite location remains below16 MiB. Four repeated
            // headings are under32 MiB raw but require nearly64 MiB escaped JSON.
            let location = TextLocation {
                page: None,
                paragraph: 1,
                heading: Some("\t".repeat(8 * 1024 * 1024 - 1024)),
                start_char: 0,
                end_char: 3,
            };
            h.h.store
                .transaction(|tx| {
                    tx.execute(
                        "UPDATE document_chunks SET location_json=?1 WHERE document_id=?2",
                        rusqlite::params![
                            serde_json::to_string(&location).unwrap(),
                            id.to_string()
                        ],
                    )?;
                    Ok(())
                })
                .unwrap();
        }
        let score = 1.0 - n as f32 * 0.1;
        let mut vector = vec![0.0; 128];
        vector[0] = score;
        vector[1] = (1.0 - score * score).sqrt();
        h.embeddings.vectors.lock().unwrap().push_back(vector);
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        ids.push(id);
    }
    let result = h.search("voltage").await.unwrap();
    assert_eq!(
        result
            .hits
            .iter()
            .map(|hit| hit.document_id)
            .collect::<Vec<_>>(),
        ids[..2]
    );
    assert_eq!(result.unavailable_documents, 0);
    let actual = serde_json::to_vec(&result).unwrap().len();
    assert!(actual <= 32 * 1024 * 1024);
    assert!(actual > 31 * 1024 * 1024);
    assert_eq!(
        result.hits[0].location.heading.as_ref().unwrap().len(),
        8 * 1024 * 1024 - 1024
    );
    h.close().await;
}
#[tokio::test]
async fn knowledge_corruption_repair_does_not_change_indexing_replacement() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("replace", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET status='indexing',version=2 WHERE id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    crate::knowledge::service::state::mark_document_repair(&h.h.store, id, 1, "stale_hash")
        .unwrap();
    let status: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT status FROM documents WHERE id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(status, "indexing");
    h.close().await;
}
#[tokio::test]
async fn knowledge_search_foreign_document_metadata_repairs_inverse_owner_only() {
    let h = KnowledgeHarness::new().await;
    let broken = h.import_text("broken", "24V");
    let good = h.import_text("good", "48V");
    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
        .unwrap()
        .unwrap();
    let key = binding.generation.key().unwrap();
    let generation = binding.generation.generation;
    let metadata =
        h.h.store
            .transaction(|tx| {
                let (chunk, hash, location): (String, String, String) = tx.query_row(
                    "SELECT id,content_hash,location_json FROM \
                    document_chunks WHERE document_id=?1",
                    [good.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                Ok(crate::knowledge::index::ChunkMetadata::new(
                    good,
                    1,
                    Uuid::parse_str(&chunk).unwrap(),
                    hash,
                    &serde_json::from_str(&location).unwrap(),
                )?)
            })
            .unwrap();
    let ticket = h
        .index
        .open_existing(binding.generation, Arc::new(()))
        .unwrap();
    h.index
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let mut vector = vec![0.; 128];
    vector[0] = 1.;
    let new = h
        .index
        .corrupt_metadata(
            &ticket,
            0,
            vector,
            serde_json::to_string(&metadata).unwrap(),
        )
        .await
        .unwrap();
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE document_chunks SET vector_id=?1 WHERE document_id=?2",
                rusqlite::params![new.to_string(), broken.to_string()],
            )?;
            tx.execute(
                "UPDATE knowledge_generation_mappings SET \
                    vector_id=?1 WHERE document_id=?2 AND \
                    identity=?3 AND generation=?4",
                rusqlite::params![
                    new.to_string(),
                    broken.to_string(),
                    key,
                    generation.to_string()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    let receipt = h.index.persist(&ticket).await.unwrap();
    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
    h.index.acknowledge(&ticket, ack).await.unwrap();
    let result = h.search("voltage").await.unwrap();
    assert_eq!(
        result
            .hits
            .iter()
            .map(|h| h.document_id)
            .collect::<Vec<_>>(),
        vec![good]
    );
    assert_eq!(result.unavailable_documents, 1);
    h.close().await;
}
#[tokio::test]
async fn knowledge_reconcile_half_open_tombstone_and_last_orphan_are_local() {
    let h = KnowledgeHarness::new().await;
    let first = h.import_text("first", "24V");
    let last = h.import_text("last", "48V");
    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(last).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(first).await.status, JobStatus::Succeeded);
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let binding = crate::knowledge::service::state::binding(&h.h.store, &profile)
        .unwrap()
        .unwrap();
    let ticket = h
        .index
        .open_existing(binding.generation, Arc::new(()))
        .unwrap();
    h.index
        .enqueue_open(&ticket)
        .unwrap()
        .observe()
        .await
        .unwrap();
    let metadata = crate::knowledge::index::ChunkMetadata::new(
        Uuid::new_v4(),
        1,
        Uuid::new_v4(),
        crate::knowledge::chunk_hash("orphan"),
        &TextLocation {
            page: None,
            paragraph: 1,
            heading: None,
            start_char: 0,
            end_char: 6,
        },
    )
    .unwrap();
    let mut vector = vec![0.; 128];
    vector[0] = 1.;
    assert_eq!(
        h.index
            .add(&ticket, vec![(vector, metadata)])
            .await
            .unwrap(),
        vec![3]
    );
    let receipt = h.index.persist(&ticket).await.unwrap();
    let ack = h.index.publish(&ticket, &receipt).await.unwrap();
    h.index.acknowledge(&ticket, ack).await.unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    let result = h.search("voltage").await.unwrap();
    let mut actual = result
        .hits
        .iter()
        .map(|h| h.document_id)
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = vec![first, last];
    expected.sort();
    assert_eq!(actual, expected);
    h.close().await;
}

#[tokio::test]
async fn knowledge_search_malformed_source_fields_repair_owner_and_preserve_neighbors() {
    for mutation in ["location", "body", "identity"] {
        let h = KnowledgeHarness::new().await;
        let broken = h.import_text("broken", "24V");
        let good = h.import_text("good", "48V");
        assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
        assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
        h.h.store
            .transaction(|tx| {
                match mutation {
                    "location" => {
                        tx.execute(
                            "UPDATE document_chunks SET location_json='{}' WHERE document_id=?1",
                            [broken.to_string()],
                        )?;
                    }
                    "body" => {
                        tx.execute(
                            "UPDATE document_chunks SET text=?1 WHERE document_id=?2",
                            rusqlite::params!["x".repeat(9601), broken.to_string()],
                        )?;
                    }
                    "identity" => {
                        tx.execute(
                            "UPDATE document_chunks SET embedding_identity=?1 WHERE document_id=?2",
                            rusqlite::params!["x".repeat(65), broken.to_string()],
                        )?;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            })
            .unwrap();
        let result = h.search("voltage").await.unwrap();
        assert_eq!(
            result
                .hits
                .iter()
                .map(|hit| hit.document_id)
                .collect::<Vec<_>>(),
            vec![good],
            "{mutation}"
        );
        let status: String =
            h.h.store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT status FROM documents WHERE id=?1",
                        [broken.to_string()],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
        assert_eq!(status, "repair_required", "{mutation}");
        h.close().await;
    }
}

#[tokio::test]
async fn knowledge_search_final_envelope_recheck_trims_only_trailing_whole_hit() {
    let h = KnowledgeHarness::new().await;
    let mut ids = Vec::new();
    for n in 0..2 {
        let id = h.import_text(&format!("document{n}"), "24V");
        let location = TextLocation {
            page: None,
            paragraph: 1,
            heading: Some("\t".repeat(8 * 1024 * 1024 - 1024)),
            start_char: 0,
            end_char: 3,
        };
        h.h.store
            .transaction(|tx| {
                tx.execute(
                    "UPDATE document_chunks SET location_json=?1 WHERE document_id=?2",
                    rusqlite::params![serde_json::to_string(&location).unwrap(), id.to_string()],
                )?;
                Ok(())
            })
            .unwrap();
        let score = 1.0 - n as f32 * 0.1;
        let mut vector = vec![0.; 128];
        vector[0] = score;
        vector[1] = (1.0 - score * score).sqrt();
        h.embeddings.vectors.lock().unwrap().push_back(vector);
        assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
        ids.push(id);
    }
    let initial = h.search("voltage").await.unwrap();
    assert_eq!(initial.hits.len(), 2);
    let gap = 32 * 1024 * 1024 - serde_json::to_vec(&initial).unwrap().len();
    assert!(gap < 32768 - 9);
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE documents SET title=title||?1 WHERE id=?2",
                rusqlite::params!["x".repeat(gap), ids[1].to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        serde_json::to_vec(&h.search("voltage").await.unwrap())
            .unwrap()
            .len(),
        32 * 1024 * 1024
    );
    let store = h.h.store.clone();
    h.service
        .at_checkpoint(CommitPoint::BeforeSearchRecheck, move || {
            store
                .transaction(|tx| {
                    for _ in 0..10 {
                        tx.execute(
                            "INSERT INTO documents(id,sha256,uri,title,\
                    created_at,origin,kind,status,\
                    parser_identity) VALUES (?1,?2,'',\
                    'unavailable',1,'native','text','parsed',\
                    'fixture')",
                            rusqlite::params![Uuid::new_v4().to_string(), "a".repeat(64)],
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
        });
    let result = h.search("voltage").await.unwrap();
    assert_eq!(
        result
            .hits
            .iter()
            .map(|hit| hit.document_id)
            .collect::<Vec<_>>(),
        vec![ids[0]]
    );
    assert_eq!(result.unavailable_documents, 10);
    assert_eq!(
        result.hits[0].location.heading,
        initial.hits[0].location.heading
    );
    assert!(serde_json::to_vec(&result).unwrap().len() < 32 * 1024 * 1024);
    h.close().await;
}

#[tokio::test]
async fn knowledge_reconcile_preserves_indexing_with_unfinished_matching_reindex_owner() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pending", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let _serial = h.service.core.serial.lock().await;
    h.h.store
        .transaction(|tx| {
            crate::intelligence::store::admit_job(
                tx,
                crate::intelligence::store::JobDraft {
                    id: Uuid::new_v4(),
                    kind: JobKind::Reindex,
                    session_id: None,
                    object_id: Some(id),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{id}"),
                    },
                    payload: serde_json::json!({
                    "version":1,
                    "profiles":[profile],
                    "input":{
                    "document_id":id,
                    "document_version":1}
                    }
                    ),
                    initial_failure: None,
                },
            )?;
            tx.execute(
                "UPDATE documents SET status='indexing' WHERE id=?1",
                [id.to_string()],
            )?;
            tx.execute(
                "UPDATE document_chunks SET index_state='indexing' WHERE document_id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.service.core.reconcile(Arc::new(()), false).await.unwrap();
    let status: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT status FROM documents WHERE id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(status, "indexing");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    drop(_serial);
    h.close().await;
}
