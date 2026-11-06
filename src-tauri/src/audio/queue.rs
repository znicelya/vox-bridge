use super::AudioFrame;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::Notify;

pub const DEFAULT_AUDIO_QUEUE_CAPACITY: usize = 50;

struct QueueState {
    frames: VecDeque<AudioFrame>,
    closed: bool,
}
struct SharedQueue {
    state: Mutex<QueueState>,
    available: Notify,
    dropped_frames: AtomicU64,
}

/// A bounded FIFO for conversion workers and async consumers, not audio callbacks.
#[derive(Clone)]
pub struct BoundedAudioQueue {
    shared: Arc<SharedQueue>,
    capacity_frames: usize,
}

impl BoundedAudioQueue {
    pub fn new(capacity_frames: usize) -> Self {
        let capacity_frames = capacity_frames.max(1);
        Self {
            shared: Arc::new(SharedQueue {
                state: Mutex::new(QueueState {
                    frames: VecDeque::with_capacity(capacity_frames),
                    closed: false,
                }),
                available: Notify::new(),
                dropped_frames: AtomicU64::new(0),
            }),
            capacity_frames,
        }
    }
    pub fn capacity_frames(&self) -> usize {
        self.capacity_frames
    }
    pub fn dropped_frames(&self) -> u64 {
        self.shared.dropped_frames.load(Ordering::Relaxed)
    }

    pub fn push_sync(&self, frame: AudioFrame) -> Result<(), AudioFrame> {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.closed {
            return Err(frame);
        }
        if state.frames.len() == self.capacity_frames {
            state.frames.pop_front();
            let _ = self.shared.dropped_frames.try_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |count| Some(count.saturating_add(1)),
            );
        }
        state.frames.push_back(frame);
        drop(state);
        self.shared.available.notify_one();
        Ok(())
    }
    pub async fn push(&self, frame: AudioFrame) -> Result<(), AudioFrame> {
        self.push_sync(frame)
    }
    pub async fn pop(&self) -> Option<AudioFrame> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .frames
            .pop_front()
    }
    pub async fn recv(&self) -> Option<AudioFrame> {
        loop {
            let available = self.shared.available.notified();
            tokio::pin!(available);
            // Register before looking at the queue, including before close can
            // notify all waiters. A permit alone cannot wake several receivers.
            available.as_mut().enable();
            {
                let mut state = self
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if let Some(frame) = state.frames.pop_front() {
                    return Some(frame);
                }
                if state.closed {
                    return None;
                }
            }
            available.await;
        }
    }
    /// Seals ingress while keeping accepted frames available to drain.
    pub fn close(&self) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed = true;
        self.shared.available.notify_waiters();
    }
    /// Discards only queued frames; it neither reopens ingress nor counts overflow.
    pub fn clear(&self) -> usize {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let count = state.frames.len();
        state.frames.clear();
        count
    }
}
impl Default for BoundedAudioQueue {
    fn default() -> Self {
        Self::new(DEFAULT_AUDIO_QUEUE_CAPACITY)
    }
}
