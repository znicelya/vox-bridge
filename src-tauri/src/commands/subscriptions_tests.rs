use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::{Notify, mpsc, watch};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{
        session::{ProviderId, SessionDirection},
        transcript::{TranscriptDetails, TranscriptKind, TranscriptUpdate},
    },
    error::AppError,
    events::{EVENT_CAPACITY, EventBus},
    storage::{LocalStore, StoredSession},
};

struct ChannelEmitter {
    output: mpsc::Sender<(String, SessionEvent)>,
    entered: Arc<Notify>,
}

#[async_trait]
impl FrontendEmitter for ChannelEmitter {
    async fn emit(&self, name: &str, event: &SessionEvent) -> AppResult<()> {
        self.entered.notify_one();
        self.output
            .send((name.to_owned(), event.clone()))
            .await
            .map_err(|_| AppError::Internal("test frontend closed".into()))
    }
}

struct AbortOnDrop(tokio::task::JoinHandle<AppResult<()>>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn next_event(receiver: &mut mpsc::Receiver<(String, SessionEvent)>) -> SessionEvent {
    tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .expect("event forwarding stalled")
        .expect("forwarder ended before required event")
        .1
}

fn row(session_id: Uuid, sequence: u64) -> TranscriptUpdate {
    TranscriptUpdate {
        id: Uuid::new_v4(),
        session_id,
        direction: if sequence % 2 == 0 {
            SessionDirection::SourceToTarget
        } else {
            SessionDirection::TargetToSource
        },
        sequence,
        occurred_at_ms: 1_799_000_000_000 + sequence as i64,
        language: "en".into(),
        text: format!("durable final {sequence}"),
        is_final: true,
        details: Some(TranscriptDetails {
            kind: TranscriptKind::Source,
            provider: ProviderId::new("fixture").unwrap(),
            provider_config_id: Uuid::nil(),
            stream_epoch: 0,
            provider_event_type: Some("fixture-final".into()),
            provider_timestamp_ms: None,
            sentence_id: None,
            local_correlation_id: None,
            log_id: None,
            start_ms: None,
            end_ms: None,
        }),
    }
}

// Catches the legacy `while let Ok` loop, loss of later terminal events, or
// snapshot recovery that duplicates finals already delivered live.
#[tokio::test]
async fn lag_emits_resync_continues_to_final_and_recovers_sqlite_pages_by_id() {
    let store = LocalStore::in_memory().unwrap();
    let session_id = Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id: session_id,
            started_at: 1_799_000_000,
            mode: "call".into(),
            direction: "bidirectional".into(),
            source_language: "en".into(),
            target_language: "fr".into(),
        })
        .unwrap();
    let bus = EventBus::new();
    let receiver = bus.subscribe(session_id);
    let (output, mut frontend) = mpsc::channel(1);
    // Fill the bounded emitter before spawning: the first live emit must block.
    output
        .send((
            "sentinel".into(),
            SessionEvent::status(session_id, "sentinel"),
        ))
        .await
        .unwrap();
    let (cancel, cancelled) = watch::channel(false);
    let entered = Arc::new(Notify::new());
    let mut task = AbortOnDrop(tokio::spawn(forward_session_events(
        session_id,
        "voxbridge://subscription/test".into(),
        receiver,
        cancelled,
        Arc::new(ChannelEmitter {
            output,
            entered: entered.clone(),
        }),
    )));
    let first = row(session_id, 0);
    store.append_transcript(&first).unwrap();
    bus.publish(
        session_id,
        SessionEvent::Transcript {
            update: first.clone(),
        },
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    let mut expected = vec![first];
    for sequence in 1..(EVENT_CAPACITY as u64 + 4) {
        let update = row(session_id, sequence);
        store.append_transcript(&update).unwrap();
        bus.publish(
            session_id,
            SessionEvent::Transcript {
                update: update.clone(),
            },
        )
        .unwrap();
        expected.push(update);
    }
    bus.close_with_final(session_id, SessionEvent::status(session_id, "completed"));
    assert!(
        matches!(next_event(&mut frontend).await, SessionEvent::Status { status, .. } if status == "sentinel")
    );
    let mut recovered = HashMap::new();
    let mut saw_resync = false;
    loop {
        match next_event(&mut frontend).await {
            SessionEvent::ResyncNeeded {
                session_id: actual,
                missed_events,
            } => {
                assert_eq!(actual, session_id);
                assert!(missed_events > 0);
                saw_resync = true;
            }
            SessionEvent::Transcript { update } => {
                recovered.insert(update.id, update);
            }
            SessionEvent::Status { status, .. } if status == "completed" => break,
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert!(saw_resync);
    assert!(recovered.contains_key(&expected.last().unwrap().id));
    let mut cursor = None;
    loop {
        let page = store.transcript_page(session_id, cursor, 37).unwrap();
        for update in page.rows {
            recovered.insert(update.id, update);
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(recovered.len(), EVENT_CAPACITY + 4);
    for update in expected {
        assert_eq!(recovered[&update.id], update);
    }
    cancel.send(true).unwrap_or(());
    tokio::time::timeout(Duration::from_secs(3), &mut task.0)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

fn emitter(capacity: usize) -> (Arc<ChannelEmitter>, mpsc::Receiver<(String, SessionEvent)>) {
    let (output, receiver) = mpsc::channel(capacity);
    (
        Arc::new(ChannelEmitter {
            output,
            entered: Arc::new(Notify::new()),
        }),
        receiver,
    )
}

// Catches an open sender incorrectly keeping a terminal forwarder alive.
#[tokio::test]
async fn terminal_status_finishes_even_when_sender_remains_open() {
    for status in ["completed", "failed"] {
        let session_id = Uuid::new_v4();
        let (sender, receiver) = broadcast::channel(4);
        sender
            .send(SessionEvent::status(session_id, status))
            .unwrap();
        let (_cancel, cancelled) = watch::channel(false);
        let (emitter, mut frontend) = emitter(4);
        let mut task = AbortOnDrop(tokio::spawn(forward_session_events(
            session_id,
            "safe-name".into(),
            receiver,
            cancelled,
            emitter,
        )));
        assert!(
            matches!(next_event(&mut frontend).await, SessionEvent::Status { status: actual, .. } if actual == status)
        );
        tokio::time::timeout(Duration::from_millis(250), &mut task.0)
            .await
            .expect("terminal forwarder retained an open sender")
            .unwrap()
            .unwrap();
        assert_eq!(sender.receiver_count(), 0);
    }
}

// Catches cancellation missing from either the receive or emit await.
#[tokio::test]
async fn cancellation_interrupts_blocked_receive_and_emit() {
    for blocked_emit in [false, true] {
        let session_id = Uuid::new_v4();
        let (sender, receiver) = broadcast::channel(4);
        let (cancel, cancelled) = watch::channel(false);
        let (emitter, _frontend) = emitter(1);
        if blocked_emit {
            emitter
                .output
                .send((
                    "sentinel".into(),
                    SessionEvent::status(session_id, "sentinel"),
                ))
                .await
                .unwrap();
            sender
                .send(SessionEvent::status(session_id, "running"))
                .unwrap();
        }
        let mut task = AbortOnDrop(tokio::spawn(forward_session_events(
            session_id,
            "safe-name".into(),
            receiver,
            cancelled,
            emitter.clone(),
        )));
        if blocked_emit {
            tokio::time::timeout(Duration::from_secs(3), emitter.entered.notified())
                .await
                .unwrap();
        }
        cancel.send(true).unwrap();
        tokio::time::timeout(Duration::from_millis(250), &mut task.0)
            .await
            .expect("cancellation did not interrupt forwarding")
            .unwrap()
            .unwrap();
        assert_eq!(sender.receiver_count(), 0);
    }
}

// Catches a frontend that never returns retaining forwarding ownership forever.
#[tokio::test]
async fn blocked_emit_is_bounded_to_one_second() {
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let (_cancel, cancelled) = watch::channel(false);
    let (emitter, _frontend) = emitter(1);
    emitter
        .output
        .send((
            "sentinel".into(),
            SessionEvent::status(session_id, "sentinel"),
        ))
        .await
        .unwrap();
    sender
        .send(SessionEvent::status(session_id, "running"))
        .unwrap();
    let started = tokio::time::Instant::now();
    let mut task = AbortOnDrop(tokio::spawn(forward_session_events(
        session_id,
        "safe-name".into(),
        receiver,
        cancelled,
        emitter,
    )));
    let result = tokio::time::timeout(Duration::from_millis(1500), &mut task.0)
        .await
        .expect("frontend emit had no one-second bound")
        .unwrap();
    assert!(matches!(result, Err(AppError::Internal(_))));
    assert!(started.elapsed() >= Duration::from_millis(950));
    assert_eq!(sender.receiver_count(), 0);
}

async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("lifecycle did not finish");
}

// Drop cancels blocked emit futures without needing a test latch to be released.
// Registry sender drop also terminates its owned worker, including on panic.
async fn subscribe(
    registry: &SubscriptionRegistry,
    session_id: Uuid,
    subscriber: &str,
    receiver: broadcast::Receiver<SessionEvent>,
) -> SubscriptionTicket {
    let (emitter, _frontend) = emitter(8);
    // Keep no emitted event pending in this helper: its streams stay idle.
    registry
        .subscribe(session_id, subscriber.into(), receiver, emitter)
        .await
        .unwrap()
}

// Catches leaked replacements or the old ticket removing/cancelling its successor.
#[tokio::test]
async fn replacement_joins_previous_forwarder_and_old_unsubscribe_is_harmless() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, first_receiver) = broadcast::channel(4);
    let first = subscribe(&registry, session_id, "window", first_receiver).await;
    let second = subscribe(&registry, session_id, "window", sender.subscribe()).await;
    assert_ne!(first.id, second.id);
    assert_ne!(first.event_name, second.event_name);
    assert_eq!(sender.receiver_count(), 1);
    registry.unsubscribe(first.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 1);
    registry.unsubscribe(second.id).await.unwrap();
    registry.unsubscribe(second.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
    registry.close_all().await.unwrap();
}

// Catches a 17th forwarder or failed capacity admission leaving a receiver owned.
#[tokio::test]
async fn seventeenth_distinct_ticket_rejects_and_replacement_works_at_capacity() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    drop(receiver);
    for subscriber in 0..16 {
        subscribe(
            &registry,
            session_id,
            &format!("subscriber{subscriber}"),
            sender.subscribe(),
        )
        .await;
    }
    let (emitter, _frontend) = emitter(1);
    assert!(matches!(
        registry
            .subscribe(session_id, "overflow".into(), sender.subscribe(), emitter)
            .await,
        Err(AppError::InvalidRequest(_))
    ));
    assert_eq!(sender.receiver_count(), 16);
    subscribe(&registry, session_id, "subscriber0", sender.subscribe()).await;
    assert_eq!(sender.receiver_count(), 16);
    registry.close_all().await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}

// Catches cross-window or cross-session cancellation and permanently closed registries.
#[tokio::test]
async fn close_session_isolates_subscribers_and_close_all_can_be_reused() {
    let registry = SubscriptionRegistry::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let (sender_a, receiver_a) = broadcast::channel(4);
    let (sender_b, receiver_b) = broadcast::channel(4);
    let first = subscribe(&registry, a, "window", receiver_a).await;
    subscribe(&registry, a, "second-window", sender_a.subscribe()).await;
    let other = subscribe(&registry, b, "window", receiver_b).await;
    assert_eq!(sender_a.receiver_count(), 2);
    registry.close_session(a).await.unwrap();
    registry.close_session(a).await.unwrap();
    assert_eq!(sender_a.receiver_count(), 0);
    assert_eq!(sender_b.receiver_count(), 1);
    registry.unsubscribe(first.id).await.unwrap();
    registry.close_all().await.unwrap();
    registry.close_all().await.unwrap();
    assert_eq!(sender_b.receiver_count(), 0);
    let fresh = subscribe(&registry, b, "window", sender_b.subscribe()).await;
    assert_ne!(fresh.id, other.id);
    registry.unsubscribe(other.id).await.unwrap();
    assert_eq!(sender_b.receiver_count(), 1);
    registry.unsubscribe(fresh.id).await.unwrap();
    assert_eq!(sender_b.receiver_count(), 0);
}

// Catches finishing wrappers retaining capacity after terminal delivery or emit error.
#[tokio::test]
async fn terminal_closed_stream_and_emit_failure_release_owned_slots() {
    let registry = SubscriptionRegistry::new();
    for finish in ["completed", "failed", "closed", "emit-error"] {
        let session_id = Uuid::new_v4();
        let (sender, receiver) = broadcast::channel(4);
        let (emitter, mut frontend) = emitter(4);
        registry
            .subscribe(session_id, "window".into(), receiver, emitter)
            .await
            .unwrap();
        match finish {
            "closed" => {
                drop(sender);
            }
            "emit-error" => {
                drop(frontend);
                sender
                    .send(SessionEvent::status(session_id, "running"))
                    .unwrap();
                until(|| sender.receiver_count() == 0).await;
            }
            status => {
                sender
                    .send(SessionEvent::status(session_id, status))
                    .unwrap();
                assert!(
                    matches!(next_event(&mut frontend).await, SessionEvent::Status { status: actual, .. } if actual == status)
                );
                until(|| sender.receiver_count() == 0).await;
            }
        }
    }
    // All slots must now be reusable without requiring unsubscribe.
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    drop(receiver);
    for subscriber in 0..16 {
        subscribe(
            &registry,
            session_id,
            &format!("fresh{subscriber}"),
            sender.subscribe(),
        )
        .await;
    }
    registry.close_all().await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}

// Catches event names containing a token and unsafe echoing of rejected input.
#[tokio::test]
async fn opaque_subscriber_token_is_bounded_and_absent_from_ticket_name() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    drop(receiver);
    for invalid in [String::new(), "s".repeat(65), "界".repeat(65)] {
        let (emitter, _frontend) = emitter(1);
        let result = registry
            .subscribe(session_id, invalid, sender.subscribe(), emitter)
            .await;
        assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    }
    assert_eq!(sender.receiver_count(), 0);
    let token = "界".repeat(64);
    let ticket = subscribe(&registry, session_id, &token, sender.subscribe()).await;
    assert!(!ticket.event_name.contains(&token));
    assert!(ticket.event_name.ends_with(&ticket.id.to_string()));
    registry.close_all().await.unwrap();
}

// Catches new() eagerly requiring a Tokio runtime.
#[test]
fn registry_constructs_outside_tokio() {
    drop(SubscriptionRegistry::new());
}

// Catches worker/sender cycles that retain forwarders after registry destruction.
#[tokio::test]
async fn dropping_last_registry_owner_drains_forwarders() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    subscribe(&registry, session_id, "window", receiver).await;
    drop(registry);
    until(|| sender.receiver_count() == 0).await;
}

// Uses a resolved receiver containing a durable final, without any replay cache.
// Orchestrator/restart receiver resolution is intentionally Task6B's boundary.
#[tokio::test]
async fn prequeued_terminal_receiver_delivers_then_releases_ticket() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let terminal = SessionEvent::status(session_id, "completed");
    let (sender, receiver) = broadcast::channel(1);
    sender.send(terminal.clone()).unwrap();
    drop(sender);
    let (emitter, mut frontend) = emitter(1);
    let ticket = registry
        .subscribe(session_id, "window".into(), receiver, emitter)
        .await
        .unwrap();
    let (event_name, delivered) = tokio::time::timeout(Duration::from_secs(3), frontend.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event_name, ticket.event_name);
    assert_eq!(delivered, terminal);
    registry.unsubscribe(ticket.id).await.unwrap();
    registry.close_all().await.unwrap();
}

// Catches owned replacement operations leaving an unclaimed ticket after caller cancellation.
#[tokio::test]
async fn cancelled_accepted_replacement_joins_old_and_does_not_keep_new() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let first = subscribe(&registry, session_id, "window", receiver).await;
    let (emitter, _frontend) = emitter(1);
    let mut replacement =
        Box::pin(registry.subscribe(session_id, "window".into(), sender.subscribe(), emitter));
    assert!(futures_util::poll!(&mut replacement).is_pending());
    drop(replacement);
    // A later FIFO operation waits for replacement cleanup without hiding an
    // unclaimed new generation by cancelling every ticket.
    registry.unsubscribe(first.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
    subscribe(&registry, session_id, "window", sender.subscribe()).await;
    registry.close_all().await.unwrap();
}

// Catches close_all completing before already-admitted subscriptions insert.
#[tokio::test]
async fn close_all_drains_prebarrier_subscriptions_and_allows_fresh_generation() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    drop(receiver);
    let mut pending = Vec::new();
    for subscriber in 0..12 {
        let (emitter, _frontend) = emitter(1);
        let mut admission = Box::pin(registry.subscribe(
            session_id,
            format!("pending{subscriber}"),
            sender.subscribe(),
            emitter,
        ));
        assert!(futures_util::poll!(&mut admission).is_pending());
        pending.push(admission);
    }
    registry.close_all().await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
    let mut old_tickets = Vec::new();
    for admission in pending {
        old_tickets.push(admission.await.unwrap());
    }
    let fresh = subscribe(&registry, session_id, "pending0", sender.subscribe()).await;
    for old in old_tickets {
        registry.unsubscribe(old.id).await.unwrap();
    }
    assert_eq!(sender.receiver_count(), 1);
    registry.unsubscribe(fresh.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}

// Catches shutdown cleanup tied to the lifetime of its command caller.
#[tokio::test]
async fn cancelling_close_all_caller_does_not_detach_cleanup() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    subscribe(&registry, session_id, "window", receiver).await;
    let mut close = Box::pin(registry.close_all());
    assert!(futures_util::poll!(&mut close).is_pending());
    drop(close);
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}

// Catches cancellation while the owner is joining, rather than only before dequeue.
// FIFO mutex waiters hold the old finishing wrapper without an unsafe/blocking latch.
#[tokio::test]
async fn cancelled_replacement_during_old_join_finishes_owned_cleanup() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let (old_emitter, _frontend) = emitter(1);
    old_emitter
        .output
        .send((
            "sentinel".into(),
            SessionEvent::status(session_id, "sentinel"),
        ))
        .await
        .unwrap();
    let first = registry
        .subscribe(session_id, "window".into(), receiver, old_emitter.clone())
        .await
        .unwrap();
    sender
        .send(SessionEvent::status(session_id, "running"))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), old_emitter.entered.notified())
        .await
        .unwrap();

    let map_guard = registry.entries.lock().await;
    let (new_emitter, _new_frontend) = emitter(1);
    let mut replacement =
        Box::pin(registry.subscribe(session_id, "window".into(), sender.subscribe(), new_emitter));
    assert!(futures_util::poll!(&mut replacement).is_pending());
    // The owner has dequeued and is waiting for the currently held map lock.
    until(|| registry.sender.capacity() == MUTATION_CAPACITY).await;
    let mut cleanup_barrier = Box::pin(registry.entries.lock());
    assert!(futures_util::poll!(&mut cleanup_barrier).is_pending());
    drop(map_guard);
    let cleanup_guard = cleanup_barrier.await;
    assert!(cleanup_guard.is_empty());
    // Old receiver is dropped, but its finishing wrapper cannot complete while
    // this guard is held; the owner is awaiting its JoinHandle.
    until(|| sender.receiver_count() == 1).await;
    drop(replacement);
    drop(cleanup_guard);
    registry.unsubscribe(first.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
    assert!(registry.entries.lock().await.is_empty());
    registry.close_all().await.unwrap();
}

// Exercises the actual finishing-wrapper removal branch with a late old UUID.
#[tokio::test]
async fn late_old_generation_completion_cannot_remove_replacement() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let first = subscribe(&registry, session_id, "window", receiver).await;
    let second = subscribe(&registry, session_id, "window", sender.subscribe()).await;
    assert!(
        take_generation(&registry.entries, &(session_id, "window".into()), first.id)
            .await
            .is_none()
    );
    assert_eq!(
        registry.entries.lock().await[&(session_id, "window".into())].id,
        second.id
    );
    assert_eq!(sender.receiver_count(), 1);
    registry.unsubscribe(second.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}

// Catches unbounded send-await admission or spawning a task for every caller.
#[tokio::test]
async fn mutation_admission_is_bounded_and_rejected_receiver_is_dropped() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    drop(receiver);
    let map_guard = registry.entries.lock().await;
    let mut pending = Vec::new();
    let (first_emitter, _frontend) = emitter(1);
    let mut first = Box::pin(registry.subscribe(
        session_id,
        "executing".into(),
        sender.subscribe(),
        first_emitter,
    ));
    assert!(futures_util::poll!(&mut first).is_pending());
    until(|| registry.sender.capacity() == MUTATION_CAPACITY).await;
    pending.push(first);
    for index in 0..16 {
        let (emitter, _frontend) = emitter(1);
        let mut request = Box::pin(registry.subscribe(
            session_id,
            format!("waiting{index}"),
            sender.subscribe(),
            emitter,
        ));
        assert!(futures_util::poll!(&mut request).is_pending());
        pending.push(request);
    }
    let (emitter, _frontend) = emitter(1);
    let mut rejected =
        Box::pin(registry.subscribe(session_id, "overflow".into(), sender.subscribe(), emitter));
    assert!(matches!(
        futures_util::poll!(&mut rejected),
        std::task::Poll::Ready(Err(AppError::InvalidRequest(_)))
    ));
    drop(rejected);
    assert_eq!(sender.receiver_count(), 17);
    assert!(map_guard.is_empty());
    drop(pending);
    drop(map_guard);
    until(|| sender.receiver_count() == 0).await;
    registry.close_all().await.unwrap();
    subscribe(&registry, session_id, "fresh", sender.subscribe()).await;
    registry.close_all().await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}

// Catches failed lazy startup admitting a detached operation or retaining its input.
#[tokio::test]
async fn closed_mutation_owner_returns_error_without_owning_receiver() {
    let registry = SubscriptionRegistry::new();
    drop(registry.receiver.lock().unwrap().take());
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let (emitter, _frontend) = emitter(1);
    assert!(matches!(
        registry
            .subscribe(session_id, "window".into(), receiver, emitter)
            .await,
        Err(AppError::Internal(_))
    ));
    assert_eq!(sender.receiver_count(), 0);
    assert!(registry.entries.lock().await.is_empty());
    assert!(matches!(
        registry.close_all().await,
        Err(AppError::Internal(_))
    ));
}

// Catches cross-subscriber cancellation and lost direction metadata in forwarding.
#[tokio::test]
async fn independent_subscribers_receive_both_directions_and_unsubscribe_isolated() {
    let registry = SubscriptionRegistry::new();
    let bus = EventBus::new();
    let session_id = Uuid::new_v4();
    let (emitter_a, mut frontend_a) = emitter(4);
    let (emitter_b, mut frontend_b) = emitter(4);
    let a = registry
        .subscribe(session_id, "a".into(), bus.subscribe(session_id), emitter_a)
        .await
        .unwrap();
    let b = registry
        .subscribe(session_id, "b".into(), bus.subscribe(session_id), emitter_b)
        .await
        .unwrap();
    assert_ne!(a.event_name, b.event_name);
    for sequence in [0, 1] {
        let update = row(session_id, sequence);
        let expected = SessionEvent::Transcript { update };
        bus.publish(session_id, expected.clone()).unwrap();
        assert_eq!(next_event(&mut frontend_a).await, expected);
        assert_eq!(next_event(&mut frontend_b).await, expected);
    }
    registry.unsubscribe(a.id).await.unwrap();
    let expected = SessionEvent::Transcript {
        update: row(session_id, 2),
    };
    assert_eq!(bus.publish(session_id, expected.clone()).unwrap(), 1);
    assert_eq!(next_event(&mut frontend_b).await, expected);
    registry.close_all().await.unwrap();
    assert!(frontend_a.recv().await.is_none());
    assert!(frontend_b.recv().await.is_none());
}

// Catches the race where cancellation happens after the owner's is_closed check.
#[tokio::test]
async fn closed_ticket_reply_after_precheck_cancels_unclaimed_generation() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let map_guard = registry.entries.lock().await;
    let (emitter, _frontend) = emitter(1);
    let mut admission =
        Box::pin(registry.subscribe(session_id, "window".into(), receiver, emitter));
    assert!(futures_util::poll!(&mut admission).is_pending());
    until(|| registry.sender.capacity() == MUTATION_CAPACITY).await;
    let mut insertion_barrier = Box::pin(registry.entries.lock());
    assert!(futures_util::poll!(&mut insertion_barrier).is_pending());
    drop(map_guard);
    let insertion_guard = insertion_barrier.await;
    // The owner took the first map lock, observed an open response, and now
    // waits for this guard before insertion. Force the later send-error path.
    assert!(insertion_guard.is_empty());
    drop(admission);
    drop(insertion_guard);
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
    assert!(registry.entries.lock().await.is_empty());
    registry.close_all().await.unwrap();
}

// Catches a successful oneshot send being abandoned before subscribe consumes it.
#[tokio::test]
async fn abandoning_successfully_sent_ticket_cancels_unclaimed_forwarder() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let (emitter, _frontend) = emitter(1);
    let mut admission =
        Box::pin(registry.subscribe(session_id, "window".into(), receiver, emitter));
    assert!(futures_util::poll!(&mut admission).is_pending());
    // This later FIFO operation proves the owner has sent the ticket response;
    // deliberately leave the subscribe future unpolled until it is dropped.
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    assert_eq!(registry.entries.lock().await.len(), 1);
    drop(admission);
    until(|| sender.receiver_count() == 0).await;
    registry.close_all().await.unwrap();
    assert!(registry.entries.lock().await.is_empty());
}

// Catches an abandoned old response guard cancelling a newer key generation.
#[tokio::test]
async fn dropping_old_response_guard_does_not_cancel_claimed_replacement() {
    let registry = SubscriptionRegistry::new();
    let session_id = Uuid::new_v4();
    let (sender, receiver) = broadcast::channel(4);
    let (emitter, _frontend) = emitter(1);
    let mut old_response =
        Box::pin(registry.subscribe(session_id, "window".into(), receiver, emitter));
    assert!(futures_util::poll!(&mut old_response).is_pending());
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    let fresh = subscribe(&registry, session_id, "window", sender.subscribe()).await;
    drop(old_response);
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    assert_eq!(sender.receiver_count(), 1);
    assert_eq!(
        registry.entries.lock().await[&(session_id, "window".into())].id,
        fresh.id
    );
    registry.unsubscribe(fresh.id).await.unwrap();
    assert_eq!(sender.receiver_count(), 0);
}
