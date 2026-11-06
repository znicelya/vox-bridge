use super::{
    SessionOrchestrator, SessionStatus,
    test_support::{RuntimeHarness, metadata},
};
use crate::{
    audio::PcmSpec,
    domain::{
        launch::{DirectionPhase, SessionContext},
        session::SessionDirection,
        transcript::{DiagnosticPayload, RuntimeFailureCode},
    },
    events::SessionEvent,
    providers::{
        ProviderEvent,
        fixture::{FixtureScript, FixtureStep},
    },
    storage::{StoredSession, StoredSessionLifecycle},
};
use std::{
    future::{Future, poll_fn},
    task::Poll,
    time::Duration,
};
use uuid::Uuid;

fn script() -> FixtureScript {
    FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap()
}
async fn wait_status(o: &SessionOrchestrator, id: Uuid, status: SessionStatus) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if o.get(id).await.unwrap().unwrap().status == status {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn poll_pending<F: Future>(future: std::pin::Pin<&mut F>) {
    let mut future = future;
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn concurrent_stop_returns_the_same_durable_terminal_record() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let started = o.start(h.launch.clone(), h.saved.clone()).await.unwrap();
    assert_eq!(started.status, SessionStatus::Starting);
    assert_eq!(
        started.runtime.as_ref().unwrap().phase,
        crate::domain::launch::RuntimePhase::Starting
    );
    assert!(
        started
            .runtime
            .as_ref()
            .unwrap()
            .directions
            .iter()
            .all(|direction| direction.phase == DirectionPhase::Starting)
    );
    assert_ne!(started.id, h.id);
    assert_eq!(started.context.as_ref().unwrap().launch, h.launch);
    let mut events = o.subscribe(started.id).await.unwrap();
    let (a, b) = tokio::join!(o.stop_session(started.id), o.stop_session(started.id));
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    assert_eq!(a.status, SessionStatus::Completed);
    assert!(a.ended_at.is_some());
    assert!(a.runtime.is_none());
    assert_eq!(o.stop_session(started.id).await.unwrap(), a);
    assert_eq!(o.get(started.id).await.unwrap(), Some(a));
    assert_eq!(h.audio.open_ports(), 0);
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, SessionEvent::Status { status, .. } if status == "running"));
    }
}

#[tokio::test]
async fn running_stop_finishes_each_call_stream_once_and_replays_terminal() {
    let h = RuntimeHarness::call(script(), script()).unwrap();
    let o = h.orchestrator();
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    wait_status(&o, id, SessionStatus::Running).await;
    assert!(o.uses_provider_config(h.saved.id()).await);
    let (a, b) = tokio::join!(o.stop_session(id), o.stop_session(id));
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(h.provider.finish_counts(), [1, 1]);
    assert_eq!(h.audio.open_ports(), 0);
    assert!(!o.uses_provider_config(h.saved.id()).await);
    let mut events = o.subscribe(id).await.unwrap();
    assert!(
        matches!(events.recv().await.unwrap(), SessionEvent::Status { status, .. } if status == "completed")
    );
    o.delete_session(id).await.unwrap();
    assert!(o.get(id).await.unwrap().is_none());
}

#[tokio::test]
async fn losing_start_never_prepares_and_cancelled_reservation_releases_immediately() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let (entered, _release) = h.audio.block_inventory();
    let winner = {
        let o = o.clone();
        let launch = h.launch.clone();
        let saved = h.saved.clone();
        tokio::spawn(async move { o.start(launch, saved).await })
    };
    entered.await.unwrap();
    assert!(o.uses_provider_config(h.saved.id()).await);
    let loser = o
        .start(h.launch.clone(), h.saved.clone())
        .await
        .unwrap_err();
    assert!(loser.to_string().contains("stop the active session first"));
    assert_eq!(h.audio.inventory_calls(), 1);
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.provider.connect_counts(), [0, 0]);
    assert_eq!(h.store.list_sessions(20, None).unwrap().len(), 1);
    winner.abort();
    let _ = winner.await;
    assert!(!o.uses_provider_config(h.saved.id()).await);
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    assert!(o.delete_session(id).await.is_err());
    o.stop_session(id).await.unwrap();
}

#[tokio::test]
async fn failed_prepare_releases_reservation_and_creates_nothing() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    h.audio.set_ready("live-input", false);
    // A missing selected endpoint fails preparation before any durable admission.
    let mut launch = h.launch.clone();
    if let crate::domain::launch::DeviceSelection::Live { input_id, .. } = &mut launch.devices {
        *input_id = "missing".into();
    }
    assert!(o.start(launch, h.saved.clone()).await.is_err());
    assert!(!o.uses_provider_config(h.saved.id()).await);
    assert_eq!(h.store.list_sessions(20, None).unwrap().len(), 1);
    assert_eq!(h.audio.open_calls(), 0);
}

#[tokio::test]
async fn failed_starting_insert_is_atomic_and_opens_nothing() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    h.fail_starting_insert();
    assert!(o.start(h.launch.clone(), h.saved.clone()).await.is_err());
    assert_eq!(h.store.list_sessions(20, None).unwrap().len(), 1);
    assert!(!o.uses_provider_config(h.saved.id()).await);
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.provider.connect_counts(), [0, 0]);
}

#[tokio::test]
async fn running_persistence_failure_cleans_resources_and_never_publishes_running() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    h.store.fail_lifecycle_status_for_tests("running").unwrap();
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    let mut events = o.subscribe(id).await.unwrap();
    wait_status(&o, id, SessionStatus::Failed).await;
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(h.provider.finish_counts(), [1, 0]);
    let mut storage_errors = 0;
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(&event,SessionEvent::Status {status,..} if status=="running"));
        if matches!(event,SessionEvent::Diagnostic{diagnostic} if matches!(diagnostic.payload,DiagnosticPayload::Failure {code:RuntimeFailureCode::Storage,..}))
        {
            storage_errors += 1;
        }
    }
    assert_eq!(storage_errors, 1);
    assert!(o.get(id).await.unwrap().unwrap().runtime.is_none());
}

#[tokio::test]
async fn stopping_persistence_failure_still_cleans_and_releases_ownership() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    wait_status(&o, id, SessionStatus::Running).await;
    h.store.fail_lifecycle_status_for_tests("stopping").unwrap();
    let (a, b) = tokio::join!(o.stop_session(id), o.stop_session(id));
    assert_eq!(a.unwrap_err().to_string(), b.unwrap_err().to_string());
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(h.provider.finish_counts(), [1, 0]);
    assert_eq!(
        o.get(id).await.unwrap().unwrap().status,
        SessionStatus::Failed
    );
    assert!(!o.uses_provider_config(h.saved.id()).await);
    o.delete_session(id).await.unwrap();
}

#[tokio::test]
async fn terminal_write_failure_replays_error_to_attached_waiters_without_success() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let release = h.audio.delay_close("live-input");
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    wait_status(&o, id, SessionStatus::Running).await;
    h.store
        .fail_lifecycle_status_for_tests("completed")
        .unwrap();
    let mut events = o.subscribe(id).await.unwrap();
    let a = o.stop_session(id);
    let b = o.stop_session(id);
    tokio::pin!(a, b);
    poll_pending(a.as_mut()).await;
    poll_pending(b.as_mut()).await;
    wait_status(&o, id, SessionStatus::Stopping).await;
    release.send(()).unwrap();
    let (a, b) = tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(a, b) })
        .await
        .unwrap();
    assert_eq!(a.unwrap_err().to_string(), b.unwrap_err().to_string());
    assert_eq!(h.audio.open_ports(), 0);
    assert!(!o.uses_provider_config(h.saved.id()).await);
    let actual = o.get(id).await.unwrap().unwrap();
    assert_eq!(actual.status, SessionStatus::Stopping);
    assert!(actual.ended_at.is_none());
    assert!(actual.runtime.is_none());
    assert!(o.stop_session(id).await.is_err());
    let mut errors = 0;
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(&event,SessionEvent::Status {status,..} if status=="completed"));
        if matches!(event,SessionEvent::Diagnostic{diagnostic} if matches!(diagnostic.payload,DiagnosticPayload::Failure {code:RuntimeFailureCode::Storage,..}))
        {
            errors += 1;
        }
    }
    assert_eq!(errors, 1);
    // No permanent error cache or retained singleton lease.
    let next = o.start(h.launch.clone(), h.saved.clone()).await.unwrap();
    assert_ne!(next.id, id);
    let _ = o.stop_session(next.id).await;
}

#[tokio::test]
async fn cancelled_stop_caller_does_not_cancel_owned_cleanup() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let release = h.audio.delay_close("live-input");
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    wait_status(&o, id, SessionStatus::Running).await;
    let caller = {
        let o = o.clone();
        tokio::spawn(async move { o.stop_session(id).await })
    };
    wait_status(&o, id, SessionStatus::Stopping).await;
    caller.abort();
    let _ = caller.await;
    release.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), o.stop_session(id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, SessionStatus::Completed);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn direction_loss_keeps_peer_running_and_resume_uses_same_owner() {
    let h = RuntimeHarness::call(script(), script()).unwrap();
    let o = h.orchestrator();
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    wait_status(&o, id, SessionStatus::Running).await;
    h.audio.lose("a-input");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let record = o.get(id).await.unwrap().unwrap();
            if record.runtime.as_ref().unwrap().directions.iter().any(|d| {
                d.direction == SessionDirection::SourceToTarget && d.phase == DirectionPhase::Paused
            }) {
                assert_eq!(record.status, SessionStatus::Running);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.audio.restore("a-input");
    o.resume_direction(id, SessionDirection::SourceToTarget)
        .await
        .unwrap();
    assert_eq!(h.provider.connect_counts(), [2, 1]);
    o.stop_session(id).await.unwrap();
}

#[test]
fn recovery_interrupts_only_incomplete_rows_and_preserves_context() {
    let h = RuntimeHarness::live(script()).unwrap();
    let context = SessionContext {
        launch: h.launch.clone(),
        provider: crate::domain::session::ProviderId::new("fixture").unwrap(),
        provider_name: "fixture".into(),
        endpoints: vec![],
    };
    let mut ids = Vec::new();
    for status in [
        "created",
        "starting",
        "running",
        "stopping",
        "paused",
        "completed",
        "failed",
    ] {
        let id = Uuid::new_v4();
        let row = StoredSession {
            id,
            started_at: 10,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        };
        h.store.create_runtime_session(&row, &context).unwrap();
        h.store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: id,
                status: status.into(),
                ended_at: if matches!(status, "completed" | "failed") {
                    Some(20)
                } else {
                    None
                },
            })
            .unwrap();
        ids.push(id);
    }
    let text = crate::storage::StoredTranscriptSegment {
        id: Uuid::new_v4(),
        session_id: ids[0],
        direction: "source_to_target".into(),
        sequence: 1,
        occurred_at: 11,
        language: "zh".into(),
        text: "Retained final text".into(),
        is_final: true,
        details: None,
        occurred_at_ms: None,
    };
    h.store.append_final_segment(&text).unwrap();
    let interrupted = h.store.interrupt_incomplete_sessions(30).unwrap();
    assert_eq!(
        h.store.get_export_data(ids[0]).unwrap().unwrap().segments,
        vec![text]
    );
    assert_eq!(interrupted.len(), 6); // includes the unrelated legacy Created harness row
    for id in &ids[..5] {
        assert!(interrupted.contains(id));
        let lifecycle = h.store.get_session_lifecycle(*id).unwrap().unwrap();
        assert_eq!(lifecycle.status, "failed");
        assert_eq!(lifecycle.ended_at, Some(30));
        assert_eq!(h.store.session_context(*id).unwrap(), Some(context.clone()));
        let diagnostics = h.store.diagnostic_page(*id, None, 10).unwrap();
        assert_eq!(diagnostics.rows.len(), 1);
        assert!(matches!(
            diagnostics.rows[0].payload,
            DiagnosticPayload::Failure {
                code: RuntimeFailureCode::Interrupted,
                ..
            }
        ));
    }
    for id in &ids[5..] {
        assert_eq!(
            h.store
                .get_session_lifecycle(*id)
                .unwrap()
                .unwrap()
                .ended_at,
            Some(20)
        );
    }
    assert!(
        h.store
            .interrupt_incomplete_sessions(40)
            .unwrap()
            .is_empty()
    );
    assert!(h.store.session_context(h.id).unwrap().is_none());
}

#[test]
fn recovery_rolls_back_all_rows_and_diagnostics_when_lifecycle_write_fails() {
    let h = RuntimeHarness::live(script()).unwrap();
    h.store.fail_lifecycle_status_for_tests("failed").unwrap();
    assert!(h.store.interrupt_incomplete_sessions(30).is_err());
    assert_eq!(
        h.store.get_session_lifecycle(h.id).unwrap().unwrap().status,
        "created"
    );
    assert!(
        h.store
            .diagnostic_page(h.id, None, 10)
            .unwrap()
            .rows
            .is_empty()
    );
}

#[tokio::test]
async fn stop_during_retained_open_cancels_startup_and_waits_for_cleanup() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let release = h.audio.delay_open("live-input");
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    let mut events = o.subscribe(id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.audio.open_calls() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let stop = o.stop_session(id);
    tokio::pin!(stop);
    poll_pending(stop.as_mut()).await;
    wait_status(&o, id, SessionStatus::Stopping).await;
    release.send(Ok(())).unwrap();
    let terminal = tokio::time::timeout(Duration::from_secs(5), stop)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal.status, SessionStatus::Completed);
    assert_eq!(h.audio.open_ports(), 0);
    // Provider and audio acquisition begin concurrently; the retained provider must be cancelled.
    assert_eq!(h.provider.cancel_counts(), [1, 0]);
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, SessionEvent::Status { status, .. } if status == "running"));
    }
}

#[tokio::test]
async fn actual_startup_failure_persists_failed_and_replays_safe_failure() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let release = h.audio.delay_open("live-input");
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    release
        .send(Err(RuntimeFailureCode::DeviceUnavailable))
        .unwrap();
    wait_status(&o, id, SessionStatus::Failed).await;
    let record = o.stop_session(id).await.unwrap();
    assert_eq!(record.status, SessionStatus::Failed);
    assert!(record.ended_at.is_some());
    assert!(record.runtime.is_none());
    assert_eq!(h.audio.open_ports(), 0);
    let mut events = o.subscribe(id).await.unwrap();
    assert!(
        matches!(events.recv().await.unwrap(),SessionEvent::Diagnostic{diagnostic} if matches!(diagnostic.payload,DiagnosticPayload::Failure{code:RuntimeFailureCode::DeviceUnavailable,..}))
    );
}

#[tokio::test]
async fn actual_cleanup_failure_after_starting_stop_is_failed() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let entered = h.provider.delay_started(SessionDirection::SourceToTarget);
    let release = h.audio.delay_close_result("live-input");
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), entered.wait_entered())
            .await
            .unwrap()
    );
    assert_eq!(
        o.get(id).await.unwrap().unwrap().status,
        SessionStatus::Starting
    );
    let stop = o.stop_session(id);
    tokio::pin!(stop);
    poll_pending(stop.as_mut()).await;
    wait_status(&o, id, SessionStatus::Stopping).await;
    release
        .send(Err(RuntimeFailureCode::DeviceUnavailable))
        .unwrap();
    entered.release();
    let terminal = tokio::time::timeout(Duration::from_secs(5), stop)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal.status, SessionStatus::Failed);
    assert!(terminal.runtime.is_none());
    assert!(!o.uses_provider_config(h.saved.id()).await);
}

#[tokio::test]
async fn replayed_storage_failure_is_the_same_diagnostic_as_durable_history() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    h.store.fail_lifecycle_status_for_tests("running").unwrap();
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    wait_status(&o, id, SessionStatus::Failed).await;
    let mut events = o.subscribe(id).await.unwrap();
    let SessionEvent::Diagnostic { diagnostic } = events.recv().await.unwrap() else {
        panic!("expected failure");
    };
    let page = h.store.diagnostic_page(id, None, 100).unwrap();
    assert!(page.rows.contains(&diagnostic));
}

// Instance-scoped scheduling instrumentation. A failed test drops the release
// sender; the timeout also bounds the worker-thread wait if that path regresses.
pub(super) struct PhaseWriteGate {
    entered: tokio::sync::oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

impl PhaseWriteGate {
    pub(super) fn wait(self) {
        let _ = self.entered.send(());
        let _ = self.release.recv_timeout(Duration::from_secs(5));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admitted_starting_stop_suppresses_inflight_running_publication() {
    let h = RuntimeHarness::live(script()).unwrap();
    let o = h.orchestrator();
    let finish_gate = h.provider.delay_finish(SessionDirection::SourceToTarget);
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    *o.phase_write_gate.lock().unwrap() = Some(PhaseWriteGate {
        entered,
        release: blocked,
    });
    let id = o.start(h.launch.clone(), h.saved.clone()).await.unwrap().id;
    let events = o.subscribe(id).await;
    let entered = tokio::time::timeout(Duration::from_secs(5), waiting).await;
    let before_stop = o.get(id).await;
    let stop = o.stop_session(id);
    tokio::pin!(stop);
    // A Pending poll attaches this caller after electing stop. Record the result
    // rather than asserting while the lifecycle observer is still blocked.
    let first_poll = poll_fn(|cx| Poll::Ready(stop.as_mut().poll(cx))).await;
    let attached = first_poll.is_pending();
    let released = release.send(());
    drop(release);
    let finish_entered =
        tokio::time::timeout(Duration::from_secs(5), finish_gate.wait_entered()).await;
    let during_stop = o.get(id).await;
    finish_gate.release();
    let terminal = match first_poll {
        Poll::Ready(result) => Ok(result),
        Poll::Pending => tokio::time::timeout(Duration::from_secs(5), stop).await,
    };
    // The barrier has been released and cleanup driven before checking results.
    let terminal = terminal.unwrap().unwrap();
    assert!(matches!(entered, Ok(Ok(()))));
    assert!(released.is_ok());
    assert_eq!(
        before_stop.unwrap().unwrap().status,
        SessionStatus::Starting
    );
    assert!(
        attached,
        "stop did not attach to the blocked lifecycle operation"
    );
    assert_eq!(terminal.status, SessionStatus::Completed);
    assert!(matches!(finish_entered, Ok(true)));
    let during_stop = during_stop.unwrap().unwrap();
    assert_eq!(during_stop.status, SessionStatus::Stopping);
    assert_eq!(
        during_stop.runtime.unwrap().phase,
        crate::domain::launch::RuntimePhase::Stopping
    );
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(h.provider.finish_counts(), [1, 0]);
    let mut observed = Vec::new();
    let mut events = events.unwrap();
    while let Ok(event) = events.try_recv() {
        if let SessionEvent::Status { status, .. } = event {
            observed.push(status);
        }
    }
    assert!(
        !observed.iter().any(|status| status == "running"),
        "Running published after stop was admitted while durable state was Starting: {observed:?}"
    );
}
