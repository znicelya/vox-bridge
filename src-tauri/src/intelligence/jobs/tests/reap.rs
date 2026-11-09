use super::*;

struct Capture {
    contexts: mpsc::Sender<JobContext>,
    release: Semaphore,
    port: super::super::reap::JobReapPort,
    terminal_proved: AtomicBool,
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
    fn terminal_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        id: Uuid,
        _: JobStatus,
        _: Option<IntelligenceErrorCode>,
        _: bool,
    ) -> AppResult<()> {
        // This runs while generation control is held. Re-locking that mutex in
        // the nonmutating port would deadlock the genuine worker transition.
        self.terminal_proved
            .store(self.port.prove(tx, id)?.is_some(), Ordering::SeqCst);
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
        port: h.jobs.reap_port(),
        terminal_proved: AtomicBool::new(false),
    });
    h.jobs.register(JobKind::Import, handler.clone()).unwrap();
    (h, handler, receiver)
}
async fn captured(
    h: &IntelligenceHarness,
    receiver: &mut mpsc::Receiver<JobContext>,
) -> JobContext {
    enqueue(&h.jobs, Uuid::new_v4()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap()
}
fn proved(h: &IntelligenceHarness, id: Uuid) -> AppResult<bool> {
    h.store.transaction(|tx| {
        h.jobs
            .reap_port()
            .prove(tx, id)
            .map(|proof| proof.is_some())
    })
}

// A terminal row must not release actual child work, and a retained context
// must not mint more work after its real handler has ended.
#[tokio::test]
async fn intelligence_reap_terminal_hook_waits_for_last_actual_child_and_closes_factory() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let context = captured(&h, &mut receiver).await;
    let child = context.operation_owner().unwrap();
    let final_clone = child.clone();
    handler.release.add_permits(1);
    status(&h, context.id, JobStatus::Succeeded).await;
    assert!(!handler.terminal_proved.load(Ordering::SeqCst));
    assert!(!proved(&h, context.id).unwrap());
    assert!(context.operation_owner().is_err());
    drop(child);
    assert!(!proved(&h, context.id).unwrap());
    drop(final_clone);
    assert!(proved(&h, context.id).unwrap());
    h.jobs.shutdown().await.unwrap();
}

// A matching live UUID is checked before status, row-order or absence can be
// used. Local rowid/payload drift cannot masquerade as unissued terminal work.
#[tokio::test]
async fn intelligence_reap_rejects_matching_live_frame_rowid_and_payload_drift() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let context = captured(&h, &mut receiver).await;
    let (rowid, raw) = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT rowid,payload_json FROM intelligence_jobs WHERE id=?1",
                [context.id.to_string()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?)
        })
        .unwrap();
    for mutation in ["rowid", "payload"] {
        h.store.transaction(|tx| {
            tx.execute("UPDATE intelligence_jobs SET status='interrupted',rowid=?2,payload_json=?3 WHERE id=?1",
                rusqlite::params![context.id.to_string(), if mutation == "rowid" { rowid + 100 } else { rowid },
                    if mutation == "payload" { payload(vec![]).to_string() + " " } else { raw.clone() }])?;
            Ok(())
        }).unwrap();
        assert!(
            proved(&h, context.id).is_err(),
            "{mutation} drift of a retained actual claim was misclassified as absent"
        );
    }
    h.store.transaction(|tx| {
        tx.execute("UPDATE intelligence_jobs SET status='running',rowid=?2,payload_json=?3 WHERE id=?1",
            rusqlite::params![context.id.to_string(), rowid, raw])?;
        Ok(())
    }).unwrap();
    handler.release.add_permits(1);
    status(&h, context.id, JobStatus::Succeeded).await;
    assert!(handler.terminal_proved.load(Ordering::SeqCst));
    h.jobs.shutdown().await.unwrap();
}

// The compact complete claim ledger, rather than absence of one frame, is the
// authority. Deliberately losing a pending frame must fail closed.
#[tokio::test]
async fn intelligence_reap_absent_pending_frame_requires_complete_claim_ledger() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let context = captured(&h, &mut receiver).await;
    let frame = h
        .jobs
        .reap
        .inner
        .lock()
        .unwrap()
        .frames
        .remove(&context.id)
        .unwrap();
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='interrupted' WHERE id=?1",
                [context.id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(proved(&h, context.id).is_err());
    h.jobs
        .reap
        .inner
        .lock()
        .unwrap()
        .frames
        .insert(context.id, frame);
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='running' WHERE id=?1",
                [context.id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    handler.release.add_permits(1);
    h.jobs.shutdown().await.unwrap();
}

// Claims may arrive in an order unrelated to a frontier. An older re-eligible
// durable row still needs its real frame even after a newer claim ended.
#[tokio::test]
async fn intelligence_reap_out_of_order_older_claim_cannot_use_newer_completion() {
    let (h, handler, mut receiver) = fixture();
    let old = h
        .store
        .transaction(|tx| {
            let object = Uuid::new_v4();
            let job = store::admit_job(
                tx,
                store::JobDraft {
                    id: Uuid::new_v4(),
                    kind: JobKind::Import,
                    session_id: None,
                    object_id: Some(object),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{object}"),
                    },
                    payload: payload(vec![]),
                    initial_failure: None,
                },
            )?
            .view()
            .clone();
            tx.execute(
                "UPDATE intelligence_jobs SET status='failed',error_code='interrupted' WHERE id=?1",
                [job.id.to_string()],
            )?;
            Ok(job.id)
        })
        .unwrap();
    h.jobs.start().unwrap();
    let newer = captured(&h, &mut receiver).await;
    // Re-enable the previously skipped row only after the newer genuine claim.
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='queued',error_code=NULL WHERE id=?1",
                [old.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(!proved(&h, old).unwrap());
    h.jobs.wake();
    let older = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(older.id, old);
    let child = older.operation_owner().unwrap();
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='interrupted' WHERE id=?1",
                [old.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(!proved(&h, old).unwrap());
    handler.release.add_permits(2);
    status(&h, newer.id, JobStatus::Succeeded).await;
    assert!(!proved(&h, old).unwrap());
    drop(child);
    h.jobs.shutdown().await.unwrap();
    assert!(proved(&h, old).unwrap());
}

// Quiescent genuine frames may be pruned, but rearm/fresh authority requires
// the explicit completed lifecycle, not a dropped generation option.
#[tokio::test]
async fn intelligence_reap_safe_pruning_and_explicit_joined_fresh_rearm_authority() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let mut ids = Vec::new();
    for _ in 0..40 {
        let context = captured(&h, &mut receiver).await;
        handler.release.add_permits(1);
        status(&h, context.id, JobStatus::Succeeded).await;
        assert!(proved(&h, context.id).unwrap());
        ids.push(context.id);
    }
    assert!(h.jobs.reap.inner.lock().unwrap().frames.len() <= 18);
    h.jobs.shutdown().await.unwrap();
    assert!(proved(&h, ids[0]).unwrap());
    h.jobs.start().unwrap();
    assert!(proved(&h, ids[0]).unwrap());
    h.jobs.shutdown().await.unwrap();
    let fresh = JobScheduler::new(h.store.clone()).reap_port();
    assert!(
        h.store
            .transaction(|tx| fresh.prove(tx, ids[0]).map(|proof| proof.is_some()))
            .unwrap()
    );
}

fn queued(h: &IntelligenceHarness) -> Uuid {
    h.store
        .transaction(|tx| {
            let object = Uuid::new_v4();
            Ok(store::admit_job(
                tx,
                store::JobDraft {
                    id: Uuid::new_v4(),
                    kind: JobKind::Import,
                    session_id: None,
                    object_id: Some(object),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{object}"),
                    },
                    payload: payload(vec![]),
                    initial_failure: None,
                },
            )?
            .view()
            .id)
        })
        .unwrap()
}

// A genuine claim is registered before handler entry or the Running event.
// Even an Interrupted SQL replacement cannot cover that unentered frame.
#[tokio::test]
async fn intelligence_reap_claim_before_handler_entry_is_not_terminal_authority() {
    let (h, handler, _receiver) = fixture();
    h.jobs.start().unwrap();
    let id = queued(&h); // Deliberately do not wake a worker yet.
    let control = h
        .jobs
        .state
        .lock()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .control
        .clone();
    let (job, _, _, frame) = claim(&h.store, &control, &h.jobs.closed, &h.jobs.reap)
        .unwrap()
        .unwrap();
    assert_eq!(job.id, id);
    h.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='interrupted' WHERE id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(!proved(&h, id).unwrap());
    // This test owns the unentered claim. No run/child was ever issued; close
    // that factory before releasing the retained genuine claim for shutdown.
    frame.close_handler();
    control.lock().unwrap().active.remove(&id);
    assert!(proved(&h, id).unwrap());
    drop(handler);
    h.jobs.shutdown().await.unwrap();
}

// Failed SQLite commit cannot leave an issued claim or a phantom active job.
#[tokio::test]
async fn intelligence_reap_claim_rollback_keeps_ledger_complete_and_allows_retry() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let id = queued(&h);
    let control = h
        .jobs
        .state
        .lock()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .control
        .clone();
    h.store
        .transaction(|tx| {
            tx.execute_batch(
                "CREATE TRIGGER reject_claim BEFORE UPDATE OF status ON intelligence_jobs \
            WHEN NEW.status='running' BEGIN SELECT RAISE(ABORT,'claim rollback'); END;",
            )?;
            Ok(())
        })
        .unwrap();
    assert!(claim(&h.store, &control, &h.jobs.closed, &h.jobs.reap).is_err());
    assert!(!proved(&h, id).unwrap());
    assert!(!control.lock().unwrap().active.contains_key(&id));
    h.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER reject_claim")?;
            Ok(())
        })
        .unwrap();
    h.jobs.wake();
    let context = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(context.id, id);
    handler.release.add_permits(1);
    status(&h, id, JobStatus::Succeeded).await;
    assert!(proved(&h, id).unwrap());
    h.jobs.shutdown().await.unwrap();
}

// Losing both the async observer and handler does not reap a ConfigIo closure
// that has actually entered blocking work under its accepted child scope.
#[tokio::test]
async fn intelligence_reap_entered_child_outlives_observer_and_terminal_handler() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let context = captured(&h, &mut receiver).await;
    let scoped =
        h.io.clone()
            .with_operation_owner(context.operation_owner().unwrap());
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    let mut observer = Box::pin(scoped.run(move |_| {
        let _ = entered.send(());
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok(())
    }));
    assert!(futures_util::poll!(observer.as_mut()).is_pending());
    tokio::time::timeout(Duration::from_secs(2), entry)
        .await
        .unwrap()
        .unwrap();
    drop(observer);
    drop(scoped);
    handler.release.add_permits(1);
    status(&h, context.id, JobStatus::Succeeded).await;
    let retained = !proved(&h, context.id).unwrap();
    let uncertain = h.jobs.shutdown().await.is_err();
    release.send(()).unwrap();
    h.jobs.shutdown().await.unwrap();
    assert!(retained && uncertain);
    assert!(proved(&h, context.id).unwrap());
}

// Model the real drop window after process/shared-slot release but before the
// per-job completion guard publishes zero. Generation shutdown must await that
// publication rather than permanently abandoning its done notification.
#[tokio::test]
async fn intelligence_reap_shutdown_waits_for_completion_publication_after_slot_release() {
    let (h, handler, mut receiver) = fixture();
    h.jobs.start().unwrap();
    let context = captured(&h, &mut receiver).await;
    let frame = h
        .jobs
        .reap
        .inner
        .lock()
        .unwrap()
        .frames
        .get(&context.id)
        .unwrap()
        .clone();
    let completion = frame.child().unwrap();
    handler.release.add_permits(1);
    status(&h, context.id, JobStatus::Succeeded).await;
    assert!(!proved(&h, context.id).unwrap());
    let mut shutdown = Box::pin(h.jobs.shutdown());
    let pending = tokio::time::timeout(Duration::from_millis(10), shutdown.as_mut())
        .await
        .is_err();
    drop(completion);
    let stopped = if pending {
        shutdown.await
    } else {
        h.jobs.shutdown().await
    };
    assert!(
        pending,
        "shared-slot drain cannot publish generation completion before per-job completion drops"
    );
    assert!(
        stopped.is_ok(),
        "the completion publication must permit the retained generation shutdown to finish"
    );
    assert!(proved(&h, context.id).unwrap());
}

// Worker join cannot complete a genuine claim whose handler/factory never
// ended. Reject that impossible ledger state promptly, even with a child.
#[tokio::test]
async fn intelligence_reap_shutdown_rejects_unfinished_factory_without_waiting_for_children() {
    let (h, _handler, _receiver) = fixture();
    h.jobs.start().unwrap();
    let id = queued(&h);
    let control = h
        .jobs
        .state
        .lock()
        .unwrap()
        .generation
        .as_ref()
        .unwrap()
        .control
        .clone();
    let (_, _, _, frame) = claim(&h.store, &control, &h.jobs.closed, &h.jobs.reap)
        .unwrap()
        .unwrap();
    let completion = frame.child().unwrap();
    let stopped = tokio::time::timeout(Duration::from_millis(20), h.jobs.shutdown()).await;
    assert!(
        matches!(stopped, Ok(Err(_))),
        "an unfinished factory must fail closed instead of waiting for an impossible handler end"
    );
    assert!(!proved(&h, id).unwrap());
    assert!(h.jobs.start().is_err());
    // This fixture owns the unentered claim; no handler was issued.
    drop(completion);
    frame.close_handler();
}
