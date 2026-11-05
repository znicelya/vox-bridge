use std::{
    collections::HashMap,
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::broadcast;
use uuid::Uuid;

use super::SessionEvent;

pub const EVENT_CAPACITY: usize = 512;
pub const TERMINAL_REPLAY_CAPACITY: usize = 256;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum EventBusError {
    #[error("session event channel has no subscribers")]
    NoSubscribers,
    #[error("session event subscriber lagged by {0} events")]
    Lagged(u64),
    #[error("event session ID does not match routing session ID")]
    SessionMismatch,
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
    state: Arc<Mutex<BusState>>,
}

#[derive(Debug, Default)]
struct BusState {
    sessions: HashMap<Uuid, broadcast::Sender<SessionEvent>>,
    terminal_events: HashMap<Uuid, SessionEvent>,
    terminal_order: VecDeque<Uuid>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            healthy: Arc::new(AtomicBool::new(true)),
            state: Arc::new(Mutex::new(BusState::default())),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
    }

    pub fn subscribe(&self, session_id: Uuid) -> broadcast::Receiver<SessionEvent> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(event) = state.terminal_events.get(&session_id).cloned() {
            let (sender, receiver) = broadcast::channel(1);
            let _ = sender.send(event);
            return receiver;
        }
        state
            .sessions
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(EVENT_CAPACITY).0)
            .subscribe()
    }

    pub fn open_session(&self, session_id: Uuid) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .sessions
            .entry(session_id)
            .or_insert_with(|| broadcast::channel(EVENT_CAPACITY).0);
        state.terminal_events.remove(&session_id);
        state.terminal_order.retain(|id| *id != session_id);
    }

    /// Subscribe only when a live channel already exists. This never creates a
    /// new channel for a session whose stream has been closed.
    pub fn try_subscribe(&self, session_id: Uuid) -> Option<broadcast::Receiver<SessionEvent>> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .sessions
            .get(&session_id)
            .map(broadcast::Sender::subscribe)
    }

    /// Subscribe to a live stream, or receive its terminal event once when the
    /// stream has already closed. Does not create a persistent channel.
    pub fn subscribe_or_replay(
        &self,
        session_id: Uuid,
    ) -> Option<broadcast::Receiver<SessionEvent>> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(sender) = state.sessions.get(&session_id) {
            return Some(sender.subscribe());
        }
        let event = state.terminal_events.get(&session_id).cloned()?;
        let (sender, receiver) = broadcast::channel(1);
        let _ = sender.send(event);
        Some(receiver)
    }

    pub fn close_with_final(&self, session_id: Uuid, event: SessionEvent) {
        if event.session_id() != session_id {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(sender) = state.sessions.remove(&session_id) {
            let _ = sender.send(event.clone());
        }
        if state.terminal_events.insert(session_id, event).is_none() {
            state.terminal_order.push_back(session_id);
        }
        while state.terminal_order.len() > TERMINAL_REPLAY_CAPACITY {
            if let Some(expired) = state.terminal_order.pop_front() {
                state.terminal_events.remove(&expired);
            }
        }
    }

    pub fn publish(&self, session_id: Uuid, event: SessionEvent) -> Result<usize, EventBusError> {
        if event.session_id() != session_id {
            return Err(EventBusError::SessionMismatch);
        }
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(sender) = state.sessions.get(&session_id) else {
            return Err(EventBusError::NoSubscribers);
        };
        sender.send(event).map_err(|_| EventBusError::NoSubscribers)
    }

    pub fn close(&self, session_id: Uuid) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.terminal_events.remove(&session_id);
        state.terminal_order.retain(|id| *id != session_id);
        state.sessions.remove(&session_id).is_some()
    }

    #[cfg(test)]
    fn terminal_replay_len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .terminal_events
            .len()
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
    async fn audio_state_is_published_as_a_transient_typed_event() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        bus.open_session(session_id);
        let mut receiver = bus.subscribe(session_id);

        bus.publish(
            session_id,
            SessionEvent::audio_state(
                session_id,
                super::super::AudioSessionState::Paused {
                    reason: "device lost".into(),
                },
            ),
        )
        .unwrap();

        assert!(
            matches!(receiver.recv().await.unwrap(), SessionEvent::AudioState { state: super::super::AudioSessionState::Paused { reason }, .. } if reason == "device lost")
        );
        assert_eq!(bus.terminal_replay_len(), 0);
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

    #[tokio::test]
    async fn close_releases_session_channel_and_closes_receivers() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        let mut receiver = bus.subscribe(session_id);

        assert!(bus.close(session_id));
        assert!(matches!(
            receiver.recv().await,
            Err(broadcast::error::RecvError::Closed)
        ));
        assert!(!bus.close(session_id));
    }

    #[tokio::test]
    async fn publish_rejects_mismatched_embedded_session_id() {
        let bus = EventBus::new();
        let routing_id = Uuid::new_v4();
        let event = SessionEvent::status(Uuid::new_v4(), "running");

        assert_eq!(
            bus.publish(routing_id, event),
            Err(EventBusError::SessionMismatch)
        );
    }

    #[test]
    fn publishing_after_close_does_not_recreate_a_live_channel() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        let _receiver = bus.subscribe(session_id);
        assert!(bus.close(session_id));
        assert_eq!(
            bus.publish(session_id, SessionEvent::status(session_id, "running")),
            Err(EventBusError::NoSubscribers)
        );
        assert!(bus.try_subscribe(session_id).is_none());
    }

    #[test]
    fn terminal_replay_is_bounded() {
        let bus = EventBus::new();
        for _ in 0..(TERMINAL_REPLAY_CAPACITY + 10) {
            let session_id = Uuid::new_v4();
            bus.close_with_final(session_id, SessionEvent::status(session_id, "completed"));
        }
        assert_eq!(bus.terminal_replay_len(), TERMINAL_REPLAY_CAPACITY);
    }

    #[tokio::test]
    async fn replay_subscriber_receives_queued_terminal_event() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        let mut receiver = bus.subscribe(session_id);
        let queued = SessionEvent::status(session_id, "starting");
        bus.publish(session_id, queued.clone()).unwrap();
        bus.close_with_final(session_id, SessionEvent::status(session_id, "completed"));
        assert_eq!(receiver.recv().await.unwrap(), queued);
        assert!(
            matches!(receiver.recv().await.unwrap(), SessionEvent::Status { status, .. } if status == "completed")
        );
    }

    #[tokio::test]
    async fn subscribe_after_terminal_replays_without_masking_terminal_state() {
        let bus = EventBus::new();
        let session_id = Uuid::new_v4();
        let final_event = SessionEvent::status(session_id, "failed");
        bus.close_with_final(session_id, final_event.clone());

        let mut receiver = bus.subscribe(session_id);
        assert_eq!(receiver.recv().await.unwrap(), final_event);
        assert!(bus.try_subscribe(session_id).is_none());

        let mut replay = bus.subscribe_or_replay(session_id).unwrap();
        assert_eq!(replay.recv().await.unwrap(), final_event);
    }
}
