use super::*;
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
    mpsc as blocking,
};
use tokio::sync::{Notify, oneshot};

struct Release(Option<blocking::Sender<()>>);
impl Drop for Release {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
struct EnteredConfig {
    io: ConfigIo,
    channels: StdMutex<Option<(oneshot::Sender<()>, blocking::Receiver<()>)>>,
    abort_observer: bool,
    abort: Notify,
    observed: Notify,
    completed: Arc<AtomicBool>,
}
#[async_trait::async_trait]
impl JobHandler for EnteredConfig {
    async fn run(
        &self,
        context: JobContext,
        _: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        let token = context
            .operation_owner()
            .map_err(|_| IntelligenceErrorCode::Interrupted)?;
        let io = self.io.with_operation_owner(token);
        let (entered, release) = self.channels.lock().unwrap().take().unwrap();
        let completed = self.completed.clone();
        let observer = tokio::spawn(async move {
            io.run(move |tx| {
                let _ = entered.send(());
                release
                    .recv()
                    .map_err(|_| AppError::Internal("test release channel closed".into()))?;
                let found = tx.secret_get("child-operation", "key")?.is_some();
                completed.store(found, Ordering::Release);
                Ok(())
            })
            .await
        });
        if self.abort_observer {
            self.abort.notified().await;
            observer.abort();
            assert!(observer.await.unwrap_err().is_cancelled());
        } else {
            let result = observer.await.unwrap();
            assert!(matches!(result,
 Err(AppError::Configuration(message)) if message.contains("timed out")));
        }
        self.observed.notify_one();
        Err(IntelligenceErrorCode::Configuration)
    }
}
async fn entered(
    abort_observer: bool,
) -> (tempfile::TempDir, AppState, Arc<EnteredConfig>, Release) {
    let dir = tempfile::tempdir().unwrap();
    let fixture = crate::app::tests::fixture();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    secrets
        .set("child-operation", "key", b"synthetic-owned-child")
        .unwrap();
    let app = crate::app::tests::injected(dir.path(), &fixture, secrets.clone());
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(40));
    let io = ConfigIo::with_runtime_for_tests(
        ConfigStore::new(dir.path(), secrets).unwrap(),
        Arc::new(Semaphore::new(2)),
        Duration::from_millis(120),
    );
    let (entered, entry) = oneshot::channel();
    let (release, wait) = blocking::channel();
    let handler = Arc::new(EnteredConfig {
        io,
        channels: StdMutex::new(Some((entered, wait))),
        abort_observer,
        abort: Notify::new(),
        observed: Notify::new(),
        completed: Arc::new(AtomicBool::new(false)),
    });
    crate::intelligence::jobs::test_support::replace_pre_start_handler(
        &app.jobs,
        JobKind::Reindex,
        handler.clone(),
    )
    .unwrap();
    app.start_intelligence().await.unwrap();
    let id = Uuid::new_v4();
    let job = app
        .jobs
        .enqueue(
            JobKind::Reindex,
            None,
            Some(id),
            JobAdmission::Manual {
                exclusion_key: format!("document:{id}"),
            },
            payload(vec![]),
        )
        .await
        .unwrap();
    let release = Release(Some(release));
    tokio::time::timeout(Duration::from_secs(2), entry)
        .await
        .unwrap()
        .unwrap();
    if abort_observer {
        handler.abort.notify_one();
    }
    tokio::time::timeout(Duration::from_secs(2), handler.observed.notified())
        .await
        .unwrap();
    crate::app::tests::eventually(|| {
        app.store.get_job(job.id).unwrap().unwrap().status == JobStatus::Failed
    })
    .await;
    assert!(!handler.completed.load(Ordering::Acquire));
    (dir, app, handler, release)
}

async fn close_and_rearm(abort: bool) {
    let (_dir, app, handler, release) = entered(abort).await;
    let remained_pending = app.close_all().await.is_err();
    let rearm_denied = app.reopen_window().await.is_err();
    drop(release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while app.close_all().await.is_err() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(handler.completed.load(Ordering::Acquire));
    app.reopen_window().await.unwrap();
    app.close_all().await.unwrap();
    assert!(
        remained_pending,
        "close acknowledged an entered config operation after observer loss"
    );
    assert!(
        rearm_denied,
        "same-AppState rearm admitted while prior native work remained entered"
    );
}
async fn process_lock(abort: bool) {
    use fs2::FileExt;
    let (dir, app, handler, release) = entered(abort).await;
    let process = Arc::downgrade(&app._process_lock);
    let jobs = app.jobs.clone();
    let mut shutdown = Box::pin(jobs.shutdown());
    let remained_pending = tokio::time::timeout(Duration::from_millis(40), shutdown.as_mut())
        .await
        .is_err();
    drop(app);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.path().join("app.lock"))
        .unwrap();
    let lock_held = lock.try_lock_exclusive().is_err();
    if !lock_held {
        lock.unlock().unwrap();
    }
    drop(release);
    if remained_pending {
        tokio::time::timeout(Duration::from_secs(2), shutdown.as_mut())
            .await
            .unwrap()
            .unwrap();
    }
    drop(shutdown);
    drop(jobs);
    crate::app::tests::eventually(|| process.upgrade().is_none()).await;
    lock.try_lock_exclusive().unwrap();
    lock.unlock().unwrap();
    assert!(handler.completed.load(Ordering::Acquire));
    assert!(
        remained_pending,
        "scheduler completed before entered config work released its owner"
    );
    assert!(
        lock_held,
        "app.lock was released while entered config work was still blocked"
    );
}

// Catch replacing actual ConfigIo/native ownership with the handler/observer's
// lifetime. The callback remains entered behind a controlled release channel.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_operations_app_close_and_rearm_after_config_timeout() {
    close_and_rearm(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_operations_app_close_and_rearm_after_observer_drop() {
    close_and_rearm(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_operations_app_process_lock_after_config_timeout() {
    process_lock(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_operations_app_process_lock_after_observer_drop() {
    process_lock(true).await;
}
