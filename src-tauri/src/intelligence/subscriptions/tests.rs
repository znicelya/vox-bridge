use super::*;
use std::time::Duration;
#[derive(Default)]
struct Emitter {
    events: tokio::sync::Mutex<Vec<IntelligenceEvent>>,
}
#[async_trait::async_trait]
impl IntelligenceEmitter for Emitter {
    async fn emit(&self, _: &str, event: &IntelligenceEvent) -> AppResult<()> {
        self.events.lock().await.push(event.clone());
        Ok(())
    }
}
async fn count(e: &Emitter, n: usize) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while e.events.lock().await.len() < n {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn intelligence_subscriptions_lag_continues_and_close_releases_capacity() {
    let registry = IntelligenceSubscriptions::new();
    let (tx, rx) = broadcast::channel(1);
    let e = Arc::new(Emitter::default());
    for i in 0..4 {
        let _ = tx.send(IntelligenceEvent::ResyncNeeded { missed_events: i });
    }
    let ticket = registry
        .subscribe("first".into(), rx, e.clone())
        .await
        .unwrap();
    count(&e, 2).await;
    assert!(matches!(
        e.events.lock().await[0],
        IntelligenceEvent::ResyncNeeded { missed_events: 3 }
    ));
    tx.send(IntelligenceEvent::ResyncNeeded { missed_events: 99 })
        .unwrap();
    count(&e, 3).await;
    for i in 0..15 {
        registry
            .subscribe(format!("other{i}"), tx.subscribe(), e.clone())
            .await
            .unwrap();
    }
    assert!(
        registry
            .subscribe("overflow".into(), tx.subscribe(), e.clone())
            .await
            .is_err()
    );
    let replacement = registry
        .subscribe("first".into(), tx.subscribe(), e.clone())
        .await
        .unwrap();
    assert_ne!(replacement.id, ticket.id);
    registry.unsubscribe(ticket.id).await.unwrap();
    registry.close_all().await.unwrap();
    let before = e.events.lock().await.len();
    let _ = tx.send(IntelligenceEvent::ResyncNeeded { missed_events: 100 });
    tokio::task::yield_now().await;
    assert_eq!(e.events.lock().await.len(), before);
    registry
        .subscribe("new".into(), tx.subscribe(), e)
        .await
        .unwrap();
    registry.close_all().await.unwrap();
}

#[tokio::test]
async fn intelligence_subscriptions_unclaimed_ticket_and_cancelled_close_own_cleanup() {
    let registry = IntelligenceSubscriptions::new();
    let (tx, rx) = broadcast::channel(4);
    let e = Arc::new(Emitter::default());
    let mut admission = Box::pin(registry.subscribe("window".into(), rx, e.clone()));
    assert!(futures_util::poll!(&mut admission).is_pending());
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    assert_eq!(registry.entries.lock().await.len(), 1);
    drop(admission);
    tokio::time::timeout(Duration::from_secs(1), async {
        while tx.receiver_count() != 0 {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    registry
        .subscribe("fresh".into(), tx.subscribe(), e)
        .await
        .unwrap();
    let mut close = Box::pin(registry.close_all());
    assert!(futures_util::poll!(&mut close).is_pending());
    drop(close);
    registry.unsubscribe(Uuid::new_v4()).await.unwrap();
    assert_eq!(tx.receiver_count(), 0);
}
#[tokio::test]
async fn intelligence_subscriptions_terminal_job_does_not_end_global_stream() {
    use crate::intelligence::{test_support::*, types::*};
    let h = IntelligenceHarness::new();
    let id = h.session();
    let mut job = h
        .store
        .transaction(|tx| super::super::store::admit_job(tx, manual(id)))
        .unwrap()
        .view()
        .clone();
    job.status = JobStatus::Succeeded;
    let registry = IntelligenceSubscriptions::new();
    let (tx, rx) = broadcast::channel(4);
    let e = Arc::new(Emitter::default());
    registry
        .subscribe("window".into(), rx, e.clone())
        .await
        .unwrap();
    tx.send(IntelligenceEvent::JobChanged { job }).unwrap();
    count(&e, 1).await;
    tx.send(IntelligenceEvent::ResyncNeeded { missed_events: 1 })
        .unwrap();
    count(&e, 2).await;
    registry.close_all().await.unwrap();
}
#[tokio::test]
async fn intelligence_subscriptions_mutation_queue_is_bounded_and_dropped_waiters_release_receivers()
 {
    let registry = IntelligenceSubscriptions::new();
    let (tx, rx) = broadcast::channel(4);
    drop(rx);
    let e = Arc::new(Emitter::default());
    let guard = registry.entries.lock().await;
    let mut pending = Vec::new();
    let mut first = Box::pin(registry.subscribe("executing".into(), tx.subscribe(), e.clone()));
    assert!(futures_util::poll!(&mut first).is_pending());
    tokio::time::timeout(Duration::from_secs(1), async {
        while registry.sender.capacity() != MUTATION_CAPACITY {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    pending.push(first);
    for i in 0..16 {
        let mut next = Box::pin(registry.subscribe(format!("wait{i}"), tx.subscribe(), e.clone()));
        assert!(futures_util::poll!(&mut next).is_pending());
        pending.push(next);
    }
    assert!(
        registry
            .subscribe("overflow".into(), tx.subscribe(), e)
            .await
            .is_err()
    );
    assert_eq!(tx.receiver_count(), 17);
    drop(pending);
    drop(guard);
    tokio::time::timeout(Duration::from_secs(1), async {
        while tx.receiver_count() != 0 {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    registry.close_all().await.unwrap();
}
