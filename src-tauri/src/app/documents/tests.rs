use super::*;
use crate::intelligence::types::{IntelligenceErrorCode as Code, JobStatus};
use crate::knowledge::{
    copying::{CopiedFile, CopyRequest, FileCopier, ManagedCopier},
    parser::DocumentParser,
    types::{DocumentKind, DocumentStatus, TextBlock, TextLocation},
};
use std::{
    sync::{
        Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

struct Parser;
#[async_trait::async_trait]
impl DocumentParser for Parser {
    async fn parse(
        &self,
        path: PathBuf,
        _: DocumentKind,
        _: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code> {
        let text = std::fs::read_to_string(path).map_err(|_| Code::ParseFailed)?;
        let end_char = text.chars().count() as u32;
        Ok(vec![TextBlock {
            text,
            location: TextLocation {
                page: None,
                paragraph: 1,
                heading: None,
                start_char: 0,
                end_char,
            },
        }])
    }
}
fn document_app(path: &Path, parser: Arc<dyn DocumentParser>) -> AppState {
    let secrets = Arc::new(SecretStore::for_tests(path).unwrap());
    let fixture = crate::app::tests::fixture();
    AppState::compose(
        path.into(),
        AppState::lock_data_dir(path).unwrap(),
        secrets.clone(),
        fixture.audio.clone(),
        Arc::new(ProviderFactory::new(secrets)),
        Arc::new(UnavailableDriverOperations),
        parser,
    )
    .unwrap()
}
async fn status(app: &AppState, id: Uuid, want: JobStatus) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if app.get_job(id).unwrap().unwrap().status == want {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn source(path: &Path, name: &str) -> PathBuf {
    let path = path.join(name);
    std::fs::write(&path, name).unwrap();
    path
}

#[tokio::test]
async fn documents_app_queued_cancel_updates_domain_and_cleans_only_candidate_without_parser() {
    let dir = tempfile::tempdir().unwrap();
    let app = document_app(dir.path(), Arc::new(Parser));
    app.start_intelligence().await.unwrap();
    let permit = app.jobs.parser_gate().acquire_owned().await.unwrap();
    let mut jobs = Vec::new();
    for name in ["one.txt", "two.txt", "three.txt"] {
        jobs.push(app.import_document(source(dir.path(), name)).await.unwrap());
    }
    status(&app, jobs[0].id, JobStatus::Running).await;
    status(&app, jobs[1].id, JobStatus::Running).await;
    let before = app
        .get_document(jobs[2].object_id.unwrap())
        .unwrap()
        .unwrap();
    let managed = before.managed_path.unwrap();
    assert_eq!(
        app.cancel_job(jobs[2].id).await.unwrap().status,
        JobStatus::Cancelled
    );
    let after = app
        .get_document(jobs[2].object_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(after.summary.status, DocumentStatus::Failed);
    assert_eq!(after.summary.error, Some(Code::Cancelled));
    assert_eq!(after.managed_path, None);
    assert!(!managed.exists());
    assert!(dir.path().join("three.txt").exists());
    assert_eq!(
        app.store
            .transaction(|tx| Ok(tx.query_row(
                "SELECT count(*) FROM knowledge_managed_files",
                [],
                |r| r.get::<_, u32>(0)
            )?))
            .unwrap(),
        2
    );
    app.close_all().await.unwrap();
    drop(permit);
}

#[tokio::test]
async fn documents_app_restart_reconciles_already_terminal_association_without_replaying_parser() {
    let dir = tempfile::tempdir().unwrap();
    let app = document_app(dir.path(), Arc::new(Parser));
    app.start_intelligence().await.unwrap();
    let permit = app.jobs.parser_gate().acquire_owned().await.unwrap();
    let job = app
        .import_document(source(dir.path(), "saved.txt"))
        .await
        .unwrap();
    status(&app, job.id, JobStatus::Running).await;
    app.store.transaction(|tx|{tx.execute("UPDATE intelligence_jobs SET status='cancelled',error_code='cancelled' WHERE id=?1",[job.id.to_string()])?;Ok(())}).unwrap();
    app.close_all().await.unwrap();
    drop(permit);
    drop(app);
    let reopened = document_app(dir.path(), Arc::new(Parser));
    let detail = reopened
        .get_document(job.object_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        detail.summary.status,
        DocumentStatus::Failed,
        "terminal job reconciliation cannot leave Parsing referencing deleted candidate"
    );
    assert_eq!(detail.managed_path, None);
    assert_eq!(detail.summary.error, Some(Code::Cancelled));
    assert!(reopened.get_job(job.id).unwrap().unwrap().status == JobStatus::Cancelled);
    assert_eq!(
        reopened
            .store
            .transaction(|tx| Ok(tx.query_row(
                "SELECT count(*) FROM knowledge_managed_files",
                [],
                |r| r.get::<_, u32>(0)
            )?))
            .unwrap(),
        0
    );
}

struct LateCopy {
    entered: Arc<Notify>,
    release: StdMutex<std::sync::mpsc::Receiver<()>>,
}
impl FileCopier for LateCopy {
    fn copy(&self, request: CopyRequest) -> AppResult<CopiedFile> {
        let copy = ManagedCopier.copy(request)?;
        self.entered.notify_one();
        let _ = self.release.lock().unwrap().recv();
        Ok(copy)
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn documents_app_copy_observer_loss_retains_process_lock_and_prevents_late_admission() {
    use fs2::FileExt;
    for abort in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = document_app(dir.path(), Arc::new(Parser));
        let entered = Arc::new(Notify::new());
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let docs = Arc::get_mut(&mut app.documents).unwrap();
        docs.copier = Arc::new(LateCopy {
            entered: entered.clone(),
            release: StdMutex::new(release_rx),
        });
        docs.observation_timeout = Duration::from_millis(20);
        app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(20));
        app.start_intelligence().await.unwrap();
        let path = source(dir.path(), "source.txt");
        let owner = app.clone();
        let command = tokio::spawn(async move { owner.import_document(path).await });
        entered.notified().await;
        if abort {
            command.abort();
            assert!(command.await.unwrap_err().is_cancelled());
        } else {
            assert!(matches!(
                command.await.unwrap(),
                Err(AppError::Intelligence(Code::Timeout))
            ));
        }
        assert!(app.close_all().await.is_err());
        assert!(app.open_window().is_err());
        assert!(app.list_documents(None, 64).unwrap().rows.is_empty());
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join("app.lock"))
            .unwrap();
        assert!(lock.try_lock_exclusive().is_err());
        drop(app);
        assert!(
            lock.try_lock_exclusive().is_err(),
            "entered copy keeps actual app.lock despite dropped command/AppState observers"
        );
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if lock.try_lock_exclusive().is_ok() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        lock.unlock().unwrap();
        let store = LocalStore::open(dir.path()).unwrap();
        assert!(store.list_jobs(None, 200).unwrap().rows.is_empty());
        assert_eq!(
            store
                .transaction(|tx| Ok(tx.query_row(
                    "SELECT count(*) FROM knowledge_managed_files",
                    [],
                    |r| r.get::<_, u32>(0)
                )?))
                .unwrap(),
            0
        );
        assert!(dir.path().join("source.txt").exists());
    }
}

struct ReapParser {
    entered: Arc<Notify>,
    cancelled: Arc<Notify>,
    reap: Arc<Notify>,
    calls: Arc<AtomicUsize>,
}
#[async_trait::async_trait]
impl DocumentParser for ReapParser {
    async fn parse(
        &self,
        path: PathBuf,
        _: DocumentKind,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        crate::intelligence::http::cancelled(&mut cancel).await;
        self.cancelled.notify_one();
        self.reap.notified().await;
        Parser
            .parse(path, DocumentKind::Text, watch::channel(false).1)
            .await
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn documents_app_parser_permit_and_generation_live_until_reap_and_late_success_cannot_commit()
{
    let dir = tempfile::tempdir().unwrap();
    let entered = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let reap = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let app = document_app(
        dir.path(),
        Arc::new(ReapParser {
            entered: entered.clone(),
            cancelled: cancelled.clone(),
            reap: reap.clone(),
            calls: calls.clone(),
        }),
    );
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(20));
    app.start_intelligence().await.unwrap();
    let first = app
        .import_document(source(dir.path(), "first.txt"))
        .await
        .unwrap();
    entered.notified().await;
    let second = app
        .import_document(source(dir.path(), "second.txt"))
        .await
        .unwrap();
    status(&app, second.id, JobStatus::Running).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(app.jobs.parser_gate().try_acquire_owned().is_err());
    assert!(app.close_all().await.is_err());
    cancelled.notified().await;
    assert!(app.jobs.parser_gate().try_acquire_owned().is_err());
    assert!(app.open_window().is_err());
    reap.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if app.close_all().await.is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(app.jobs.parser_gate().try_acquire_owned().is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for job in [first, second] {
        let detail = app.get_document(job.object_id.unwrap()).unwrap().unwrap();
        assert_eq!(detail.summary.status, DocumentStatus::Failed);
        assert_eq!(detail.summary.chunk_count, 0);
        assert_eq!(detail.managed_path, None);
        assert_ne!(
            app.get_job(job.id).unwrap().unwrap().status,
            JobStatus::Succeeded
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn documents_completion_cleanup_error_retains_intent_and_marks_repair_without_following_symlink()
 {
    let dir = tempfile::tempdir().unwrap();
    let app = document_app(dir.path(), Arc::new(Parser));
    app.start_intelligence().await.unwrap();
    let permit = app.jobs.parser_gate().acquire_owned().await.unwrap();
    let mut jobs = Vec::new();
    for name in ["first.txt", "second.txt", "third.txt"] {
        jobs.push(app.import_document(source(dir.path(), name)).await.unwrap());
    }
    status(&app, jobs[0].id, JobStatus::Running).await;
    status(&app, jobs[1].id, JobStatus::Running).await;
    let managed = dir.path().join("knowledge/documents");
    let saved = dir.path().join("knowledge/retained-documents");
    let foreign = tempfile::tempdir().unwrap();
    std::fs::write(foreign.path().join("keep.txt"), b"foreign").unwrap();
    std::fs::rename(&managed, &saved).unwrap();
    std::os::unix::fs::symlink(foreign.path(), &managed).unwrap();
    let outcome = app.cancel_job(jobs[2].id).await;
    let detail = app
        .get_document(jobs[2].object_id.unwrap())
        .unwrap()
        .unwrap();
    let state: String = app
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT state FROM knowledge_managed_files WHERE job_id=?1",
                [jobs[2].id.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    // Restore before assertions so even RED cannot retain the shutdown owner forever.
    std::fs::remove_file(&managed).unwrap();
    std::fs::rename(&saved, &managed).unwrap();
    app.cancel_job(jobs[2].id).await.unwrap();
    app.close_all().await.unwrap();
    drop(permit);
    assert!(
        matches!(outcome, Err(AppError::Intelligence(Code::Storage))),
        "ordinary cleanup outcome={outcome:?}, pending={state}, document={:?}/{:?}",
        detail.summary.status,
        detail.summary.error
    );
    assert_eq!(state, "delete_pending");
    assert_eq!(
        detail.summary.status,
        DocumentStatus::RepairRequired,
        "failed cleanup must remain a visible repair state"
    );
    assert_eq!(detail.summary.error, Some(Code::Storage));
    assert_eq!(
        std::fs::read(foreign.path().join("keep.txt")).unwrap(),
        b"foreign"
    );
}
