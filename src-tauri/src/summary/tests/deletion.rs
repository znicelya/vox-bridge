use super::harness::*;
use crate::{
    error::AppError,
    intelligence::{jobs::OperationLease, types::*},
    summary::operations::AttemptRegistry,
};
use std::{
    fs::File,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use uuid::Uuid;

#[tokio::test]
async fn summary_delete_ten_second_timeout_retains_fence_after_actual_release_until_explicit_retry()
{
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["owned full input"]).await;
    h.llm.block.store(true, Ordering::SeqCst);
    let job = h.generate(session, None).await.unwrap();
    h.llm_entered().await;
    let id = job.object_id.unwrap();
    let began = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(12), h.delete(id)).await;
    let timed_out = matches!(
        result,
        Ok(Err(AppError::Intelligence(IntelligenceErrorCode::Timeout)))
    );
    let elapsed = began.elapsed();
    let before = h.native_versions(session).unwrap();
    h.llm.block.store(false, Ordering::SeqCst);
    h.llm.release.notify_one();
    h.wait(job.id).await;
    h.service.drain_actual().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = h.native_versions(session).unwrap();
    let raw = h.raw_responses(id).unwrap();
    h.delete(id).await.unwrap();
    let next = h.generate(session, None).await.unwrap();
    let next_done = h.wait(next.id).await;
    let versions = h.native_versions(session).unwrap();
    h.close().await;
    assert!(
        timed_out,
        "real 10s observation must return Timeout while the entered owner remains live"
    );
    assert!(elapsed >= Duration::from_secs(9) && elapsed < Duration::from_secs(12));
    assert_eq!(before.len(), 1);
    assert!(before[0].deleting);
    assert_eq!(after.len(), 1);
    assert!(after[0].deleting);
    assert!(after[0].content.is_none());
    assert!(raw.is_empty());
    assert_eq!(next_done.status, JobStatus::Succeeded);
    assert_eq!(versions[0].version, 2);
    assert!(h.store.get_job(job.id).unwrap().is_some());
}

#[tokio::test]
async fn summary_delete_dropped_observer_keeps_owned_cleanup_and_automatic_existing_counter_marker()
{
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["automatic admitted input"]).await;
    h.llm.block.store(true, Ordering::SeqCst);
    let job = h.create_stored(session, true).unwrap();
    h.jobs.wake();
    h.llm_entered().await;
    let id = job.object_id.unwrap();
    let mut deletion = Box::pin(h.delete(id));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), deletion.as_mut())
            .await
            .is_err()
    );
    drop(deletion);
    let fenced = h.native_versions(session).unwrap();
    h.llm.block.store(false, Ordering::SeqCst);
    h.llm.release.notify_one();
    h.wait(job.id).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while h.store.count_summaries(session).unwrap() != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let existing = h.create_stored(session, true).unwrap();
    let counter = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT last_version FROM summary_version_counters WHERE session_id=?1",
                [session.to_string()],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    h.close().await;
    assert_eq!(fenced.len(), 1);
    assert!(fenced[0].deleting);
    assert_eq!(existing.id, job.id);
    assert_eq!(counter, 1);
    assert_eq!(h.llm.calls.lock().unwrap().len(), 1);
    assert!(h.raw_responses(id).unwrap().is_empty());
}

#[tokio::test]
async fn summary_session_guard_fences_all_versions_waits_actual_work_and_rechecks_matching_session()
{
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["session full input"]).await;
    let foreign = h.completed_session(&["unmodified foreign"]).await;
    h.llm.block.store(true, Ordering::SeqCst);
    let job = h.generate(session, None).await.unwrap();
    h.llm_entered().await;
    let mut prepare = Box::pin(
        h.service
            .prepare_session_delete_owned(session, h.process.clone()),
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), prepare.as_mut())
            .await
            .is_err()
    );
    let flags = h
        .store
        .transaction(|tx| {
            Ok((
                tx.query_row(
                    "SELECT summary_session_deleting FROM session_lifecycle WHERE session_id=?1",
                    [session.to_string()],
                    |r| r.get::<_, i64>(0),
                )?,
                tx.query_row(
                    "SELECT deleting FROM session_summaries WHERE id=?1",
                    [job.object_id.unwrap().to_string()],
                    |r| r.get::<_, i64>(0),
                )?,
            ))
        })
        .unwrap();
    assert!(matches!(
        h.generate(session, None).await,
        Err(AppError::Intelligence(IntelligenceErrorCode::Cancelled))
    ));
    h.llm.block.store(false, Ordering::SeqCst);
    h.llm.release.notify_one();
    let guard = prepare.await.unwrap();
    assert!(matches!(
        guard.delete_owned(foreign).await,
        Err(AppError::Intelligence(IntelligenceErrorCode::Storage))
    ));
    assert!(h.store.get_session(session).unwrap().is_some());
    assert!(h.store.get_session(foreign).unwrap().is_some());
    let guard = h
        .service
        .prepare_session_delete_owned(session, h.process.clone())
        .await
        .unwrap();
    guard.delete_owned(session).await.unwrap();
    h.close().await;
    assert_eq!(flags, (1, 1));
    assert!(h.store.get_session(session).unwrap().is_none());
    assert!(h.store.get_session(foreign).unwrap().is_some());
    assert!(h.store.get_job(job.id).unwrap().is_none());
    assert!(h.raw_responses(job.object_id.unwrap()).unwrap().is_empty());
}

struct RegistryOwner {
    _file: Arc<File>,
    _slot: tokio::sync::OwnedSemaphorePermit,
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_session_guard_sql_timeout_retains_actual_owner_and_fence_until_retry() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["retained session input"]).await;
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(h.dir.path().join("session-delete-owner.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let owner: OperationLease = Arc::new(RegistryOwner {
        _file: Arc::new(file),
        _slot: slots.clone().try_acquire_owned().unwrap(),
    });
    let weak = Arc::downgrade(&owner);
    let guard = h
        .service
        .prepare_session_delete_owned(session, owner)
        .await
        .unwrap();
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel::<()>();
    let blocked_store = h.store.clone();
    let blocker = std::thread::spawn(move || {
        blocked_store.transaction(|_| {
            let _ = entered.send(());
            let _ = gate.recv();
            Ok(())
        })
    });
    tokio::time::timeout(Duration::from_secs(2), entry)
        .await
        .unwrap()
        .unwrap();
    let mut deletion = Box::pin(guard.delete_owned(session));
    let began = std::time::Instant::now();
    let observed = tokio::time::timeout(Duration::from_secs(11), deletion.as_mut()).await;
    let still_waiting = observed.is_err();
    let observed_timeout = matches!(
        observed,
        Ok(Err(AppError::Intelligence(IntelligenceErrorCode::Timeout)))
    );
    let elapsed = began.elapsed();
    let retained = weak.upgrade().is_some();
    let available = slots.available_permits();
    drop(release);
    blocker.join().unwrap().unwrap();
    if still_waiting {
        let _ = deletion.as_mut().await;
    }
    drop(deletion);
    tokio::time::timeout(Duration::from_secs(2), async {
        while weak.upgrade().is_some() || slots.available_permits() != 16 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let preserved = h.store.get_session(session).unwrap().is_some();
    let fenced = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT summary_session_deleting FROM session_lifecycle WHERE session_id=?1",
                [session.to_string()],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    let retry = h
        .service
        .prepare_session_delete_owned(session, h.process.clone())
        .await
        .unwrap();
    retry.delete_owned(session).await.unwrap();
    h.close().await;
    assert!(
        observed_timeout,
        "the final session DELETE observer must return its own 10s Timeout while SQLite remains owned"
    );
    assert!(elapsed >= Duration::from_secs(9) && elapsed < Duration::from_secs(11));
    assert!(retained && preserved);
    assert_eq!(available, 15);
    assert_eq!(fenced, 1);
    assert!(h.store.get_session(session).unwrap().is_none());
}
#[tokio::test]
async fn summary_attempt_registry_bound_and_last_clone_require_actual_slot_release() {
    let dir = tempfile::tempdir().unwrap();
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dir.path().join("registry-process.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    let file = Arc::new(file);
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let registry = AttemptRegistry::default();
    let mut attempts = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..16 {
        let id = Uuid::new_v4();
        let owner: OperationLease = Arc::new(RegistryOwner {
            _file: file.clone(),
            _slot: slots.clone().try_acquire_owned().unwrap(),
        });
        attempts.push(registry.register(id, owner).unwrap());
        ids.push(id);
    }
    let overflow: OperationLease = file.clone();
    assert!(matches!(
        registry.register(Uuid::new_v4(), overflow),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    assert_eq!(slots.available_permits(), 0);
    let retained = attempts[0].clone();
    attempts.remove(0);
    assert!(registry.with_idle(&[ids[0]], || Ok(())).unwrap().is_none());
    assert_eq!(slots.available_permits(), 0);
    drop(retained);
    assert!(registry.with_idle(&[ids[0]], || Ok(())).unwrap().is_some());
    assert_eq!(slots.available_permits(), 1);
    drop(attempts);
    registry.drain().await.unwrap();
    assert_eq!(slots.available_permits(), 16);
}
