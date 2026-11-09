use super::{
    embedding::{EmbeddingExecution, Embeddings},
    service::{CommitPoint, KnowledgeService},
    types::*,
};
use crate::{
    app::CredentialChange,
    error::{AppError, AppResult},
    intelligence::{test_support::*, types::*},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

#[derive(Default)]
struct Axes {
    vectors: std::sync::Mutex<std::collections::VecDeque<Vec<f32>>>,
    calls: AtomicUsize,
    requests: std::sync::Mutex<Vec<Vec<String>>>,
    block: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    after: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}
#[async_trait::async_trait]
impl Embeddings for Axes {
    async fn embed(
        &self,
        _: &ModelProfile,
        inputs: &[String],
        _execution: EmbeddingExecution,
    ) -> AppResult<Vec<Vec<f32>>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(inputs.to_vec());
        if self.block.load(Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        if let Some(after) = self.after.lock().unwrap().take() {
            after();
        }
        Ok(inputs
            .iter()
            .map(|_| {
                self.vectors.lock().unwrap().pop_front().unwrap_or_else(|| {
                    let mut v = vec![0.0; 128];
                    v[0] = 1.0;
                    v
                })
            })
            .collect())
    }
}
struct KnowledgeHarness {
    h: IntelligenceHarness,
    service: KnowledgeService,
    index: super::index::IndexOwner,
    embeddings: Arc<Axes>,
}
impl KnowledgeHarness {
    async fn new() -> Self {
        Self::with_auto(false).await
    }
    async fn with_auto(auto: bool) -> Self {
        let h = IntelligenceHarness::new();
        let embeddings = Arc::new(Axes::default());
        let profile = ModelProfile {
            kind: ServiceKind::Embedding,
            max_context_tokens: None,
            max_output_tokens: None,
            dimensions: Some(128),
            ..test_llm_profile()
        };
        h.config
            .save(SaveModelProfile {
                profile: profile.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-embedding-key".into(),
                },
            })
            .await
            .unwrap();
        let mut settings = h.config.settings().await.unwrap();
        settings.embedding_profile_id = Some(profile.id);
        h.config.save_settings(settings).await.unwrap();
        let index =
            super::index::IndexOwner::new(h.temp_dir.path().join("indexes"), h.store.clone());
        index.start_owned(Arc::new(())).await.unwrap();
        let service = KnowledgeService::new(
            h.store.clone(),
            h.config.clone(),
            h.jobs.clone(),
            embeddings.clone(),
            index.clone(),
            h.temp_dir.path().to_path_buf(),
        );
        service.register_handlers().unwrap();
        if auto {
            let documents = super::documents::DocumentService::new(
                h.store.clone(),
                h.config.clone(),
                h.jobs.clone(),
                Arc::new(FixtureParser),
                h.temp_dir.path().to_path_buf(),
            );
            documents.register_import_handler().unwrap();
            service.register_continuation().unwrap();
        }
        h.jobs.start().unwrap();
        Self {
            h,
            service,
            index,
            embeddings,
        }
    }
    fn import_text(&self, title: &str, text: &str) -> Uuid {
        let id = Uuid::new_v4();
        let chunk = Uuid::new_v4();
        let location = TextLocation {
            page: None,
            paragraph: 1,
            heading: None,
            start_char: 0,
            end_char: text.chars().count() as u32,
        };
        self.h
            .store
            .transaction(|tx| {
                tx.execute(
                    "INSERT INTO documents(id,sha256,uri,title,\
                    created_at,origin,kind,status,\
                    parser_identity) VALUES (?1,?2,'',?3,1,\
                    'native','text','parsed',?4)",
                    rusqlite::params![id.to_string(), "a".repeat(64), title, super::PARSER_ID],
                )?;
                tx.execute(
                    "INSERT INTO document_chunks(id,document_id,\
                    chunk_index,text,location_json,content_hash,\
                    document_version,index_state) VALUES (?1,?2,\
                    0,?3,?4,?5,1,'parsed')",
                    rusqlite::params![
                        chunk.to_string(),
                        id.to_string(),
                        text,
                        serde_json::to_string(&location).unwrap(),
                        super::chunk_hash(text)
                    ],
                )?;
                Ok(())
            })
            .unwrap();
        id
    }
    async fn observe_job(
        &self,
        jobs: &crate::intelligence::jobs::JobScheduler,
        id: Uuid,
        expected: Option<JobStatus>,
    ) -> JobView {
        // Subscribe before the first authoritative read. Notifications wake a
        // resync; they never replace durable status or spin on SQLite ownership.
        let mut events = jobs.subscribe();
        let started = std::time::Instant::now();
        let mut reads = 0usize;
        let matches = |job: &JobView| match expected {
            Some(status) => job.status == status,
            None => !matches!(job.status, JobStatus::Queued | JobStatus::Running),
        };
        // This bounds end-to-end test observation, including complete journals
        // under the default parallel gate; production deadlines are unchanged.
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                reads += 1;
                let j = self.h.store.get_job(id).unwrap().unwrap();
                if matches(&j) {
                    return j;
                }
                assert!(
                    matches!(j.status, JobStatus::Queued | JobStatus::Running),
                    "job {id} ended as {:?} before the expected state {expected:?}",
                    j.status
                );
                match events.recv().await {
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            self.h.store.get_job(id).unwrap().unwrap()
        })
        .await;
        if started.elapsed() >= std::time::Duration::from_secs(1) {
            eprintln!(
                "job observer {id}: elapsed={:?}, SQL reads={reads}, terminal={:?}",
                started.elapsed(),
                outcome.as_ref().map(|job| job.status)
            );
        }
        // A missed/evicted notification cannot turn a committed terminal result
        // into a timer failure. The same durable read resolves the final resync.
        let job = outcome.unwrap_or_else(|_| self.h.store.get_job(id).unwrap().unwrap());
        assert!(
            matches(&job),
            "job {id} did not reach {expected:?} within the finite observation bound: {:?}",
            job.status
        );
        job
    }
    async fn wait(&self, id: Uuid) -> JobView {
        self.wait_on(&self.h.jobs, id).await
    }
    async fn wait_on(&self, jobs: &crate::intelligence::jobs::JobScheduler, id: Uuid) -> JobView {
        self.observe_job(jobs, id, None).await
    }
    async fn wait_running(&self, id: Uuid) -> JobView {
        self.observe_job(&self.h.jobs, id, Some(JobStatus::Running))
            .await
    }
    async fn index(&self, id: Uuid) -> JobView {
        let j = self.service.reindex_owned(id, Arc::new(())).await.unwrap();
        self.wait(j.id).await
    }
    async fn search(&self, query: &str) -> AppResult<KnowledgeResults> {
        self.service
            .search_owned(
                KnowledgeSearch {
                    query: query.into(),
                    selected_segment_ids: vec![],
                    session_id: None,
                    limit: 5,
                },
                Arc::new(()),
            )
            .await
    }
    async fn close(&self) {
        self.h.jobs.shutdown().await.unwrap();
        self.index.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn knowledge_reindex_publishes_authoritative_ready_mapping_and_consecutive_searches_reopen_cleanly()
 {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump.md", "额定电压 24 V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    for _ in 0..2 {
        let result = h.search("额定电压").await.unwrap();
        assert_eq!(result.hits[0].document_id, id);
        assert_eq!(result.hits[0].text, "额定电压 24 V");
        assert_eq!(result.identity.as_ref().unwrap().len(), 64);
    }
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.search("额定电压").await.unwrap().hits[0].document_id, id);
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_crash_after_vector_persist_never_exposes_an_uncommitted_document() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump.md", "额定电压 24 V");
    h.service.crash_at(CommitPoint::AfterVectorPersist);
    assert_ne!(h.index(id).await.status, JobStatus::Succeeded);
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    let before = h.embeddings.calls.load(Ordering::SeqCst);
    assert!(matches!(
        h.search("额定电压").await,
        Err(AppError::Intelligence(
            IntelligenceErrorCode::IndexRepairRequired
        ))
    ));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), before);
    h.h.jobs.start().unwrap();
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    assert_eq!(h.search("额定电压").await.unwrap().hits[0].document_id, id);
    h.close().await;
}
#[tokio::test]
async fn knowledge_empty_library_and_broken_readiness_never_embed() {
    let h = KnowledgeHarness::new().await;
    let result = h.search("query").await.unwrap();
    assert!(result.hits.is_empty());
    assert_eq!(result.identity, None);
    h.import_text("pump", "24V");
    assert!(matches!(
        h.search("query").await,
        Err(AppError::Intelligence(
            IntelligenceErrorCode::IndexRepairRequired
        ))
    ));
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), 0);
    h.close().await;
}

#[tokio::test]
async fn knowledge_reindex_replacement_persists_a_new_vector_id() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let before: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT vector_id FROM document_chunks WHERE document_id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(before, "0");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let after: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT vector_id FROM document_chunks WHERE document_id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(after, "1");
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
    h.index.shutdown().await.unwrap();
}
#[tokio::test]
async fn knowledge_delete_survives_clean_reopen_and_never_embeds() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    let j = h.service.delete_owned(id, Arc::new(())).await.unwrap();
    assert_eq!(h.wait(j.id).await.status, JobStatus::Succeeded);
    assert!(
        h.h.store
            .transaction(|tx| Ok(!tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                [id.to_string()],
                |r| r.get::<_, bool>(0)
            )?))
            .unwrap()
    );
    h.close().await;
    h.index.start_owned(Arc::new(())).await.unwrap();
    assert!(h.search("24V").await.unwrap().hits.is_empty());
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.index.shutdown().await.unwrap();
}

#[tokio::test]
async fn knowledge_reconcile_marks_stale_authoritative_hash_without_embedding() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    let calls = h.embeddings.calls.load(Ordering::SeqCst);
    h.h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE document_chunks SET content_hash=?1 WHERE document_id=?2",
                rusqlite::params!["b".repeat(64), id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    h.service.reconcile_owned(Arc::new(())).await.unwrap();
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
    assert_eq!(status, "repair_required");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    h.close().await;
}
#[tokio::test]
async fn knowledge_rebuild_reclaims_tombstone_ids_and_atomically_repoints_ready_mappings() {
    let h = KnowledgeHarness::new().await;
    let id = h.import_text("pump", "24V");
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
    assert_eq!(h.index(id).await.status, JobStatus::Succeeded);
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
    h.service.rebuild_owned(Arc::new(())).await.unwrap();
    let new: String =
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
    assert_ne!(old, new);
    let vector: String =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT vector_id FROM document_chunks WHERE document_id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
    assert_eq!(vector, "0");
    assert_eq!(h.embeddings.calls.load(Ordering::SeqCst), calls);
    assert_eq!(h.search("24V").await.unwrap().hits[0].document_id, id);
    h.close().await;
}
#[tokio::test]
async fn knowledge_search_preserves_healthy_hits_and_repairs_the_inverse_owner_of_malformed_metadata()
 {
    let h = KnowledgeHarness::new().await;
    let broken = h.import_text("broken", "24V");
    assert_eq!(h.index(broken).await.status, JobStatus::Succeeded);
    let good = h.import_text("healthy", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let profile =
        h.h.config
            .selected_embedding_snapshot()
            .await
            .unwrap()
            .unwrap();
    let binding = super::service::state::binding(&h.h.store, &profile)
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
    let result = h.search("voltage").await.unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].document_id, good);
    assert_eq!(result.unavailable_documents, 1);
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
    assert_eq!(status, "repair_required");
    h.close().await;
}

mod search;

struct FixtureParser;
#[async_trait::async_trait]
impl super::parser::DocumentParser for FixtureParser {
    async fn parse(
        &self,
        path: std::path::PathBuf,
        _kind: DocumentKind,
        _cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
        let text = std::fs::read_to_string(path).map_err(|_| IntelligenceErrorCode::ParseFailed)?;
        let end = text.chars().count() as u32;
        Ok(vec![TextBlock {
            text,
            location: TextLocation {
                page: None,
                paragraph: 1,
                heading: None,
                start_char: 0,
                end_char: end,
            },
        }])
    }
}
mod binding;
mod continuation;

mod recovery;

mod publication;
mod review_deletion;
mod review_fixes;
mod review_jobs;
mod review_replacement;
mod review_round2;
