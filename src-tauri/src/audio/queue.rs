use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::Mutex;

use super::AudioFrame;

pub const DEFAULT_AUDIO_QUEUE_CAPACITY: usize = 50;

/// A bounded FIFO that favors recent audio when producers outrun consumers.
#[derive(Clone)]
pub struct BoundedAudioQueue {
    frames: Arc<Mutex<VecDeque<AudioFrame>>>,
    capacity_frames: usize,
    dropped_frames: Arc<AtomicU64>,
}

impl BoundedAudioQueue {
    pub fn new(capacity_frames: usize) -> Self {
        Self {
            frames: Arc::new(Mutex::new(VecDeque::with_capacity(capacity_frames.max(1)))),
            capacity_frames: capacity_frames.max(1),
            dropped_frames: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn capacity_frames(&self) -> usize {
        self.capacity_frames
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }

    pub async fn push(&self, frame: AudioFrame) -> Result<(), AudioFrame> {
        let mut frames = self.frames.lock().await;
        if frames.len() == self.capacity_frames {
            frames.pop_front();
            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
        frames.push_back(frame);
        Ok(())
    }

    pub async fn pop(&self) -> Option<AudioFrame> {
        self.frames.lock().await.pop_front()
    }
}

impl Default for BoundedAudioQueue {
    fn default() -> Self {
        Self::new(DEFAULT_AUDIO_QUEUE_CAPACITY)
    }
}
