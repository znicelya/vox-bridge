use super::*;

struct Capture {
    contexts: mpsc::Sender<JobContext>,
    release: Semaphore,
}
#[async_trait::async_trait]
impl JobHandler for Capture {
    async fn run(
        &self,
        context: JobContext,
        _: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        self.contexts.send(context).await.unwrap();
        self.release.acquire().await.unwrap().forget();
        Ok(())
    }
}
fn fixture() -> (
    IntelligenceHarness,
    Arc<Capture>,
    mpsc::Receiver<JobContext>,
) {
    let mut h = IntelligenceHarness::new();
    Arc::get_mut(&mut h.jobs).unwrap().shutdown_timeout = Duration::from_millis(40);
    let (contexts, receiver) = mpsc::channel(2);
    let handler = Arc::new(Capture {
        contexts,
        release: Semaphore::new(0),
    });
    h.jobs.register(JobKind::Import, handler.clone()).unwrap();
    (h, handler, receiver)
}
async fn context(h: &IntelligenceHarness, receiver: &mut mpsc::Receiver<JobContext>) -> JobContext {
    enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap()
}

// Catches per-job rather than per-generation capacity and releasing a slot before
// the last clone (which would allow more native work than the advertised bound).
#[tokio::test]
async fn child_operations_share_sixteen_slots_and_hold_each_until_final_clone_drop() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let first = context(&h, &mut receiver).await;
    let second = context(&h, &mut receiver).await;
    let mut tokens: Vec<_> = (0..16).map(|_| first.operation_owner().unwrap()).collect();
    assert!(matches!(
        second.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    let final_clone = tokens.pop().unwrap();
    let another_clone = final_clone.clone();
    drop(final_clone);
    assert!(matches!(
        second.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    drop(another_clone);
    let replacement = second.operation_owner().unwrap();
    drop((tokens, replacement, first, second));
    handler.release.add_permits(2);
    h.jobs.shutdown().await.unwrap();
}

// Catches admitting new work after per-job cancellation or the immediate close
// fence, including when the underlying process lease is still alive.
#[tokio::test]
async fn child_operations_reject_cancelled_and_closed_jobs() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let cancelled_context = context(&h, &mut receiver).await;
    let open_context = context(&h, &mut receiver).await;
    h.jobs.cancel(cancelled_context.id).await.unwrap();
    assert!(matches!(
        cancelled_context.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Cancelled))
    ));
    h.jobs.signal_shutdown();
    assert!(matches!(
        open_context.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    handler.release.add_permits(2);
    h.jobs.shutdown().await.unwrap();
}

// Catches a retained successful JobContext admitting new native work after the
// worker removed it from the active map, while the generation remains open.
#[tokio::test]
async fn child_operations_reject_terminal_context_in_open_generation() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let context = context(&h, &mut receiver).await;
    handler.release.add_permits(1);
    status(&h, context.id, JobStatus::Succeeded).await;
    assert!(matches!(
        context.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Cancelled))
    ));
    h.jobs.shutdown().await.unwrap();
}

// Catches acknowledging shutdown/retiring the generation when the handler has
// returned but its accepted native operation still owns a token.
#[tokio::test]
async fn child_operations_drain_after_handler_completion_before_process_release_and_rearm() {
    let (h, handler, mut receiver) = fixture();
    let process: OperationLease = Arc::new(());
    let weak = Arc::downgrade(&process);
    h.jobs.start_owned(process).unwrap();
    let context = context(&h, &mut receiver).await;
    let id = context.id;
    let token = context.operation_owner().unwrap();
    drop(context);
    handler.release.add_permits(1);
    status(&h, id, JobStatus::Succeeded).await;
    assert!(
        h.jobs.shutdown().await.is_err(),
        "shutdown acknowledged a still-owned native operation"
    );
    assert!(h.jobs.start().is_err());
    assert!(weak.upgrade().is_some());
    drop(token);
    h.jobs.shutdown().await.unwrap();
    assert!(
        weak.upgrade().is_none(),
        "completion preceded actual background process-lease release"
    );
    h.jobs.start().unwrap();
    h.jobs.shutdown().await.unwrap();
}

// Catches an old context being admitted using the shared reopened closed flag,
// and an already-pending old shutdown observer retiring the next generation.
#[tokio::test]
async fn child_operations_old_context_and_observer_cannot_affect_rearmed_generation() {
    let (h, handler, mut receiver) = fixture();
    let app_process: OperationLease = Arc::new(());
    h.jobs.start_owned(app_process.clone()).unwrap();
    let old_context = context(&h, &mut receiver).await;
    let token = old_context.operation_owner().unwrap();
    let mut old_observer = Box::pin(h.jobs.shutdown());
    assert!(futures_util::poll!(old_observer.as_mut()).is_pending());
    handler.release.add_permits(1);
    drop(token);
    h.jobs.shutdown().await.unwrap();
    h.jobs.start_owned(app_process.clone()).unwrap();
    old_observer.await.unwrap();
    assert!(matches!(
        old_context.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    let new_context = context(&h, &mut receiver).await;
    let new_token = new_context.operation_owner().unwrap();
    drop((new_context, new_token));
    handler.release.add_permits(1);
    h.jobs.shutdown().await.unwrap();
}

// Catches putting a strong process lease in tracker/context clones rather than
// only in the live generation and issued operation tokens.
#[tokio::test]
async fn child_operations_retained_context_does_not_keep_process_after_generation_done() {
    let (h, handler, mut receiver) = fixture();
    let process: OperationLease = Arc::new(());
    let weak = Arc::downgrade(&process);
    h.jobs.start_owned(process).unwrap();
    let context = context(&h, &mut receiver).await;
    handler.release.add_permits(1);
    status(&h, context.id, JobStatus::Succeeded).await;
    h.jobs.shutdown().await.unwrap();
    assert!(weak.upgrade().is_none());
    assert!(matches!(
        context.operation_owner(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
}

// Catches leaking a reserved slot when the weak process upgrade fails.
#[test]
fn child_operations_failed_process_upgrade_rolls_back_the_reserved_slot() {
    let process: OperationLease = Arc::new(());
    let tracker = ChildOperations::new(&process);
    drop(process);
    for _ in 0..17 {
        assert!(matches!(
            tracker.try_acquire(),
            Err(AppError::Intelligence(IntelligenceErrorCode::Interrupted))
        ));
    }
}

// Catches releasing the slot before the token's actual process owner drops.
// A controlled process destructor makes that order observable without sleeps.
#[tokio::test]
async fn child_operations_release_process_before_publishing_slot_completion() {
    struct Process {
        entered: Option<tokio::sync::oneshot::Sender<()>>,
        release: std::sync::mpsc::Receiver<()>,
    }
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.entered.take().unwrap().send(());
            let _ = self.release.recv();
        }
    }
    struct Release(Option<std::sync::mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    // Mutex makes the test process owner Sync without changing its destructor.
    let process: OperationLease = Arc::new(Mutex::new(Process {
        entered: Some(entered),
        release: wait,
    }));
    let tracker = ChildOperations::new(&process);
    let token = tracker.try_acquire().unwrap();
    drop(process);
    let release = Release(Some(release));
    let dropper = std::thread::spawn(move || drop(token));
    tokio::time::timeout(Duration::from_secs(2), entry)
        .await
        .unwrap()
        .unwrap();
    let mut drain = Box::pin(tracker.drain());
    let remained_pending = futures_util::poll!(drain.as_mut()).is_pending();
    drop(release);
    dropper.join().unwrap();
    if remained_pending {
        drain.await;
    }
    assert!(
        remained_pending,
        "slot completion preceded the process owner destructor"
    );
}
