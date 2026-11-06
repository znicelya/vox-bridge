//! Bounded frontend event delivery, independent of session/audio ownership.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use async_trait::async_trait;
use tokio::{
    sync::{Mutex, broadcast, mpsc, oneshot, watch},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    events::SessionEvent,
};

#[async_trait]
pub trait FrontendEmitter: Send + Sync {
    async fn emit(&self, event_name: &str, event: &SessionEvent) -> AppResult<()>;
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SubscriptionTicket {
    pub id: Uuid,
    pub event_name: String,
}

#[derive(Clone)]
pub(crate) struct SubscriptionRegistry {
    sender: mpsc::Sender<Mutation>,
    receiver: Arc<StdMutex<Option<mpsc::Receiver<Mutation>>>>,
    entries: Entries,
}

const SUBSCRIPTION_CAPACITY: usize = 16;
const MUTATION_CAPACITY: usize = 16;

type SubscriptionKey = (Uuid, String);
type Entries = Arc<Mutex<HashMap<SubscriptionKey, Entry>>>;

struct Entry {
    id: Uuid,
    cancel: watch::Sender<bool>,
    task: JoinHandle<AppResult<()>>,
}

/// A sent response can be dropped before its caller ever polls it. Keep that
/// response responsible for cancellation until subscribe synchronously claims it.
struct TicketDelivery {
    ticket: Option<SubscriptionTicket>,
    cancel: watch::Sender<bool>,
}

impl TicketDelivery {
    fn claim(mut self) -> SubscriptionTicket {
        self.ticket.take().expect("ticket delivery is claimed once")
    }
}

impl Drop for TicketDelivery {
    fn drop(&mut self) {
        if self.ticket.is_some() {
            self.cancel.send_replace(true);
        }
    }
}

enum Mutation {
    Subscribe {
        key: SubscriptionKey,
        receiver: broadcast::Receiver<SessionEvent>,
        emitter: Arc<dyn FrontendEmitter>,
        reply: oneshot::Sender<AppResult<TicketDelivery>>,
    },
    Unsubscribe {
        id: Uuid,
        reply: oneshot::Sender<AppResult<()>>,
    },
    CloseSession {
        session_id: Uuid,
        reply: oneshot::Sender<AppResult<()>>,
    },
    CloseAll {
        reply: oneshot::Sender<AppResult<()>>,
    },
}

impl SubscriptionRegistry {
    /// Local construction does not spawn or require an active Tokio runtime.
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel(MUTATION_CAPACITY);
        Self {
            sender,
            receiver: Arc::new(StdMutex::new(Some(receiver))),
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub(crate) async fn subscribe(
        &self,
        session_id: Uuid,
        subscriber_id: String,
        receiver: broadcast::Receiver<SessionEvent>,
        emitter: Arc<dyn FrontendEmitter>,
    ) -> AppResult<SubscriptionTicket> {
        if subscriber_id.is_empty() || subscriber_id.chars().take(65).count() > 64 {
            return Err(AppError::InvalidRequest(
                "subscriber token must contain 1 to 64 characters".into(),
            ));
        }
        let (reply, response) = oneshot::channel();
        self.admit(Mutation::Subscribe {
            key: (session_id, subscriber_id),
            receiver,
            emitter,
            reply,
        })?;
        // No await between receiving and claiming: a discarded response instead
        // cancels its own generation through TicketDelivery::drop.
        response
            .await
            .map_err(|_| owner_closed())?
            .map(TicketDelivery::claim)
    }
    pub(crate) async fn unsubscribe(&self, id: Uuid) -> AppResult<()> {
        let (reply, response) = oneshot::channel();
        self.admit(Mutation::Unsubscribe { id, reply })?;
        response.await.map_err(|_| owner_closed())?
    }
    pub(crate) async fn close_session(&self, session_id: Uuid) -> AppResult<()> {
        let (reply, response) = oneshot::channel();
        self.admit(Mutation::CloseSession { session_id, reply })?;
        response.await.map_err(|_| owner_closed())?
    }
    /// FIFO barrier: prior accepted mutations and their forwarders are drained.
    /// The same owner remains available for a later window lifecycle.
    pub(crate) async fn close_all(&self) -> AppResult<()> {
        let (reply, response) = oneshot::channel();
        self.admit(Mutation::CloseAll { reply })?;
        response.await.map_err(|_| owner_closed())?
    }

    fn admit(&self, mutation: Mutation) -> AppResult<()> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| owner_closed())?;
        // Serialize initial admission/startup only; no async work under this lock.
        let mut receiver = self
            .receiver
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.sender
            .try_send(mutation)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    AppError::InvalidRequest("subscription mutation queue is full".into())
                }
                mpsc::error::TrySendError::Closed(_) => owner_closed(),
            })?;
        if let Some(receiver) = receiver.take() {
            // Never capture a sender/registry here: dropping the last caller
            // closes this receiver and the owner then drains its forwarders.
            runtime.spawn(run_mutations(receiver, self.entries.clone()));
        }
        Ok(())
    }
}

fn owner_closed() -> AppError {
    AppError::Internal("subscription mutation owner unavailable".into())
}

/// One owned worker, at most 16 waiting mutations and one executing mutation.
/// A replacement occupies its predecessor's slot until cancellation/join ends.
async fn run_mutations(mut receiver: mpsc::Receiver<Mutation>, entries: Entries) {
    while let Some(mutation) = receiver.recv().await {
        match mutation {
            Mutation::Subscribe {
                key,
                receiver,
                emitter,
                reply,
            } => {
                let previous = entries.lock().await.remove(&key);
                cancel_and_join(previous.into_iter().collect()).await;
                if reply.is_closed() {
                    continue;
                }
                let mut map = entries.lock().await;
                if map.len() >= SUBSCRIPTION_CAPACITY {
                    let _ = reply.send(Err(AppError::InvalidRequest(
                        "subscription capacity reached".into(),
                    )));
                    continue;
                }
                let id = Uuid::new_v4();
                let ticket = SubscriptionTicket {
                    id,
                    event_name: format!("voxbridge://subscription/{id}"),
                };
                let event_name = ticket.event_name.clone();
                let (cancel, cancelled) = watch::channel(false);
                let weak_entries = Arc::downgrade(&entries);
                let forwarder_key = key.clone();
                let session_id = key.0;
                // Hold the map lock through insertion, so even an immediately
                // terminal receiver cannot finish before its entry exists.
                let task = tokio::spawn(async move {
                    let result = forward_session_events(
                        session_id, event_name, receiver, cancelled, emitter,
                    )
                    .await;
                    if let Some(entries) = weak_entries.upgrade() {
                        drop(take_generation(&entries, &forwarder_key, id).await);
                    }
                    result
                });
                let delivery = TicketDelivery {
                    ticket: Some(ticket),
                    cancel: cancel.clone(),
                };
                map.insert(key.clone(), Entry { id, cancel, task });
                drop(map);
                if reply.send(Ok(delivery)).is_err() {
                    // Cancellation may happen after the is_closed check. The
                    // owner still cancels/joins the unclaimed generation.
                    let unclaimed = take_generation(&entries, &key, id).await;
                    cancel_and_join(unclaimed.into_iter().collect()).await;
                }
            }
            Mutation::Unsubscribe { id, reply } => {
                let removed = {
                    let mut map = entries.lock().await;
                    let key = map
                        .iter()
                        .find(|(_, entry)| entry.id == id)
                        .map(|(key, _)| key.clone());
                    key.and_then(|key| map.remove(&key))
                };
                cancel_and_join(removed.into_iter().collect()).await;
                let _ = reply.send(Ok(()));
            }
            Mutation::CloseSession { session_id, reply } => {
                let removed = take_matching(&entries, Some(session_id)).await;
                cancel_and_join(removed).await;
                let _ = reply.send(Ok(()));
            }
            Mutation::CloseAll { reply } => {
                let removed = take_matching(&entries, None).await;
                cancel_and_join(removed).await;
                let _ = reply.send(Ok(()));
            }
        }
    }
    // Registry destruction also owns shutdown; no self-retaining sender cycle.
    cancel_and_join(take_matching(&entries, None).await).await;
}

async fn take_generation(entries: &Entries, key: &SubscriptionKey, id: Uuid) -> Option<Entry> {
    let mut map = entries.lock().await;
    if map.get(key).is_some_and(|entry| entry.id == id) {
        map.remove(key)
    } else {
        None
    }
}

async fn take_matching(entries: &Entries, session_id: Option<Uuid>) -> Vec<Entry> {
    let mut map = entries.lock().await;
    let keys: Vec<_> = map
        .keys()
        .filter(|key| session_id.is_none_or(|id| key.0 == id))
        .cloned()
        .collect();
    keys.into_iter()
        .filter_map(|key| map.remove(&key))
        .collect()
}

async fn cancel_and_join(entries: Vec<Entry>) {
    // Cancel the whole bounded set before joining, including any blocked emits.
    for entry in &entries {
        entry.cancel.send_replace(true);
    }
    for entry in entries {
        let _ = entry.task.await;
    }
}

pub async fn forward_session_events(
    session_id: Uuid,
    event_name: String,
    mut receiver: broadcast::Receiver<SessionEvent>,
    mut cancel: watch::Receiver<bool>,
    emitter: Arc<dyn FrontendEmitter>,
) -> AppResult<()> {
    loop {
        let event = tokio::select! {
            biased;
            _ = cancelled(&mut cancel) => return Ok(()),
            received = receiver.recv() => match received {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(missed_events)) => {
                    SessionEvent::ResyncNeeded { session_id, missed_events }
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            }
        };
        let terminal = matches!(&event, SessionEvent::Status { status, .. } if matches!(status.as_str(), "completed" | "failed"));
        tokio::select! {
            biased;
            _ = cancelled(&mut cancel) => return Ok(()),
            result = tokio::time::timeout(Duration::from_secs(1), emitter.emit(&event_name, &event)) => {
                result.map_err(|_| AppError::Internal("frontend emit timed out".into()))??;
            }
        }
        if terminal {
            return Ok(());
        }
    }
}

async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow_and_update() {
            return;
        }
        if cancel.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "subscriptions_tests.rs"]
mod tests;
