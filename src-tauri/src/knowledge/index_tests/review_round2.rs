use super::review_deletion::{exists, generation, generation_path, work_count};
use super::*;
use crate::knowledge::index::delete_completion as completed;

mod files;
mod peers;
mod startup;

fn queue_axis(h: &KnowledgeHarness, position: usize) {
    let mut vector = vec![0.0; 128];
    vector[position] = 1.0;
    h.embeddings.vectors.lock().unwrap().push_back(vector);
}

async fn cancelled_delete(h: &KnowledgeHarness, target: Uuid) -> JobView {
    let serial = h.service.core.serial.lock().await;
    let job = h.service.delete_owned(target, Arc::new(())).await.unwrap();
    h.wait_running(job.id).await;
    h.h.jobs.cancel(job.id).await.unwrap();
    drop(serial);
    assert_eq!(h.wait(job.id).await.status, JobStatus::Cancelled);
    assert!(exists(h, target));
    job
}

fn pad_original_fence(h: &KnowledgeHarness, operation: Uuid) {
    h.h.store
        .transaction(|tx| {
            let original: String = tx.query_row(
                "SELECT document_fence_json FROM knowledge_index_work_items WHERE operation_id=?1 AND ordinal=0",
                [operation.to_string()],
                |row| row.get(0),
            )?;
            let padded = format!("{original}{}", " ".repeat(1024 - original.len()));
            assert_eq!(padded.len(), 1024);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&padded).unwrap(),
                serde_json::from_str::<serde_json::Value>(&original).unwrap()
            );
            assert_eq!(tx.execute(
                "UPDATE knowledge_index_work_items SET document_fence_json=?2 WHERE operation_id=?1 AND ordinal=0",
                rusqlite::params![operation.to_string(), padded],
            )?, 1);
            Ok(())
        })
        .unwrap();
}

fn survivor_bits(h: &KnowledgeHarness, source: &crate::knowledge::index::Generation) -> Vec<u8> {
    let vectors = std::fs::read(generation_path(h, source).join("vectors.bin")).unwrap();
    vectors[24..24 + 128 * 4].to_vec()
}

async fn native_target(versions: u32) -> (KnowledgeHarness, Uuid, Uuid, Vec<JobView>) {
    let h = KnowledgeHarness::with_auto(true).await;
    let good = h.import_text("neighbor", "48V");
    assert_eq!(h.index(good).await.status, JobStatus::Succeeded);
    let documents = crate::knowledge::documents::DocumentService::new(
        h.h.store.clone(),
        h.h.config.clone(),
        h.h.jobs.clone(),
        Arc::new(FixtureParser),
        h.h.temp_dir.path().to_path_buf(),
    );
    let mut target = None;
    let mut imports = Vec::new();
    for version in 1..=versions {
        let original = h.h.temp_dir.path().join(format!("native-{version}.txt"));
        std::fs::write(&original, format!("native version {version}")).unwrap();
        queue_axis(&h, version as usize);
        let job = match target {
            None => documents.import_owned(original, Arc::new(())).await,
            Some(id) => documents.replace_owned(id, original, Arc::new(())).await,
        }
        .unwrap();
        assert_eq!(h.wait(job.id).await.status, JobStatus::Succeeded);
        target = job.object_id;
        let id = target.unwrap();
        let mut events = h.h.jobs.subscribe();
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                let ready: bool = h.h.store.transaction(|tx| Ok(tx.query_row(
                    "SELECT status='ready' AND version=?2 AND NOT EXISTS(SELECT 1 FROM intelligence_jobs WHERE object_id=?1 AND status IN ('queued','running')) FROM documents WHERE id=?1",
                    rusqlite::params![id.to_string(), version], |row| row.get(0),
                )?)).unwrap();
                if ready { break }
                match events.recv().await {
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(error) => panic!("native continuation closed before Ready: {error:?}"),
                }
            }
        }).await.unwrap();
        imports.push(job);
    }
    (h, good, target.unwrap(), imports)
}

fn original_file(
    h: &KnowledgeHarness,
    import: Uuid,
) -> (String, std::path::PathBuf, std::path::PathBuf) {
    let (id, name): (String, String) =
        h.h.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT file_id,relative_name FROM knowledge_managed_files WHERE job_id=?1",
                    [import.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .unwrap();
    let directory = h.h.temp_dir.path().join("knowledge/documents");
    (
        id.clone(),
        directory.join(name),
        directory.join(format!("{id}.tmp")),
    )
}

fn current_generation(store: &crate::storage::LocalStore) -> crate::knowledge::index::Generation {
    store.transaction(|tx| {
        let raw: String = tx.query_row(
            "SELECT s.seal_json FROM knowledge_index_generation_seals s JOIN knowledge_index_generations g ON g.identity=s.identity AND g.active_generation=s.generation WHERE g.state='active'",
            [], |row| row.get(0),
        )?;
        Ok(crate::knowledge::index::Seal::decode(&raw)?.descriptor)
    }).unwrap()
}

struct Reopened {
    store: Arc<crate::storage::LocalStore>,
    jobs: Arc<crate::intelligence::jobs::JobScheduler>,
    service: KnowledgeService,
    index: crate::knowledge::index::IndexOwner,
}
impl Reopened {
    fn new(h: &KnowledgeHarness, store: Arc<crate::storage::LocalStore>) -> Self {
        let jobs = Arc::new(crate::intelligence::jobs::JobScheduler::new(store.clone()));
        let config = Arc::new(crate::intelligence::config::ServiceConfigStore::new(
            h.h.io.clone(),
            store.clone(),
        ));
        let index = crate::knowledge::index::IndexOwner::new(
            h.h.temp_dir.path().join("indexes"),
            store.clone(),
        );
        let service = KnowledgeService::new(
            store.clone(),
            config.clone(),
            jobs.clone(),
            h.embeddings.clone(),
            index.clone(),
            h.h.temp_dir.path().to_path_buf(),
        );
        let documents = crate::knowledge::documents::DocumentService::new(
            store.clone(),
            config,
            jobs.clone(),
            Arc::new(FixtureParser),
            h.h.temp_dir.path().to_path_buf(),
        );
        documents.register_import_handler().unwrap();
        service.register_handlers().unwrap();
        service.register_continuation().unwrap();
        Self {
            store,
            jobs,
            service,
            index,
        }
    }
    async fn close(&self) {
        self.jobs.shutdown().await.unwrap();
        self.index.shutdown().await.unwrap();
    }
}

fn journal_keys(store: &crate::storage::LocalStore) -> Vec<String> {
    store.transaction(|tx| {
        let mut query = tx.prepare("SELECT key FROM intelligence_meta WHERE key LIKE 'knowledge.delete.v1.%' ORDER BY key")?;
        Ok(query.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?)
    }).unwrap()
}
