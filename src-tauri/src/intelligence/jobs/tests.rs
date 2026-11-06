use super::*;
mod child_operations;
mod documents;
use crate::intelligence::test_support::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
struct Handler {
    calls: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
    fail: bool,
}
#[async_trait::async_trait]
impl JobHandler for Handler {
    async fn run(
        &self,
        mut context: JobContext,
        _: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::select! {_=super::super::http::cancelled(&mut context.cancel)=>Err(IntelligenceErrorCode::Cancelled),_=self.release.notified()=>if self.fail{Err(IntelligenceErrorCode::InvalidResponse)}else{Ok(())}}
    }
}
fn handler() -> Arc<Handler> {
    Arc::new(Handler {
        calls: Arc::default(),
        release: Arc::default(),
        fail: false,
    })
}
async fn enqueue(jobs: &JobScheduler, id: Uuid) -> AppResult<JobView> {
    jobs.enqueue(
        JobKind::Import,
        None,
        Some(id),
        JobAdmission::Manual {
            exclusion_key: format!("document:{id}"),
        },
        payload(vec![]),
    )
    .await
}
async fn status(h: &IntelligenceHarness, id: Uuid, want: JobStatus) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if h.store.get_job(id).unwrap().unwrap().status == want {
                return;
            }
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn intelligence_jobs_registry_is_frozen_and_missing_handlers_reject_before_admission() {
    let h = IntelligenceHarness::new();
    let j = JobScheduler::new(h.store.clone());
    assert!(enqueue(&j, Uuid::new_v4()).await.is_err());
    let a = handler();
    j.register(JobKind::Import, a.clone()).unwrap();
    assert!(j.register(JobKind::Import, a.clone()).is_err());
    j.start().unwrap();
    j.start().unwrap();
    assert!(j.register(JobKind::Reindex, a).is_err());
    j.shutdown().await.unwrap();
    j.start().unwrap();
    j.shutdown().await.unwrap();
    assert!(h.store.list_jobs(None, 200).unwrap().rows.is_empty());
}
#[tokio::test]
async fn intelligence_jobs_two_workers_capacity_cancel_and_terminal_persistence() {
    let h = IntelligenceHarness::new();
    let j = JobScheduler::new(h.store.clone());
    let a = handler();
    j.register(JobKind::Import, a.clone()).unwrap();
    j.start().unwrap();
    let mut jobs = Vec::new();
    for _ in 0..32 {
        jobs.push(enqueue(&j, Uuid::new_v4()).await.unwrap());
    }
    assert!(enqueue(&j, Uuid::new_v4()).await.is_err());
    tokio::time::timeout(Duration::from_secs(2), async {
        while a.calls.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    assert_eq!(a.calls.load(Ordering::SeqCst), 2);
    let last = jobs.last().unwrap();
    assert_eq!(
        j.cancel(last.id).await.unwrap().status,
        JobStatus::Cancelled
    );
    assert_eq!(
        j.cancel(last.id).await.unwrap().status,
        JobStatus::Cancelled
    );
    j.cancel(jobs[0].id).await.unwrap();
    status(&h, jobs[0].id, JobStatus::Cancelled).await;
    j.shutdown().await.unwrap();
    assert!(
        h.store
            .list_jobs(None, 200)
            .unwrap()
            .rows
            .iter()
            .all(|r| r.status != JobStatus::Queued && r.status != JobStatus::Running)
    );
}
#[tokio::test]
async fn intelligence_jobs_success_failure_release_exclusion_and_parser_is_shared() {
    for fail in [false, true] {
        let h = IntelligenceHarness::new();
        let j = JobScheduler::new(h.store.clone());
        let a = Arc::new(Handler {
            fail,
            ..Handler {
                calls: Arc::default(),
                release: Arc::default(),
                fail: false,
            }
        });
        j.register(JobKind::Import, a.clone()).unwrap();
        j.start().unwrap();
        let object = Uuid::new_v4();
        let job = enqueue(&j, object).await.unwrap();
        a.release.notify_one();
        status(
            &h,
            job.id,
            if fail {
                JobStatus::Failed
            } else {
                JobStatus::Succeeded
            },
        )
        .await;
        assert!(enqueue(&j, object).await.is_ok());
        let permit = j.parser_gate().try_acquire_owned().unwrap();
        assert!(j.parser_gate().try_acquire_owned().is_err());
        drop(permit);
        j.shutdown().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn intelligence_jobs_sqlite_wait_does_not_block_snapshot_admission_control() {
    let h = IntelligenceHarness::new();
    let j = h.jobs.clone();
    let a = handler();
    j.register(JobKind::Import, a.clone()).unwrap();
    j.start().unwrap();
    let job = enqueue(&j, Uuid::new_v4()).await.unwrap();
    status(&h, job.id, JobStatus::Running).await;
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let store = h.store.clone();
    let blocker = std::thread::spawn(move || {
        store.transaction(|_| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        })
    });
    entered_rx.await.unwrap();
    let control = j
        .state
        .lock()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .control
        .clone();
    let (check_tx, check_rx) = tokio::sync::oneshot::channel();
    let observer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let available = control.try_lock().is_ok();
        release_tx.send(()).unwrap();
        check_tx.send(available).unwrap();
    });
    a.release.notify_one();
    let available = check_rx.await.unwrap();
    observer.join().unwrap();
    blocker.join().unwrap().unwrap();
    j.shutdown().await.unwrap();
    assert!(
        available,
        "worker held generation control while awaiting SQLite, reversing snapshot admission lock order"
    );
}

struct Stubborn {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl JobHandler for Stubborn {
    async fn run(&self, _: JobContext, _: serde_json::Value) -> Result<(), IntelligenceErrorCode> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(())
    }
}
#[tokio::test]
async fn intelligence_jobs_uncertain_shutdown_retains_lease_and_fences_late_success_and_restart() {
    let h = IntelligenceHarness::new();
    let mut j = JobScheduler::new(h.store.clone());
    j.shutdown_timeout = Duration::from_millis(30);
    let a = Arc::new(Stubborn {
        entered: Default::default(),
        release: Default::default(),
    });
    j.register(JobKind::Import, a.clone()).unwrap();
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    j.start_owned(owner).unwrap();
    let job = enqueue(&j, Uuid::new_v4()).await.unwrap();
    a.entered.notified().await;
    assert!(j.shutdown().await.is_err());
    assert_eq!(
        h.store.get_job(job.id).unwrap().unwrap().status,
        JobStatus::Interrupted
    );
    assert!(j.start().is_err());
    assert!(enqueue(&j, Uuid::new_v4()).await.is_err());
    assert!(weak.upgrade().is_some());
    a.release.notify_one();
    j.shutdown().await.unwrap();
    assert_eq!(
        h.store.get_job(job.id).unwrap().unwrap().status,
        JobStatus::Interrupted
    );
    assert!(weak.upgrade().is_none());
    j.start().unwrap();
    j.shutdown().await.unwrap();
}
#[tokio::test]
async fn intelligence_jobs_snapshot_admission_retains_public_identity() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    h.config
        .save(SaveModelProfile {
            profile: p.clone(),
            credential: crate::app::CredentialChange::Replace {
                api_key: "synthetic-admission".into(),
            },
        })
        .await
        .unwrap();
    let a = handler();
    h.jobs.register(JobKind::Import, a).unwrap();
    h.jobs.start().unwrap();
    let id = Uuid::new_v4();
    let job = h
        .jobs
        .enqueue_with_profiles(
            &h.config,
            JobKind::Import,
            None,
            Some(id),
            JobAdmission::Manual {
                exclusion_key: format!("document:{id}"),
            },
            vec![p.id],
            serde_json::json!({"document_id":id}),
        )
        .await
        .unwrap();
    assert!(h.config.delete(p.id).await.is_err());
    let raw: String = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                [job.id.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(raw.contains("fixture-model"));
    assert!(!raw.contains("synthetic-admission"));
    h.jobs.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn intelligence_jobs_actual_config_snapshot_and_shutdown_share_one_order() {
    let h = IntelligenceHarness::new();
    h.jobs.register(JobKind::Import, handler()).unwrap();
    h.jobs.start().unwrap();
    let fence = h.jobs.admission_fence(JobKind::Import).unwrap();
    let id = Uuid::new_v4();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let config = h.config.clone();
    let admission = tokio::spawn(async move {
        config
            .with_profile_snapshot(vec![], move |tx, profiles| {
                fence.run(|| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    super::super::store::admit_job(
                        tx,
                        JobDraft {
                            id: Uuid::new_v4(),
                            kind: JobKind::Import,
                            session_id: None,
                            object_id: Some(id),
                            admission: JobAdmission::Manual {
                                exclusion_key: format!("document:{id}"),
                            },
                            payload: payload(profiles),
                            initial_failure: None,
                        },
                    )
                })
            })
            .await
    });
    entered_rx.await.unwrap();
    let jobs = h.jobs.clone();
    let close = tokio::spawn(async move { jobs.shutdown().await });
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        release_tx.send(()).unwrap();
    });
    let admitted = tokio::time::timeout(Duration::from_secs(2), admission)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), close)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    release.join().unwrap();
    assert_eq!(
        h.store.get_job(admitted.view().id).unwrap().unwrap().status,
        JobStatus::Cancelled
    );
}
