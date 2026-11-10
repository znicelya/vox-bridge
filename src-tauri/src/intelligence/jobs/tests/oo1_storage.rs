use super::*;
use std::{
    future::{Future, poll_fn},
    sync::mpsc as thread_channel,
    task::Poll,
    time::Instant,
};
use tokio::sync::oneshot;

struct HeldHandler {
    contexts: mpsc::Sender<JobContext>,
    ended: thread_channel::Sender<()>,
    release: Semaphore,
    interrupted: AtomicUsize,
    cancelled: AtomicUsize,
}
#[async_trait::async_trait]
impl JobHandler for HeldHandler {
    async fn run(
        &self,
        context: JobContext,
        _: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        self.contexts.send(context).await.unwrap();
        self.release.acquire().await.unwrap().forget();
        let _ = self.ended.send(());
        Ok(())
    }
    fn terminal_tx(
        &self,
        _: &rusqlite::Transaction<'_>,
        _: Uuid,
        status: JobStatus,
        _: Option<IntelligenceErrorCode>,
        _: bool,
    ) -> AppResult<()> {
        match status {
            JobStatus::Interrupted => {
                self.interrupted.fetch_add(1, Ordering::SeqCst);
            }
            JobStatus::Cancelled => {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        Ok(())
    }
}
fn fixture() -> (
    IntelligenceHarness,
    Arc<HeldHandler>,
    mpsc::Receiver<JobContext>,
    thread_channel::Receiver<()>,
    std::sync::Weak<std::fs::File>,
) {
    let mut h = IntelligenceHarness::new();
    Arc::get_mut(&mut h.jobs).unwrap().shutdown_timeout = Duration::from_millis(30);
    let (contexts, receiver) = mpsc::channel(2);
    let (ended, finished) = thread_channel::channel();
    let handler = Arc::new(HeldHandler {
        contexts,
        ended,
        release: Semaphore::new(0),
        interrupted: AtomicUsize::new(0),
        cancelled: AtomicUsize::new(0),
    });
    h.jobs.register(JobKind::Import, handler.clone()).unwrap();
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(h.temp_dir.path().join("oo1-scheduler-process.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    let process = Arc::new(file);
    let weak = Arc::downgrade(&process);
    h.jobs.start_owned(process).unwrap();
    (h, handler, receiver, finished, weak)
}

// Two real, nonempty original handlers end while SQLite is held. Their
// terminal writes must yield just as empty claims do, then actually join.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonempty_worker_terminal_sql_keeps_timers_pollable_and_joins_before_retirement() {
    let (h, handler, mut contexts, ended, process) = fixture();
    let first = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    let second = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    let a = contexts.recv().await.unwrap();
    let b = contexts.recv().await.unwrap();
    assert_ne!(a.id, b.id);
    let count = Arc::new(AtomicUsize::new(0));
    let (stop, mut stopping) = watch::channel(false);
    let (ready, waiting) = oneshot::channel();
    let heartbeat = tokio::spawn({
        let count = count.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(25));
            let mut ready = Some(ready);
            loop {
                tokio::select! {
                    biased;
                    _ = stopping.changed() => break,
                    _ = tick.tick() => {
                        count.fetch_add(1, Ordering::SeqCst);
                        if let Some(ready) = ready.take() { let _ = ready.send(()); }
                    }
                }
            }
        }
    });
    waiting.await.unwrap();
    let (done, observed) = oneshot::channel();
    let blocker = std::thread::spawn({
        let store = h.store.clone();
        let handler = handler.clone();
        let count = count.clone();
        move || {
            let result = store.transaction(|_| {
                handler.release.add_permits(2);
                let returned = ended.recv_timeout(Duration::from_secs(2)).is_ok()
                    && ended.recv_timeout(Duration::from_secs(2)).is_ok();
                let start = Instant::now();
                let before = count.load(Ordering::SeqCst);
                let mut samples = Vec::new();
                for at in [150, 300, 450] {
                    std::thread::sleep(
                        (start + Duration::from_millis(at))
                            .saturating_duration_since(Instant::now()),
                    );
                    samples.push(count.load(Ordering::SeqCst));
                }
                Ok((returned, before, samples))
            });
            let _ = done.send(());
            result
        }
    });
    observed.await.unwrap();
    let (returned, before, samples) = blocker.join().unwrap().unwrap();
    status(&h, first.id, JobStatus::Succeeded).await;
    status(&h, second.id, JobStatus::Succeeded).await;
    let stopped = h.jobs.shutdown().await;
    stop.send_replace(true);
    let heartbeat_joined = heartbeat.await.is_ok();
    assert!(returned && heartbeat_joined && stopped.is_ok());
    assert!(process.upgrade().is_none());
    assert!(
        samples[0] > before && samples.windows(2).all(|pair| pair[1] > pair[0]),
        "nonempty worker terminal SQL starved the original two-worker executor: {samples:?}"
    );
}

// Repeated/dropped shutdown observations share actual queued/interruption
// work; an old observer cannot interrupt a later generation after real joins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn held_sql_shutdown_observer_loss_coalesces_durable_work_and_fences_old_generation() {
    let (h, handler, mut contexts, _ended, process) = fixture();
    let first = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    let second = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    let a = contexts.recv().await.unwrap();
    let b = contexts.recv().await.unwrap();
    let queued = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    assert_eq!(
        h.store.get_job(queued.id).unwrap().unwrap().status,
        JobStatus::Queued
    );
    let held = Arc::new(AtomicBool::new(false));
    let (entered, waiting) = oneshot::channel();
    let (done, released) = oneshot::channel();
    let blocker = std::thread::spawn({
        let store = h.store.clone();
        let held = held.clone();
        move || {
            let result = store.transaction(|_| {
                held.store(true, Ordering::Release);
                let _ = entered.send(());
                std::thread::sleep(Duration::from_millis(450));
                held.store(false, Ordering::Release);
                Ok(())
            });
            let _ = done.send(());
            result
        }
    });
    waiting.await.unwrap();
    let mut lost = Box::pin(h.jobs.shutdown());
    let lost_pending = poll_fn(|cx| Poll::Ready(lost.as_mut().poll(cx)))
        .await
        .is_pending();
    drop(lost);
    let mut late = Box::pin(h.jobs.shutdown());
    let late_pending = poll_fn(|cx| Poll::Ready(late.as_mut().poll(cx)))
        .await
        .is_pending();
    let fenced = *a.cancel.borrow()
        && *b.cancel.borrow()
        && a.operation_owner().is_err()
        && b.operation_owner().is_err();
    let mut observations = Vec::new();
    for _ in 0..3 {
        let result = h.jobs.shutdown().await;
        observations.push((
            result.is_err(),
            held.load(Ordering::Acquire),
            process.upgrade().is_some(),
        ));
    }
    released.await.unwrap();
    blocker.join().unwrap().unwrap();
    status(&h, first.id, JobStatus::Interrupted).await;
    status(&h, second.id, JobStatus::Interrupted).await;
    status(&h, queued.id, JobStatus::Cancelled).await;
    let no_rearm = h.jobs.start().is_err();
    handler.release.add_permits(2);
    h.jobs.shutdown().await.unwrap();
    let prior_released = process.upgrade().is_none();
    h.jobs.start().unwrap();
    let new = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    let current = contexts.recv().await.unwrap();
    let old_result = late.await;
    let new_status = h.store.get_job(new.id).unwrap().unwrap().status;
    h.jobs.request_shutdown().unwrap();
    handler.release.add_permits(1);
    h.jobs.shutdown().await.unwrap();
    assert!(lost_pending && late_pending && fenced && no_rearm && prior_released);
    assert!(
        observations
            .iter()
            .all(|&(error, held, retained)| error && held && retained),
        "held SQLite stalled a shutdown observer or lost actual ownership: {observations:?}"
    );
    assert_eq!(handler.interrupted.load(Ordering::SeqCst), 2);
    assert_eq!(handler.cancelled.load(Ordering::SeqCst), 2);
    assert!(old_result.is_ok());
    assert_eq!(current.id, new.id);
    assert_eq!(new_status, JobStatus::Running);
}

// Abort one original worker while its actual claim waits on SQLite. The
// generation must retain and drain that genuine handle, then fail closed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_worker_loss_retains_claim_sql_and_never_publishes_joined_or_rearm() {
    let mut h = IntelligenceHarness::new();
    Arc::get_mut(&mut h.jobs).unwrap().shutdown_timeout = Duration::from_millis(30);
    let handler = handler();
    h.jobs.register(JobKind::Import, handler.clone()).unwrap();
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(h.temp_dir.path().join("oo1-aborted-worker.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    let process = Arc::new(file);
    let weak = Arc::downgrade(&process);
    let (entered, waiting) = oneshot::channel();
    let (done, released) = oneshot::channel();
    let held = Arc::new(AtomicBool::new(false));
    let blocker = std::thread::spawn({
        let store = h.store.clone();
        let held = held.clone();
        move || {
            let result = store.transaction(|_| {
                held.store(true, Ordering::Release);
                let _ = entered.send(());
                std::thread::sleep(Duration::from_millis(450));
                held.store(false, Ordering::Release);
                Ok(())
            });
            let _ = done.send(());
            result
        }
    });
    waiting.await.unwrap();
    let (claims, issued) = oneshot::channel();
    let claims = Arc::new(Mutex::new(Some(claims)));
    let count = Arc::new(AtomicUsize::new(0));
    *h.jobs.oo1_claim_probe.lock().unwrap() = Some(Arc::new(move |phase, _| {
        if phase == "before_sql_lock" && count.fetch_add(1, Ordering::SeqCst) == 1 {
            if let Some(claims) = claims.lock().unwrap().take() {
                let _ = claims.send(());
            }
        }
    }));
    h.jobs.start_owned(process).unwrap();
    let both_issued = matches!(
        tokio::time::timeout(Duration::from_secs(2), issued).await,
        Ok(Ok(()))
    );
    let abort = h.jobs.oo1_workers.lock().unwrap()[0].clone();
    abort.abort();
    let observer = h.jobs.shutdown().await;
    let pending_owned =
        observer.is_err() && held.load(Ordering::Acquire) && weak.upgrade().is_some();
    released.await.unwrap();
    blocker.join().unwrap().unwrap();
    let final_observer = h.jobs.shutdown().await;
    let actually_released = tokio::time::timeout(Duration::from_secs(2), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    let joined_published = *h
        .jobs
        .state
        .lock()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .completion
        .borrow();
    assert!(both_issued && pending_owned && actually_released && abort.is_finished());
    assert!(final_observer.is_err() && !joined_published && h.jobs.start().is_err());
    assert_eq!(handler.calls.load(Ordering::SeqCst), 0);
}
