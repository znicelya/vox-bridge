use super::harness::*;
use crate::{intelligence::types::*, summary::SummaryService};
#[tokio::test]
async fn summary_readiness_cannot_be_inferred_from_started_jobs_without_local_recovery() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["完整输入"]).await;
    let unready = SummaryService::new(
        h.store.clone(),
        h.config.clone(),
        h.jobs.clone(),
        h.llm.clone(),
    );
    let result = generate_local_owned(&unready, session, None, h.process.clone()).await;
    if let Ok(job) = &result {
        h.wait(job.id).await;
    }
    h.close().await;
    assert!(
        matches!(
            result,
            Err(crate::error::AppError::Intelligence(
                IntelligenceErrorCode::Interrupted
            ))
        ),
        "started scheduler does not mint summary-ready authority"
    );
    assert!(h.native_versions(session).unwrap().is_empty());
}
#[tokio::test]
async fn summary_local_recovery_finishes_durable_delete_with_joined_prior_job_ownership() {
    let h = SummaryHarness::new().await;
    let session = h.completed_session(&["已保留完整输入"]).await;
    let job = h.generate(session, None).await.unwrap();
    h.wait(job.id).await;
    h.close().await;
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET deleting=1 WHERE id=?1",
                [job.object_id.unwrap().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    h.jobs.reconcile().unwrap();
    h.service
        .reconcile_local_owned(h.process.clone(), 1, h.jobs.reap_port())
        .await
        .unwrap();
    assert!(
        h.native_versions(session).unwrap().is_empty(),
        "ready is published only after local deletion recovery"
    );
    assert!(
        h.store.get_job(job.id).unwrap().is_some(),
        "summary recovery preserves permanent/job history"
    );
    assert_eq!(
        h.llm.calls.lock().unwrap().len(),
        1,
        "recovery itself must make zero model calls"
    );
}

struct RecoveryLease {
    _file: std::fs::File,
    _slot: tokio::sync::OwnedSemaphorePermit,
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_recovery_sql_acquisition_timeout_keeps_readiness_closed_and_owner_until_database_release()
 {
    use std::{sync::Arc, time::Duration};
    let h = SummaryHarness::new().await;
    let service = SummaryService::new(
        h.store.clone(),
        h.config.clone(),
        h.jobs.clone(),
        h.llm.clone(),
    );
    let session = h.completed_session(&[]).await;
    // Join initial empty claims so both Tokio workers remain available while
    // this local recovery fixture deliberately holds SQLite.
    h.close().await;
    let file = std::fs::File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(h.dir.path().join("recovery-owner.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&file).unwrap();
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let owner: crate::intelligence::jobs::OperationLease = Arc::new(RecoveryLease {
        _file: file,
        _slot: slots.clone().try_acquire_owned().unwrap(),
    });
    let weak = Arc::downgrade(&owner);
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
    let result = service
        .reconcile_local_owned(owner, 0, h.jobs.reap_port())
        .await;
    let held = weak.upgrade().is_some();
    let available = slots.available_permits();
    // Do not start any SQL-dependent command while the actual mutex is held.
    let closed = service.require_epoch(0).is_err();
    let retry = service
        .reconcile_local_owned(h.process.clone(), 1, h.jobs.reap_port())
        .await
        .err();
    let mut drain = Box::pin(service.drain_actual());
    let waits = tokio::time::timeout(Duration::from_millis(50), drain.as_mut())
        .await
        .is_err();
    drop(release);
    blocker.join().unwrap().unwrap();
    if waits {
        drain.await.unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while weak.upgrade().is_some() || slots.available_permits() != 16 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::Timeout
        ))
    ));
    assert!(held && closed && waits);
    assert_eq!(available, 15);
    assert!(matches!(
        retry,
        Some(crate::error::AppError::Intelligence(
            IntelligenceErrorCode::Busy
        ))
    ));
    assert!(
        generate_local_owned(&service, session, None, h.process.clone())
            .await
            .is_err()
    );
    service
        .reconcile_local_owned(h.process.clone(), 1, h.jobs.reap_port())
        .await
        .unwrap();
    h.close().await;
}
