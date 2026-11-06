//! Real binary through the actual admitted document handler and shared parser gate.
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, watch};
use vox_bridge_lib::{
    app::AppState,
    intelligence::types::{IntelligenceErrorCode, JobStatus},
    knowledge::{
        parser::{DocumentParser, ProcessDocumentParser},
        types::{DocumentKind, DocumentStatus, TextBlock},
    },
};
struct Parser {
    inner: ProcessDocumentParser,
    calls: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    release: Arc<Notify>,
}
struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[async_trait::async_trait]
impl DocumentParser for Parser {
    async fn parse(
        &self,
        path: PathBuf,
        kind: DocumentKind,
        cancel: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, IntelligenceErrorCode> {
        let ordinal = self.calls.fetch_add(1, Ordering::SeqCst);
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        let _active = Active(self.active.clone());
        if ordinal == 0 {
            self.release.notified().await;
        }
        self.inner.parse(path, kind, cancel).await
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_document_service_two_admitted_imports_share_one_actual_worker_permit() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let parser = Arc::new(Parser {
        inner: ProcessDocumentParser::with_executable(env!("CARGO_BIN_EXE_vox-bridge").into()),
        calls: calls.clone(),
        active: Arc::default(),
        peak: peak.clone(),
        release: release.clone(),
    });
    let app = AppState::with_document_parser(dir.path().into(), parser).unwrap();
    app.start_intelligence().await.unwrap();
    let (one, two) = tokio::join!(
        app.import_document(super::fixture("unicode.txt")),
        app.import_document(super::fixture("two-pages.pdf"))
    );
    let one = one.unwrap();
    let two = two.unwrap();
    let admitted = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if [one.id, two.id]
                .into_iter()
                .all(|id| app.get_job(id).unwrap().unwrap().status != JobStatus::Queued)
                && calls.load(Ordering::SeqCst) > 0
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    let initial_calls = calls.load(Ordering::SeqCst);
    release.notify_one();
    let finished = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if [one.id, two.id].into_iter().all(|id| {
                !matches!(
                    app.get_job(id).unwrap().unwrap().status,
                    JobStatus::Queued | JobStatus::Running
                )
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    app.close_all().await.unwrap();
    assert!(admitted.is_ok());
    assert!(finished.is_ok());
    assert_eq!(
        initial_calls, 1,
        "the second admitted worker waits for the same sole parser permit"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(peak.load(Ordering::SeqCst), 1);
    for job in [one, two] {
        let detail = app.get_document(job.object_id.unwrap()).unwrap().unwrap();
        assert_eq!(detail.summary.status, DocumentStatus::Parsed);
        assert!(detail.summary.chunk_count > 0);
        assert_eq!(detail.index_identity, None);
        let progress = app.get_job(job.id).unwrap().unwrap();
        assert_eq!(progress.completed_units, detail.summary.chunk_count);
        assert_eq!(progress.total_units, Some(detail.summary.chunk_count));
    }
    assert!(!dir.path().join("secrets.json").exists());
    assert!(!dir.path().join("provider-configs.json").exists());
}
