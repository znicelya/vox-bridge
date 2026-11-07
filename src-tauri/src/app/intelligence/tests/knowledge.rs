use super::*;
use crate::knowledge::{
    embedding::{EmbeddingExecution, Embeddings},
    parser::DocumentParser,
    types::*,
};
use std::sync::atomic::{AtomicBool, Ordering};
struct Parser;
#[async_trait::async_trait]
impl DocumentParser for Parser {
    async fn parse(
        &self,
        path: std::path::PathBuf,
        _: DocumentKind,
        _: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
        let text = std::fs::read_to_string(path).unwrap();
        Ok(vec![TextBlock {
            location: TextLocation {
                page: None,
                paragraph: 1,
                heading: None,
                start_char: 0,
                end_char: text.chars().count() as u32,
            },
            text,
        }])
    }
}
#[derive(Default)]
struct EnteredEmbedding {
    block: AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl Embeddings for EnteredEmbedding {
    async fn embed(
        &self,
        _: &ModelProfile,
        inputs: &[String],
        _: EmbeddingExecution,
    ) -> AppResult<Vec<Vec<f32>>> {
        if self.block.load(Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(inputs
            .iter()
            .map(|_| {
                let mut v = vec![0.; 128];
                v[0] = 1.;
                v
            })
            .collect())
    }
}
async fn entered() -> (
    tempfile::TempDir,
    AppState,
    Arc<EnteredEmbedding>,
    tokio::task::JoinHandle<AppResult<KnowledgeResults>>,
) {
    let dir = tempfile::tempdir().unwrap();
    let embeddings = Arc::new(EnteredEmbedding::default());
    let app = crate::app::document_test_support::with_knowledge_ports(
        dir.path(),
        Arc::new(Parser),
        embeddings.clone(),
    );
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(40));
    let p = ModelProfile {
        kind: ServiceKind::Embedding,
        max_context_tokens: None,
        max_output_tokens: None,
        dimensions: Some(128),
        ..crate::intelligence::test_support::test_llm_profile()
    };
    app.save_model_profile(SaveModelProfile {
        profile: p.clone(),
        credential: CredentialChange::Replace {
            api_key: "synthetic".into(),
        },
    })
    .await
    .unwrap();
    let mut settings = app.get_intelligence_settings().await.unwrap();
    settings.embedding_profile_id = Some(p.id);
    app.save_intelligence_settings(settings).await.unwrap();
    app.start_intelligence().await.unwrap();
    let source = dir.path().join("pump.txt");
    std::fs::write(&source, "rated voltage24V").unwrap();
    let job = app.import_document(source).await.unwrap();
    let document = job.object_id.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if app.get_document(document).unwrap().unwrap().summary.status == DocumentStatus::Ready
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    embeddings.block.store(true, Ordering::SeqCst);
    let observer = {
        let app = app.clone();
        tokio::spawn(async move {
            app.search_knowledge(KnowledgeSearch {
                query: "voltage".into(),
                selected_segment_ids: vec![],
                session_id: None,
                limit: 5,
            })
            .await
        })
    };
    embeddings.entered.notified().await;
    (dir, app, embeddings, observer)
}
#[tokio::test]
async fn knowledge_app_close_and_rearm_wait_for_entered_search_after_observer_loss() {
    let (_dir, app, embeddings, observer) = entered().await;
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let close_pending = app.close_all().await.is_err();
    let rearm_denied = app.reopen_window().await.is_err();
    embeddings.block.store(false, Ordering::SeqCst);
    embeddings.release.notify_waiters();
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_secs(2));
    tokio::time::timeout(Duration::from_secs(3), async {
        while app.close_all().await.is_err() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(close_pending);
    assert!(rearm_denied);
    app.reopen_window().await.unwrap();
    app.close_all().await.unwrap();
}
#[tokio::test]
async fn knowledge_app_process_lock_outlives_search_observer_until_entered_work_finishes() {
    use fs2::FileExt;
    let (dir, app, embeddings, observer) = entered().await;
    let process = Arc::downgrade(&app._process_lock);
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    drop(app);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join("app.lock"))
        .unwrap();
    let held = lock.try_lock_exclusive().is_err();
    if !held {
        lock.unlock().unwrap();
    }
    embeddings.block.store(false, Ordering::SeqCst);
    embeddings.release.notify_waiters();
    tokio::time::timeout(Duration::from_secs(3), async {
        while process.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    lock.try_lock_exclusive().unwrap();
    lock.unlock().unwrap();
    assert!(
        held,
        "entered search must own physical app lock after observer loss"
    );
}
