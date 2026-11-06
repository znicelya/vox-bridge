use super::platform::{AudioDirection, AudioEndpoint};
use crate::{
    audio::{AudioFrame, PcmSpec, Resampler, runtime::*},
    error::{AppError, AppResult},
};
use cpal::{
    FromSample, Sample, SampleFormat as CpalSampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_queue::ArrayQueue;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::mpsc;
const EVENT_CAPACITY: usize = 16;
const STATE_RUNNING: u8 = 0;
const STATE_UNDERFLOW: u8 = 1;
const STATE_DISCONNECTED: u8 = 2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Running,
    AudioUnderflow,
    DeviceDisconnected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackEvent {
    AudioUnderflow,
    DeviceDisconnected,
}
#[derive(Debug, PartialEq, Eq)]
pub struct PlaybackStats {
    pub dropped_samples: u64,
}

/// Shared application ring; it owns no native stream and is safe to inspect after timeout.
pub(crate) struct PlaybackBuffer {
    ring: ArrayQueue<i16>,
    pub(crate) done: AtomicBool,
    sealed: AtomicBool,
    shared: AudioShared,
}
impl PlaybackBuffer {
    pub(crate) fn new(capacity: usize, shared: AudioShared) -> Self {
        Self {
            ring: ArrayQueue::new(capacity.max(1)),
            done: AtomicBool::new(false),
            sealed: AtomicBool::new(false),
            shared,
        }
    }
    pub(crate) fn drained(&self) -> bool {
        drain_complete(self.done.load(Ordering::Acquire), self.ring.is_empty())
    }
    fn push(&self, sample: i16) {
        if self
            .shared
            .gates
            .playback_suppressed
            .load(Ordering::Acquire)
        {
            add(&self.shared.counters.native_discarded, 1);
            return;
        }
        if self.ring.push(sample).is_ok() {
            return;
        }
        self.replace_full(sample);
    }
    fn replace_full(&self, sample: i16) {
        let overwritten = u64::from(self.ring.pop().is_some());
        let rejected = u64::from(self.ring.push(sample).is_err());
        add(
            &self.shared.counters.native_discarded,
            overwritten + rejected,
        );
    }
    fn pop(&self) -> Option<i16> {
        let sample = self.ring.pop();
        if sample.is_some() {
            add(&self.shared.counters.native_consumed, 1);
        }
        sample
    }
    pub(crate) fn discard(&self) {
        while self.ring.pop().is_some() {
            add(&self.shared.counters.native_discarded, 1);
        }
    }
}
pub(crate) fn drain_complete(worker_done: bool, ring_empty: bool) -> bool {
    worker_done && ring_empty
}

pub struct PlaybackHandle {
    stream: Option<Stream>,
    worker: Option<JoinHandle<()>>,
    state: Arc<AtomicU8>,
    buffer: Arc<PlaybackBuffer>,
    shared: AudioShared,
    events: mpsc::Receiver<PlaybackEvent>,
}
impl PlaybackHandle {
    pub fn state(&self) -> PlaybackState {
        decode_state(self.state.load(Ordering::Acquire))
    }
    pub fn is_disconnected(&self) -> bool {
        self.state() == PlaybackState::DeviceDisconnected
    }
    pub fn dropped_samples(&self) -> u64 {
        self.shared
            .counters
            .native_discarded
            .load(Ordering::Acquire)
    }
    pub fn try_recv_event(&mut self) -> Result<PlaybackEvent, mpsc::error::TryRecvError> {
        self.events.try_recv()
    }
    pub fn stop(mut self) {
        self.close_inner();
    }
    pub fn stop_with_stats(mut self) -> PlaybackStats {
        self.close_inner();
        PlaybackStats {
            dropped_samples: self.dropped_samples(),
        }
    }
    pub(crate) fn seal(&self) {
        self.buffer.sealed.store(true, Ordering::Release);
    }
    pub(crate) fn discard(&self) {
        self.shared.suppress();
        self.buffer.discard();
    }
    pub(crate) fn drained(&self) -> bool {
        self.buffer.drained()
    }
    fn close_inner(&mut self) {
        self.shared
            .gates
            .playback_suppressed
            .store(true, Ordering::Release);
        self.shared
            .gates
            .abort_pending
            .store(true, Ordering::Release);
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.buffer.discard();
        self.shared
            .counters
            .tail_known
            .store(true, Ordering::Release);
        self.shared
            .counters
            .playback_finalized
            .store(true, Ordering::Release);
    }
}
impl Drop for PlaybackHandle {
    fn drop(&mut self) {
        self.close_inner();
    }
}
pub struct AudioPlayback;
impl AudioPlayback {
    pub fn start(
        endpoint: AudioEndpoint,
        source: mpsc::Receiver<AudioFrame>,
    ) -> AppResult<PlaybackHandle> {
        Self::start_inner(
            endpoint,
            source,
            AudioShared::new(super::queue::BoundedAudioQueue::default()),
            false,
        )
    }
    pub(crate) fn start_runtime(
        endpoint: AudioEndpoint,
        source: mpsc::Receiver<AudioFrame>,
        shared: AudioShared,
    ) -> AppResult<PlaybackHandle> {
        Self::start_inner(endpoint, source, shared, true)
    }
    fn start_inner(
        endpoint: AudioEndpoint,
        source: mpsc::Receiver<AudioFrame>,
        shared: AudioShared,
        strict: bool,
    ) -> AppResult<PlaybackHandle> {
        if endpoint.direction != AudioDirection::Output {
            return Err(audio_error("playback endpoint is not an output"));
        }
        let device = exact_output(&endpoint)?;
        let supported = device.default_output_config().map_err(device_error)?;
        let rate = supported.sample_rate().0;
        let channels = supported.channels();
        if !(8000..=192000).contains(&rate) || !(1..=32).contains(&channels) {
            return Err(audio_error("unsupported native playback configuration"));
        }
        shared.counters.native_rate.store(rate, Ordering::Release);
        let buffer = Arc::new(PlaybackBuffer::new(rate as usize * 5, shared.clone()));
        let state = Arc::new(AtomicU8::new(STATE_RUNNING));
        let (tx, events) = mpsc::channel(EVENT_CAPACITY);
        let config: StreamConfig = supported.clone().into();
        let stream = match supported.sample_format() {
            CpalSampleFormat::I16 => {
                build_stream::<i16>(&device, &config, buffer.clone(), state.clone(), tx)
            }
            CpalSampleFormat::F32 => {
                build_stream::<f32>(&device, &config, buffer.clone(), state.clone(), tx)
            }
            CpalSampleFormat::U16 => {
                build_stream::<u16>(&device, &config, buffer.clone(), state.clone(), tx)
            }
            _ => Err(audio_error("unsupported native playback sample type")),
        }?;
        let worker_buffer = buffer.clone();
        let worker_state = state.clone();
        let worker = thread::Builder::new()
            .name("vox-playback-convert".into())
            .spawn(move || process_playback(source, worker_buffer, worker_state, rate, strict))
            .map_err(|_| audio_error("playback worker could not start"))?;
        let handle = PlaybackHandle {
            stream: Some(stream),
            worker: Some(worker),
            state,
            buffer,
            shared,
            events,
        };
        handle
            .stream
            .as_ref()
            .unwrap()
            .play()
            .map_err(device_error)?;
        Ok(handle)
    }
}
fn exact_output(endpoint: &AudioEndpoint) -> AppResult<cpal::Device> {
    let mut found = None;
    for device in cpal::default_host()
        .output_devices()
        .map_err(device_error)?
    {
        if device
            .name()
            .is_ok_and(|name| name == endpoint.id || name == endpoint.display_name)
        {
            if found.is_some() {
                return Err(audio_error("ambiguous native output name"));
            }
            found = Some(device);
        }
    }
    found.ok_or_else(|| audio_error("selected native output is unavailable"))
}
fn build_stream<T: cpal::SizedSample + Sample + FromSample<i16>>(
    device: &cpal::Device,
    config: &StreamConfig,
    buffer: Arc<PlaybackBuffer>,
    state: Arc<AtomicU8>,
    events: mpsc::Sender<PlaybackEvent>,
) -> AppResult<Stream> {
    let channels = config.channels as usize;
    let error_state = state.clone();
    let error_events = events.clone();
    let error_shared = buffer.shared.clone();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| render(data, channels, &buffer, &state, &events),
            move |_| {
                error_shared.suppress();
                if error_state.swap(STATE_DISCONNECTED, Ordering::AcqRel) != STATE_DISCONNECTED {
                    let _ = error_events.try_send(PlaybackEvent::DeviceDisconnected);
                }
            },
            None,
        )
        .map_err(device_error)
}
fn render<T: Sample + FromSample<i16>>(
    data: &mut [T],
    channels: usize,
    buffer: &PlaybackBuffer,
    state: &AtomicU8,
    events: &mpsc::Sender<PlaybackEvent>,
) {
    if buffer
        .shared
        .gates
        .playback_suppressed
        .load(Ordering::Acquire)
        || state.load(Ordering::Acquire) == STATE_DISCONNECTED
    {
        data.fill(T::from_sample(0_i16));
        return;
    }
    let mut underflow = false;
    for frame in data.chunks_mut(channels.max(1)) {
        let value = if buffer
            .shared
            .gates
            .playback_suppressed
            .load(Ordering::Acquire)
        {
            0
        } else {
            buffer.pop().unwrap_or_else(|| {
                underflow = true;
                0
            })
        };
        frame.fill(T::from_sample(value));
    }
    if underflow {
        if state
            .compare_exchange(
                STATE_RUNNING,
                STATE_UNDERFLOW,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            let _ = events.try_send(PlaybackEvent::AudioUnderflow);
        }
    } else {
        let _ = state.compare_exchange(
            STATE_UNDERFLOW,
            STATE_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}
fn process_playback(
    mut input: mpsc::Receiver<AudioFrame>,
    buffer: Arc<PlaybackBuffer>,
    state: Arc<AtomicU8>,
    rate: u32,
    strict: bool,
) {
    let mut resampler: Option<Resampler> = None;
    let mut input_spec = None;
    loop {
        if buffer.sealed.load(Ordering::Acquire) {
            input.close();
        }
        if buffer.shared.gates.abort_pending.load(Ordering::Acquire)
            || state.load(Ordering::Acquire) == STATE_DISCONNECTED
        {
            input.close();
            while let Ok(frame) = input.try_recv() {
                if !strict || validate_playback(&frame).is_ok() {
                    add(
                        &buffer.shared.counters.accepted,
                        frame.samples().len() as u64,
                    );
                    add(
                        &buffer.shared.counters.pre_conversion_discarded,
                        frame.samples().len() as u64,
                    );
                }
            }
            break;
        }
        match input.try_recv() {
            Ok(frame) => {
                if (strict && validate_playback(&frame).is_err())
                    || input_spec.is_some_and(|spec| spec != frame.spec())
                {
                    state.store(STATE_DISCONNECTED, Ordering::Release);
                    buffer.shared.suppress();
                    continue;
                }
                add(
                    &buffer.shared.counters.accepted,
                    frame.samples().len() as u64,
                );
                if resampler.is_none() {
                    match Resampler::new(frame.spec(), PcmSpec::pcm16_mono(rate)) {
                        Ok(r) => {
                            resampler = Some(r);
                            input_spec = Some(frame.spec());
                        }
                        Err(_) => {
                            add(
                                &buffer.shared.counters.pre_conversion_discarded,
                                frame.samples().len() as u64,
                            );
                            state.store(STATE_DISCONNECTED, Ordering::Release);
                            buffer.shared.suppress();
                            continue;
                        }
                    }
                }
                for chunk in frame.samples().chunks(1920) {
                    if buffer.shared.gates.abort_pending.load(Ordering::Acquire) {
                        add(
                            &buffer.shared.counters.pre_conversion_discarded,
                            chunk.len() as u64,
                        );
                        continue;
                    }
                    match resampler.as_mut().unwrap().process(chunk) {
                        Ok(samples) => {
                            add(
                                &buffer.shared.counters.native_generated,
                                samples.len() as u64,
                            );
                            for sample in samples {
                                buffer.push(sample);
                            }
                        }
                        Err(_) => {
                            add(
                                &buffer.shared.counters.pre_conversion_discarded,
                                chunk.len() as u64,
                            );
                            state.store(STATE_DISCONNECTED, Ordering::Release);
                            buffer.shared.suppress();
                        }
                    }
                }
            }
            Err(mpsc::error::TryRecvError::Disconnected) => break,
            Err(mpsc::error::TryRecvError::Empty) => thread::sleep(Duration::from_millis(2)),
        }
    }
    if !buffer.shared.gates.abort_pending.load(Ordering::Acquire) {
        if let Some(mut resampler) = resampler {
            match resampler.flush() {
                Ok(samples) => {
                    add(
                        &buffer.shared.counters.native_generated,
                        samples.len() as u64,
                    );
                    for sample in samples {
                        buffer.push(sample);
                    }
                }
                Err(_) => {
                    state.store(STATE_DISCONNECTED, Ordering::Release);
                    buffer.shared.suppress();
                }
            }
        }
    } else {
        buffer.discard();
    }
    buffer.done.store(true, Ordering::Release);
}
fn decode_state(value: u8) -> PlaybackState {
    match value {
        STATE_UNDERFLOW => PlaybackState::AudioUnderflow,
        STATE_DISCONNECTED => PlaybackState::DeviceDisconnected,
        _ => PlaybackState::Running,
    }
}
fn device_error(_: impl std::fmt::Display) -> AppError {
    audio_error("native playback device operation failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audio::AudioSource;
    #[test]
    fn suppression_silences_real_callback_without_waiting_for_owner() {
        let shared = AudioShared::new(super::super::queue::BoundedAudioQueue::default());
        let buffer = PlaybackBuffer::new(4, shared.clone());
        buffer.push(17);
        let (tx, _) = mpsc::channel(2);
        let state = AtomicU8::new(STATE_RUNNING);
        shared.suppress();
        let mut out = [9i16; 4];
        render(&mut out, 2, &buffer, &state, &tx);
        assert_eq!(out, [0; 4]);
        assert_eq!(
            shared
                .statistics()
                .playback
                .native
                .map(|s| s.callback_consumed_mono_samples),
            None
        );
        assert_eq!(buffer.ring.len(), 1);
    }
    #[test]
    fn seal_waits_for_all_accepted_conversion_and_actual_ring_consumption() {
        let shared = AudioShared::new(super::super::queue::BoundedAudioQueue::default());
        let buffer = Arc::new(PlaybackBuffer::new(24000, shared.clone()));
        let (tx, rx) = mpsc::channel(2);
        tx.try_send(
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(24000),
                vec![9; 1920],
            )
            .unwrap(),
        )
        .unwrap();
        assert!(!buffer.drained());
        buffer.sealed.store(true, Ordering::Release);
        let b = buffer.clone();
        let worker =
            thread::spawn(move || process_playback(rx, b, Arc::new(AtomicU8::new(0)), 24000, true));
        worker.join().unwrap();
        assert!(!buffer.drained());
        assert_eq!(
            shared.statistics().playback.accepted_for_processing_samples,
            1920
        );
        assert!(tx.is_closed());
        let mut count = 0;
        while buffer.pop().is_some() {
            count += 1;
        }
        assert_eq!(count, 1920);
        assert!(buffer.drained());
    }
    #[test]
    fn actual_overwrite_and_discard_are_counted_without_callback_pops() {
        let shared = AudioShared::new(super::super::queue::BoundedAudioQueue::default());
        shared.counters.native_rate.store(48000, Ordering::Release);
        let buffer = PlaybackBuffer::new(2, shared.clone());
        for n in 1..=3 {
            buffer.push(n);
        }
        assert_eq!(buffer.pop(), Some(2));
        buffer.discard();
        let native = shared.statistics().playback.native.unwrap();
        assert_eq!(native.discarded_mono_samples, 2);
        assert_eq!(native.callback_consumed_mono_samples, 1);
    }
}

#[cfg(test)]
mod runtime_regressions {
    use super::*;
    use crate::{
        audio::{platform::native_io, queue::BoundedAudioQueue},
        domain::{audio::AudioSource, transcript::RuntimeFailureCode},
    };
    #[test]
    fn full_ring_consumed_by_callback_before_discard_does_not_invent_loss() {
        let shared = AudioShared::new(BoundedAudioQueue::default());
        shared.counters.native_rate.store(48000, Ordering::Release);
        let ring = PlaybackBuffer::new(1, shared.clone());
        ring.push(1);
        assert!(ring.ring.push(2).is_err());
        // The callback runs after the producer saw Full, before replace_full's pop.
        let (events, _) = mpsc::channel(2);
        let mut output = [0i16];
        render(
            &mut output,
            1,
            &ring,
            &AtomicU8::new(STATE_RUNNING),
            &events,
        );
        assert_eq!(output, [1]);
        ring.replace_full(2);
        assert_eq!(ring.pop(), Some(2));
        assert_eq!(
            shared
                .statistics()
                .playback
                .native
                .unwrap()
                .discarded_mono_samples,
            0
        );
    }
    #[test]
    fn concurrent_overwrite_preserves_order_and_accounts_actual_pops() {
        let shared = AudioShared::new(BoundedAudioQueue::default());
        shared.counters.native_rate.store(48000, Ordering::Release);
        let ring = Arc::new(PlaybackBuffer::new(64, shared.clone()));
        let producer_ring = ring.clone();
        let producer = thread::spawn(move || {
            for n in 0..20000i16 {
                producer_ring.push(n);
            }
        });
        let mut seen = Vec::new();
        while !producer.is_finished() {
            if let Some(sample) = ring.pop() {
                seen.push(sample);
            } else {
                thread::yield_now();
            }
        }
        producer.join().unwrap();
        while let Some(sample) = ring.pop() {
            seen.push(sample);
        }
        assert!(!seen.is_empty());
        assert!(seen.windows(2).all(|p| p[0] < p[1]));
        let counters = shared.statistics().playback.native.unwrap();
        assert_eq!(
            counters.callback_consumed_mono_samples + counters.discarded_mono_samples,
            20000
        );
    }
    #[test]
    fn real_callback_underflow_is_coalesced_and_disconnect_is_sticky() {
        let shared = AudioShared::new(BoundedAudioQueue::default());
        let ring = PlaybackBuffer::new(4, shared);
        let state = AtomicU8::new(STATE_RUNNING);
        let (events, mut received) = mpsc::channel(4);
        for _ in 0..2 {
            render(&mut [0i16], 1, &ring, &state, &events);
        }
        assert_eq!(received.try_recv().unwrap(), PlaybackEvent::AudioUnderflow);
        assert!(received.try_recv().is_err());
        ring.push(17);
        let mut output = [0i16];
        render(&mut output, 1, &ring, &state, &events);
        assert_eq!(output, [17]);
        assert_eq!(
            decode_state(state.load(Ordering::Acquire)),
            PlaybackState::Running
        );
        state.store(STATE_DISCONNECTED, Ordering::Release);
        ring.push(23);
        render(&mut output, 1, &ring, &state, &events);
        assert_eq!(output, [0]);
        assert_eq!(
            decode_state(state.load(Ordering::Acquire)),
            PlaybackState::DeviceDisconnected
        );
    }
    #[test]
    fn stop_stats_include_terminal_worker_drop_and_pending_ingress() {
        let shared = AudioShared::new(BoundedAudioQueue::default());
        shared.counters.native_rate.store(24000, Ordering::Release);
        let ring = Arc::new(PlaybackBuffer::new(2, shared.clone()));
        let (tx, rx) = mpsc::channel(2);
        for sequence in 0..2 {
            tx.try_send(
                AudioFrame::new(
                    AudioSource::Microphone,
                    sequence,
                    PcmSpec::pcm16_mono(24000),
                    vec![3; 1280],
                )
                .unwrap(),
            )
            .unwrap();
        }
        let r = ring.clone();
        let s = shared.clone();
        let worker = thread::spawn(move || {
            while !s.gates.abort_pending.load(Ordering::Acquire) {
                thread::yield_now();
            }
            r.push(7); // An already running converter reaches publication after stop.
            process_playback(rx, r, Arc::new(AtomicU8::new(STATE_RUNNING)), 24000, true);
        });
        let (_, events) = mpsc::channel(1);
        let handle = PlaybackHandle {
            stream: None,
            worker: Some(worker),
            state: Arc::new(AtomicU8::new(STATE_RUNNING)),
            buffer: ring,
            shared: shared.clone(),
            events,
        };
        assert_eq!(handle.stop_with_stats().dropped_samples, 1);
        let stats = shared.statistics().playback;
        assert!(stats.finalized);
        assert!(tx.is_closed());
        assert_eq!(stats.accepted_for_processing_samples, 2560);
        assert_eq!(stats.discarded_before_conversion_samples, 2560);
    }
    struct BlockingFactory {
        release: Arc<AtomicBool>,
        buffer: Arc<std::sync::Mutex<Option<Arc<PlaybackBuffer>>>>,
    }
    struct BlockingOwner {
        release: Arc<AtomicBool>,
        _not_send: std::rc::Rc<()>,
    }
    impl native_io::NativeOwnerFactory for BlockingFactory {
        fn open_on_owner_thread(
            &self,
            _: &AudioOpenRequest,
            _: BoundedAudioQueue,
            _: mpsc::Receiver<AudioFrame>,
            shared: AudioShared,
        ) -> AppResult<Box<dyn native_io::NativeOwner>> {
            shared.counters.native_rate.store(48000, Ordering::Release);
            let ring = Arc::new(PlaybackBuffer::new(4, shared));
            ring.push(99);
            *self.buffer.lock().unwrap() = Some(ring);
            Ok(Box::new(BlockingOwner {
                release: self.release.clone(),
                _not_send: std::rc::Rc::new(()),
            }))
        }
    }
    impl native_io::NativeOwner for BlockingOwner {
        fn stop_capture(&mut self) -> AppResult<()> {
            Ok(())
        }
        fn discard_pending(&mut self) -> AppResult<()> {
            Ok(())
        }
        fn seal_playback(&mut self) -> AppResult<()> {
            Ok(())
        }
        fn playback_drained(&mut self) -> AppResult<Option<PlaybackReport>> {
            Ok(None)
        }
        fn close(&mut self) -> AppResult<()> {
            while !self.release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        }
    }
    #[tokio::test]
    async fn blocked_owner_cannot_block_statistics_or_callback_suppression() {
        let release = Arc::new(AtomicBool::new(false));
        let buffer = Arc::new(std::sync::Mutex::new(None));
        let ports = native_io::open_owned(
            AudioOpenRequest {
                inventory_generation: uuid::Uuid::nil(),
                input_id: "in".into(),
                output_id: "out".into(),
                source: AudioSource::Microphone,
            },
            Arc::new(BlockingFactory {
                release: release.clone(),
                buffer: buffer.clone(),
            }),
            None,
            native_io::Deadlines {
                open: Duration::from_secs(1),
                close: Duration::from_millis(30),
                drain: Duration::from_millis(30),
            },
        )
        .await
        .unwrap();
        let result = ports.control.close().await;
        ports.control.suppress_now();
        let stats = ports.control.statistics();
        let ring = buffer.lock().unwrap().clone().unwrap();
        let (events, _) = mpsc::channel(1);
        let mut out = [9i16; 4];
        render(&mut out, 2, &ring, &AtomicU8::new(STATE_RUNNING), &events);
        release.store(true, Ordering::Release);
        assert!(matches!(
            result,
            Err(crate::error::AppError::Audio {
                code: RuntimeFailureCode::CleanupTimeout,
                ..
            })
        ));
        assert_eq!(out, [0; 4]);
        assert!(!stats.playback.finalized);
        assert_eq!(
            stats
                .playback
                .native
                .unwrap()
                .callback_consumed_mono_samples,
            0
        );
        assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
    }
}
