use super::*;
use crate::{events::SessionEvent, session::SessionStatus};
use tokio::sync::mpsc;

struct Emitter(mpsc::Sender<SessionEvent>);
#[async_trait::async_trait]
impl FrontendEmitter for Emitter {
    async fn emit(&self, _: &str, event: &SessionEvent) -> AppResult<()> {
        self.0
            .send(event.clone())
            .await
            .map_err(|_| AppError::Internal("test frontend closed".into()))
    }
}
fn emitter() -> (Arc<dyn FrontendEmitter>, mpsc::Receiver<SessionEvent>) {
    let (sender, receiver) = mpsc::channel(32);
    (Arc::new(Emitter(sender)), receiver)
}
async fn next(receiver: &mut mpsc::Receiver<SessionEvent>) -> SessionEvent {
    tokio::time::timeout(std::time::Duration::from_secs(10), receiver.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn active_and_restarted_terminal_subscriptions_use_runtime_and_durable_history() {
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = tests::injected(dir.path(), &h, secrets.clone());
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let record = app.create_session(h.launch.clone()).await.unwrap();
    let (out, mut events) = emitter();
    let ticket = app
        .subscribe_session(record.id, "window-one".into(), out)
        .await
        .unwrap();
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status {session_id, ..} if session_id == record.id)
    );
    app.stop_session(record.id).await.unwrap();
    while !matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "completed")
    {
    }
    app.unsubscribe_session(ticket.id).await.unwrap();
    app.unsubscribe_session(ticket.id).await.unwrap();
    drop(app);
    let app = tests::injected(dir.path(), &h, secrets);
    assert_eq!(
        app.get_session(record.id).await.unwrap().unwrap().status,
        SessionStatus::Completed
    );
    assert!(app.event_bus().subscribe_or_replay(record.id).is_none());
    let (out, mut events) = emitter();
    let ticket = app
        .subscribe_session(record.id, "restarted-window".into(), out)
        .await
        .unwrap();
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "completed")
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .is_none()
    );
    app.delete_session(record.id).await.unwrap();
    assert!(app.get_session(record.id).await.unwrap().is_none());
    let (out, _) = emitter();
    assert!(
        app.subscribe_session(record.id, "deleted".into(), out)
            .await
            .is_err()
    );
    app.unsubscribe_session(ticket.id).await.unwrap();
}

#[tokio::test]
async fn closing_starting_runtime_cleans_ports_and_blocked_emit_even_when_caller_cancels() {
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let started = h
        .provider
        .delay_started(crate::domain::session::SessionDirection::SourceToTarget);
    let record = app.create_session(h.launch.clone()).await.unwrap();
    assert!(started.wait_entered().await);
    let (sender, mut receiver) = mpsc::channel(1);
    let output = Arc::new(Emitter(sender));
    app.subscribe_session(record.id, "window".into(), output)
        .await
        .unwrap();
    let _ = next(&mut receiver).await;
    let owner = app.clone();
    let close = tokio::spawn(async move { owner.close_all().await });
    tokio::task::yield_now().await;
    close.abort();
    started.release();
    app.close_all().await.unwrap();
    assert_eq!(h.audio.open_ports(), 0);
    assert!(matches!(
        app.get_session(record.id).await.unwrap().unwrap().status,
        SessionStatus::Failed | SessionStatus::Completed
    ));
    assert!(app.create_session(h.launch.clone()).await.is_err());
    app.open_window().unwrap();
    let (out, mut events) = emitter();
    app.subscribe_session(record.id, "new-window".into(), out)
        .await
        .unwrap();
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "completed" || status == "failed")
    );
}

struct DropLatch {
    released: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
}
impl DropLatch {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct ReleaseLatch(Arc<DropLatch>);
impl Drop for ReleaseLatch {
    fn drop(&mut self) {
        self.0.release();
    }
}
struct BlockingDrop {
    latch: Arc<DropLatch>,
    entered: Option<tokio::sync::oneshot::Sender<()>>,
}
impl Drop for BlockingDrop {
    fn drop(&mut self) {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
        }
        let guard = self.latch.released.lock().unwrap();
        let _ = self
            .latch
            .changed
            .wait_timeout_while(guard, std::time::Duration::from_secs(10), |released| {
                !*released
            })
            .unwrap();
    }
}
struct DropBlockedEmitter {
    latch: Arc<DropLatch>,
    emit_entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    drop_entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}
#[async_trait::async_trait]
impl FrontendEmitter for DropBlockedEmitter {
    async fn emit(&self, _: &str, _: &SessionEvent) -> AppResult<()> {
        let _guard = BlockingDrop {
            latch: self.latch.clone(),
            entered: self.drop_entered.lock().unwrap().take(),
        };
        if let Some(entered) = self.emit_entered.lock().unwrap().take() {
            let _ = entered.send(());
        }
        std::future::pending().await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_observes_registry_barrier_even_when_its_queue_is_full_and_stops_audio_independently()
{
    use std::{
        future::Future,
        task::{Context, Poll},
    };
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap();
    tests::eventually(|| {
        app.store
            .get_session_lifecycle(session.id)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    assert_eq!(h.audio.open_ports(), 1);
    let latch = Arc::new(DropLatch {
        released: std::sync::Mutex::new(false),
        changed: std::sync::Condvar::new(),
    });
    let release = ReleaseLatch(latch.clone());
    let (emit_tx, emit_rx) = tokio::sync::oneshot::channel();
    let (drop_tx, drop_rx) = tokio::sync::oneshot::channel();
    let out = Arc::new(DropBlockedEmitter {
        latch,
        emit_entered: std::sync::Mutex::new(Some(emit_tx)),
        drop_entered: std::sync::Mutex::new(Some(drop_tx)),
    });
    app.subscribe_session(session.id, "blocked".into(), out)
        .await
        .unwrap();
    emit_rx.await.unwrap();
    // Saturate the real producer queue through an owned replacement that must
    // join its predecessor. The bounded app close must observe actual admission.
    let receiver = app.orchestrator.subscribe(session.id).await.unwrap();
    let registry = app.subscriptions.clone();
    let (out, _events) = emitter();
    let replacement = tokio::spawn(async move {
        registry
            .subscribe(session.id, "blocked".into(), receiver, out)
            .await
    });
    drop_rx.await.unwrap();
    let mut waiting = Vec::new();
    for _ in 0..16 {
        let mut unsubscribe = Box::pin(app.unsubscribe_session(Uuid::new_v4()));
        assert!(matches!(
            unsubscribe.as_mut().poll(&mut Context::from_waker(
                futures_util::task::noop_waker_ref()
            )),
            Poll::Pending
        ));
        waiting.push(unsubscribe);
    }
    assert!(
        app.unsubscribe_session(Uuid::new_v4())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("queue is full")
    );
    let owner = app.clone();
    let close = tokio::spawn(async move { owner.close_all().await });
    tests::eventually(|| h.audio.open_ports() == 0).await;
    assert!(
        !close.is_finished(),
        "close must not discard full-queue rejection"
    );
    let (out, _) = emitter();
    assert!(
        app.subscribe_session(session.id, "late-old-window".into(), out)
            .await
            .is_err()
    );
    let error = close.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("close admission timed out"));
    assert!(
        app.open_window().is_err(),
        "failed cleanup cannot reopen admission"
    );
    drop(release);
    app.close_all().await.unwrap();
    drop(waiting);
    let _ = replacement.await.unwrap();
    app.open_window().unwrap();
    let (out, mut events) = emitter();
    app.subscribe_session(session.id, "new-window".into(), out)
        .await
        .unwrap();
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "completed")
    );
}

#[tokio::test]
async fn deletion_joins_blocked_terminal_delivery_and_evicted_history_gets_fresh_status() {
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap();
    app.stop_session(session.id).await.unwrap();
    for _ in 0..300 {
        let id = Uuid::new_v4();
        app.event_bus
            .close_with_final(id, SessionEvent::status(id, "completed"));
    }
    assert!(app.event_bus.subscribe_or_replay(session.id).is_none());
    let (out, mut events) = emitter();
    app.subscribe_session(session.id, "evicted".into(), out)
        .await
        .unwrap();
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "completed")
    );
    let (sender, mut events) = mpsc::channel(1);
    sender
        .send(SessionEvent::status(session.id, "occupied"))
        .await
        .unwrap();
    app.subscribe_session(
        session.id,
        "blocked-terminal".into(),
        Arc::new(Emitter(sender)),
    )
    .await
    .unwrap();
    app.delete_session(session.id).await.unwrap();
    assert!(app.get_session(session.id).await.unwrap().is_none());
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "occupied")
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn queued_old_window_subscription_cannot_finish_into_new_lifecycle() {
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap();
    let guard = app.subscription_mutation.clone().lock_owned().await;
    let owner = app.clone();
    let (out, _events) = emitter();
    let old = tokio::spawn(async move {
        owner
            .subscribe_session(session.id, "same-token".into(), out)
            .await
    });
    tests::eventually(|| app.subscription_admission.available_permits() == 15).await;
    let owner = app.clone();
    let close = tokio::spawn(async move { owner.close_all().await });
    tests::eventually(|| app.admitted_epoch().is_err()).await;
    drop(guard);
    assert!(old.await.unwrap().is_err());
    close.await.unwrap().unwrap();
    app.open_window().unwrap();
    let (out, mut events) = emitter();
    app.subscribe_session(session.id, "same-token".into(), out)
        .await
        .unwrap();
    assert!(
        matches!(next(&mut events).await, SessionEvent::Status { status, .. } if status == "completed")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retry_delete_after_registry_admission_failure_preserves_durable_row_until_cleanup() {
    use std::{
        future::Future,
        task::{Context, Poll},
    };
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap();
    app.stop_session(session.id).await.unwrap();
    let latch = Arc::new(DropLatch {
        released: std::sync::Mutex::new(false),
        changed: std::sync::Condvar::new(),
    });
    let release = ReleaseLatch(latch.clone());
    let (emit_tx, emit_rx) = tokio::sync::oneshot::channel();
    let (drop_tx, drop_rx) = tokio::sync::oneshot::channel();
    app.subscribe_session(
        session.id,
        "blocked".into(),
        Arc::new(DropBlockedEmitter {
            latch,
            emit_entered: std::sync::Mutex::new(Some(emit_tx)),
            drop_entered: std::sync::Mutex::new(Some(drop_tx)),
        }),
    )
    .await
    .unwrap();
    emit_rx.await.unwrap();
    let receiver = app.orchestrator.subscribe(session.id).await.unwrap();
    let registry = app.subscriptions.clone();
    let (output, mut delivery) = mpsc::channel(1);
    output
        .send(SessionEvent::status(session.id, "occupied"))
        .await
        .unwrap();
    let out = Arc::new(Emitter(output));
    let replacement = tokio::spawn(async move {
        registry
            .subscribe(session.id, "blocked".into(), receiver, out)
            .await
    });
    drop_rx.await.unwrap();
    let mut waiting = Vec::new();
    for _ in 0..16 {
        let mut unsubscribe = Box::pin(app.unsubscribe_session(Uuid::new_v4()));
        assert!(matches!(
            unsubscribe.as_mut().poll(&mut Context::from_waker(
                futures_util::task::noop_waker_ref()
            )),
            Poll::Pending
        ));
        waiting.push(unsubscribe);
    }
    let first = app.delete_session(session.id).await.unwrap_err();
    assert!(first.to_string().contains("close admission timed out"));
    assert!(app.get_session(session.id).await.unwrap().is_some());
    drop(release);
    let retry = app.delete_session(session.id).await;
    let delivered = delivery.recv().await;
    let forwarder_closed =
        tokio::time::timeout(std::time::Duration::from_secs(2), delivery.recv()).await;
    let removed = app.get_session(session.id).await.unwrap().is_none();
    // Release every owner before asserting the recovery contract.
    app.close_all().await.unwrap();
    drop(waiting);
    let _ = replacement.await.unwrap();
    assert!(
        retry.is_ok(),
        "deletion retry must observe pending cleanup; got {retry:?}"
    );
    assert!(removed);
    assert!(matches!(delivered, Some(SessionEvent::Status { status, .. }) if status == "occupied"));
    assert!(matches!(forwarder_closed, Ok(None)));
}

#[tokio::test]
async fn active_session_delete_rejects_before_cancelling_its_subscription() {
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap();
    let (out, mut events) = emitter();
    app.subscribe_session(session.id, "active-window".into(), out)
        .await
        .unwrap();
    let result = app.delete_session(session.id).await;
    let publication = app
        .event_bus
        .publish(session.id, SessionEvent::status(session.id, "still-active"));
    let still_subscribed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Some(event) = events.recv().await {
            if matches!(event, SessionEvent::Status { status, .. } if status == "still-active") {
                return true;
            }
        }
        false
    })
    .await;
    app.stop_session(session.id).await.unwrap();
    app.close_all().await.unwrap();
    assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    assert!(publication.is_ok());
    assert!(matches!(still_subscribed, Ok(true)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_delete_caller_still_owns_cleanup_before_durable_removal() {
    let h = tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap();
    app.stop_session(session.id).await.unwrap();
    let latch = Arc::new(DropLatch {
        released: std::sync::Mutex::new(false),
        changed: std::sync::Condvar::new(),
    });
    let release = ReleaseLatch(latch.clone());
    let (emit_tx, emit_rx) = tokio::sync::oneshot::channel();
    let (drop_tx, drop_rx) = tokio::sync::oneshot::channel();
    app.subscribe_session(
        session.id,
        "blocked".into(),
        Arc::new(DropBlockedEmitter {
            latch,
            emit_entered: std::sync::Mutex::new(Some(emit_tx)),
            drop_entered: std::sync::Mutex::new(Some(drop_tx)),
        }),
    )
    .await
    .unwrap();
    emit_rx.await.unwrap();
    let owner = app.clone();
    let caller = tokio::spawn(async move { owner.delete_session(session.id).await });
    drop_rx.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(app.get_session(session.id).await.unwrap().is_some());
    drop(release);
    tests::eventually(|| app.store.get_session(session.id).unwrap().is_none()).await;
    assert!(matches!(
        app.delete_session(Uuid::new_v4()).await,
        Err(AppError::SessionNotFound(_))
    ));
    app.close_all().await.unwrap();
}
