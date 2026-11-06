//! Synthetic native owner using the production converter, ring and callback.
use super::*;
use crate::{
    audio::{platform::native_io, queue::BoundedAudioQueue},
    domain::transcript::PlaybackBoundary,
};
use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
pub(crate) struct NativePlaybackFixture {
    probes: Mutex<HashMap<String, Vec<Arc<NativeProbe>>>>,
}
pub(crate) struct NativeProbe {
    pub capture: BoundedAudioQueue,
    buffer: Arc<PlaybackBuffer>,
    state: Arc<AtomicU8>,
}
impl NativeProbe {
    pub fn statistics(&self) -> crate::domain::transcript::PlaybackStatistics {
        self.buffer.shared.statistics().playback
    }
    pub fn pending_samples(&self) -> usize {
        self.buffer.ring.len()
    }
    pub fn consume(&self) -> Vec<i16> {
        let mut output = vec![0; self.pending_samples()];
        let (events, _) = mpsc::channel(1);
        render(&mut output, 1, &self.buffer, &self.state, &events);
        output
    }
}
impl NativePlaybackFixture {
    pub fn probes(&self, input: &str) -> Vec<Arc<NativeProbe>> {
        self.probes
            .lock()
            .unwrap()
            .get(input)
            .cloned()
            .unwrap_or_default()
    }
}
impl native_io::NativeOwnerFactory for NativePlaybackFixture {
    fn open_on_owner_thread(
        &self,
        request: &AudioOpenRequest,
        capture: BoundedAudioQueue,
        source: mpsc::Receiver<AudioFrame>,
        shared: AudioShared,
    ) -> AppResult<Box<dyn native_io::NativeOwner>> {
        let mut probes = self.probes.lock().unwrap();
        let previous = probes.entry(request.input_id.clone()).or_default();
        // Only the first forward ring is tiny; the real ingress remains 50 frames.
        let capacity = if request.input_id == "a-input" && previous.is_empty() {
            64
        } else {
            48000 * 5
        };
        shared.counters.native_rate.store(48000, Ordering::Release);
        let buffer = Arc::new(PlaybackBuffer::new(capacity, shared.clone()));
        let state = Arc::new(AtomicU8::new(STATE_RUNNING));
        let worker_buffer = buffer.clone();
        let worker_state = state.clone();
        let worker = thread::spawn(move || {
            process_playback(source, worker_buffer, worker_state, 48000, true)
        });
        previous.push(Arc::new(NativeProbe {
            capture,
            buffer: buffer.clone(),
            state: state.clone(),
        }));
        let (_, events) = mpsc::channel(1);
        Ok(Box::new(SyntheticOwner {
            playback: Some(PlaybackHandle {
                stream: None,
                worker: Some(worker),
                state,
                buffer,
                shared: shared.clone(),
                events,
            }),
            shared,
            _not_send: std::rc::Rc::new(()),
        }))
    }
}
struct SyntheticOwner {
    playback: Option<PlaybackHandle>,
    shared: AudioShared,
    _not_send: std::rc::Rc<()>,
}
impl native_io::NativeOwner for SyntheticOwner {
    fn stop_capture(&mut self) -> AppResult<()> {
        self.shared
            .counters
            .capture_finalized
            .store(true, Ordering::Release);
        Ok(())
    }
    fn discard_pending(&mut self) -> AppResult<()> {
        if let Some(playback) = &self.playback {
            playback.discard();
        }
        Ok(())
    }
    fn seal_playback(&mut self) -> AppResult<()> {
        self.playback.as_ref().unwrap().seal();
        Ok(())
    }
    fn playback_drained(&mut self) -> AppResult<Option<PlaybackReport>> {
        if !self.playback.as_ref().unwrap().drained() {
            return Ok(None);
        }
        self.shared
            .counters
            .playback_finalized
            .store(true, Ordering::Release);
        self.shared
            .counters
            .tail_known
            .store(true, Ordering::Release);
        Ok(Some(PlaybackReport {
            boundary: PlaybackBoundary::CallbackConsumed,
            statistics: self.shared.statistics().playback,
        }))
    }
    fn close(&mut self) -> AppResult<()> {
        self.playback.take();
        self.shared
            .counters
            .capture_finalized
            .store(true, Ordering::Release);
        Ok(())
    }
}
