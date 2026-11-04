use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::broadcast;
use uuid::Uuid;

use super::SessionEvent;

pub const EVENT_CAPACITY: usize = 512;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum EventBusError {
    #[error("session event channel has no subscribers")]
    NoSubscribers,
    #[error("session event subscriber lagged by {0} events")]
    Lagged(u64),
}

impl From<broadcast::error::RecvError> for EventBusError {
    fn from(error: broadcast::error::RecvError) -> Self {
        match error {
            broadcast::error::RecvError::Lagged(count) => Self::Lagged(count),
            broadcast::error::RecvError::Closed => Self::NoSubscribers,
        }
    }
}

/// The process-wide event bus owned by [`crate::app::AppState`].
#[derive(Clone, Debug)]
pub struct EventBus {
    healthy: Arc<AtomicBool>,
    sessions: Arc<Mutex<HashMap<Uuid, broadcast::Sender<SessionEvent>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            healthy: Arc::new(AtomicBool::new(true)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }

    pub fn subscribe(&self, session_id: Uuid) -> broadcast::Receiver<SessionEvent> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        sessions
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(EVENT_CAPACITY).0)
            .subscribe()
    }

    pub fn publish(&self, session_id: Uuid, event: SessionEvent) -> Result<usize, EventBusError> {
        let sender = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            sessions
                .entry(session_id)
                .or_insert_with(|| broadcast::channel(EVENT_CAPACITY).0)
                .clone()
        };
        sender.send(event).map_err(|_| EventBusError::NoSubscribers)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_receive_typed_session_events() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        let mut receiver = bus.subscribe(session_id);
        let event = SessionEvent::status(session_id, "running");

        bus.publish(session_id, event.clone()).unwrap();

        assert_eq!(receiver.recv().await.unwrap(), event);
    }

    #[tokio::test]
    async fn slow_subscribers_receive_a_typed_lagged_error() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        let mut receiver = bus.subscribe(session_id);

        for _ in 0..=EVENT_CAPACITY {
            bus.publish(session_id, SessionEvent::status(session_id, "running"))
                .unwrap();
        }

        assert!(matches!(
            EventBusError::from(receiver.recv().await.unwrap_err()),
            EventBusError::Lagged(1)
        ));
    }
}
