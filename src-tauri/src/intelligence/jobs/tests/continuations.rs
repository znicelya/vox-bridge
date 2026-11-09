use super::*;
struct Immediate;
#[async_trait::async_trait]
impl JobHandler for Immediate {
    async fn run(&self, _: JobContext, _: serde_json::Value) -> Result<(), IntelligenceErrorCode> {
        Ok(())
    }
}
struct Hook {
    io: crate::config::ConfigIo,
    store: Arc<crate::storage::LocalStore>,
    channels: std::sync::Mutex<
        Option<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
    calls: AtomicUsize,
    completed: Arc<AtomicUsize>,
    reap: JobReapPort,
}
#[async_trait::async_trait]
impl AfterTerminal for Hook {
    fn notice(
        &self,
        tx: &rusqlite::Transaction<'_>,
        job: &JobView,
    ) -> AppResult<Option<AfterTerminalNotice>> {
        if job.kind != JobKind::Import {
            return Ok(None);
        }
        let _ = tx;
        Ok(Some(AfterTerminalNotice {
            parent_id: job.id,
            document_id: job.object_id.unwrap(),
            version: 1,
            sha256: "a".repeat(64),
            parser: "fixture".into(),
        }))
    }
    fn deferred(&self, _: &rusqlite::Transaction<'_>, _: &AfterTerminalNotice) -> AppResult<()> {
        Ok(())
    }
    async fn succeeded(
        &self,
        notice: AfterTerminalNotice,
        owner: OperationLease,
        fence: JobAdmissionFence,
    ) {
        assert_eq!(
            self.store
                .get_job(notice.parent_id)
                .unwrap()
                .unwrap()
                .status,
            JobStatus::Succeeded
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            self.store
                .transaction(|tx| self.reap.prove(tx, notice.parent_id))
                .unwrap()
                .is_none(),
            "the preterminal reservation still owns the actual continuation"
        );
        self.store
            .transaction(|tx| {
                fence.run(|| {
                    store::admit_job(
                        tx,
                        store::JobDraft {
                            id: Uuid::new_v4(),
                            kind: JobKind::Reindex,
                            session_id: None,
                            object_id: Some(notice.document_id),
                            admission: JobAdmission::Manual {
                                exclusion_key: format!("document:{}", notice.document_id),
                            },
                            payload: payload(vec![]),
                            initial_failure: None,
                        },
                    )?;
                    Ok(())
                })
            })
            .unwrap();
        let (entered, release) = self.channels.lock().unwrap().take().unwrap();
        let completed = self.completed.clone();
        self.io
            .with_operation_owner(owner)
            .run(move |_| {
                let _ = entered.send(());
                release.recv_timeout(Duration::from_secs(5)).unwrap();
                completed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn intelligence_after_terminal_reserves_before_success_releases_exclusion_and_drains_actual_child()
 {
    let h = IntelligenceHarness::new();
    h.jobs
        .register(JobKind::Import, Arc::new(Immediate))
        .unwrap();
    h.jobs
        .register(JobKind::Reindex, Arc::new(Immediate))
        .unwrap();
    let (entered, wait) = tokio::sync::oneshot::channel();
    let (release, receive) = std::sync::mpsc::channel();
    let hook = Arc::new(Hook {
        io: h.io.clone(),
        store: h.store.clone(),
        channels: std::sync::Mutex::new(Some((entered, receive))),
        calls: AtomicUsize::new(0),
        completed: Arc::new(AtomicUsize::new(0)),
        reap: h.jobs.reap_port(),
    });
    h.jobs.register_after_terminal(hook.clone()).unwrap();
    h.jobs.start().unwrap();
    let parent = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), wait)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        h.store.get_job(parent.id).unwrap().unwrap().status,
        JobStatus::Succeeded
    );
    let mut shutdown = Box::pin(h.jobs.shutdown());
    let pending = tokio::time::timeout(Duration::from_millis(40), shutdown.as_mut())
        .await
        .is_err();
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), shutdown.as_mut())
        .await
        .unwrap()
        .unwrap();
    assert!(
        pending,
        "terminal parent does not release the reserved entered child operation"
    );
    assert_eq!(hook.completed.load(Ordering::SeqCst), 1);
    assert_eq!(hook.calls.load(Ordering::SeqCst), 1);
    assert!(
        h.store
            .transaction(|tx| h.jobs.reap_port().prove(tx, parent.id))
            .unwrap()
            .is_some()
    );
    h.jobs.start().unwrap();
    h.jobs.shutdown().await.unwrap();
    assert_eq!(
        hook.calls.load(Ordering::SeqCst),
        1,
        "startup must not replay successful parent notices"
    );
}
#[tokio::test]
async fn intelligence_after_terminal_does_not_run_success_hook_for_failed_or_cancelled_parent() {
    for scenario in ["handler_failure", "preterminal_failure", "cancelled"] {
        let h = IntelligenceHarness::new();
        let mut handler = handler();
        Arc::get_mut(&mut handler).unwrap().fail = scenario == "handler_failure";
        h.jobs.register(JobKind::Import, handler.clone()).unwrap();
        h.jobs
            .register(JobKind::Reindex, Arc::new(Immediate))
            .unwrap();
        let (entered, _wait) = tokio::sync::oneshot::channel();
        let (_release, receive) = std::sync::mpsc::channel();
        let hook = Arc::new(Hook {
            io: h.io.clone(),
            store: h.store.clone(),
            channels: std::sync::Mutex::new(Some((entered, receive))),
            calls: AtomicUsize::new(0),
            completed: Arc::new(AtomicUsize::new(0)),
            reap: h.jobs.reap_port(),
        });
        h.jobs.register_after_terminal(hook.clone()).unwrap();
        h.jobs.start().unwrap();
        let parent = enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
        status(&h, parent.id, JobStatus::Running).await;
        if scenario == "cancelled" {
            h.jobs.cancel(parent.id).await.unwrap();
        } else if scenario == "preterminal_failure" {
            h.store
                .transaction(|tx| {
                    tx.execute(
                        "UPDATE intelligence_jobs SET status='failed' WHERE id=?1",
                        [parent.id.to_string()],
                    )?;
                    Ok(())
                })
                .unwrap();
        }
        handler.release.notify_one();
        status(
            &h,
            parent.id,
            if scenario == "cancelled" {
                JobStatus::Cancelled
            } else {
                JobStatus::Failed
            },
        )
        .await;
        h.jobs.shutdown().await.unwrap();
        assert_eq!(hook.calls.load(Ordering::SeqCst), 0);
    }
}
