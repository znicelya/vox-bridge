use super::platform::{AudioDirection, AudioEndpoint};
use crate::{
    audio::{AudioFrame, PcmSpec, Resampler, queue::BoundedAudioQueue, runtime::*},
    domain::audio::AudioSource,
    error::{AppError, AppResult},
};
use cpal::{
    Device, SampleFormat as CpalSampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc as sync_mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};
use tokio::sync::mpsc;
const CAPTURE_INGRESS_BLOCKS: usize = 8;
const CAPTURE_BUFFER_SAMPLES: usize = 8192;
enum RawSamples {
    I16(Vec<i16>),
    F32(Vec<f32>),
    U16(Vec<u16>),
}
struct RawBlock {
    samples: RawSamples,
    len: usize,
    generation: u64,
    captured_at: SystemTime,
}
trait CaptureSample: SizedSample + Copy {
    fn copy(data: &[Self], buffer: &mut RawSamples) -> Option<usize>;
}
macro_rules! capture_sample {
    ($ty:ty, $variant:ident) => {
        impl CaptureSample for $ty {
            fn copy(data: &[Self], buffer: &mut RawSamples) -> Option<usize> {
                if let RawSamples::$variant(samples) = buffer {
                    if data.len() <= samples.len() {
                        samples[..data.len()].copy_from_slice(data);
                        return Some(data.len());
                    }
                }
                None
            }
        }
    };
}
capture_sample!(i16, I16);
capture_sample!(f32, F32);
capture_sample!(u16, U16);
#[derive(Clone)]
enum CaptureSink {
    Legacy(mpsc::Sender<AudioFrame>),
    Runtime(BoundedAudioQueue),
}
impl CaptureSink {
    fn send(&self, frame: AudioFrame, dropped: &AtomicU64) -> bool {
        match self {
            Self::Runtime(queue) => queue.push_sync(frame).is_ok(),
            Self::Legacy(tx) => match tx.try_send(frame) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    add(dropped, 1);
                    true
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            },
        }
    }
}
pub struct CaptureHandle {
    stream: Option<Stream>,
    worker: Option<JoinHandle<()>>,
    shared: AudioShared,
    dropped_frames: Arc<AtomicU64>,
    disconnected: Arc<AtomicBool>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CaptureStats {
    pub dropped_chunks: u64,
    pub dropped_frames: u64,
}
impl CaptureStats {
    #[cfg(test)]
    fn after_stop(stop: impl FnOnce(), chunks: &AtomicU64, frames: &AtomicU64) -> Self {
        stop();
        Self {
            dropped_chunks: chunks.load(Ordering::Acquire),
            dropped_frames: frames.load(Ordering::Acquire),
        }
    }
}
impl CaptureHandle {
    pub fn dropped_chunks(&self) -> u64 {
        self.shared.counters.native_blocks.load(Ordering::Acquire)
    }
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
            .load(Ordering::Acquire)
            .saturating_add(self.shared.statistics().capture.dropped_normalized_frames)
    }
    pub fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::Acquire)
    }
    pub fn stop(mut self) {
        self.close_inner();
    }
    pub fn stop_with_stats(mut self) -> CaptureStats {
        self.close_inner();
        CaptureStats {
            dropped_chunks: self.dropped_chunks(),
            dropped_frames: self.dropped_frames(),
        }
    }
    fn close_inner(&mut self) {
        self.shared.stop_capture();
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.shared
            .counters
            .capture_finalized
            .store(true, Ordering::Release);
    }
}
impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.close_inner();
    }
}
pub struct AudioCapture;
impl AudioCapture {
    pub fn start(
        endpoint: AudioEndpoint,
        sink: mpsc::Sender<AudioFrame>,
    ) -> AppResult<CaptureHandle> {
        Self::start_inner(
            endpoint,
            CaptureSink::Legacy(sink),
            AudioSource::Microphone,
            AudioShared::new(BoundedAudioQueue::default()),
        )
    }
    pub(crate) fn start_runtime(
        endpoint: AudioEndpoint,
        sink: BoundedAudioQueue,
        source: AudioSource,
        shared: AudioShared,
    ) -> AppResult<CaptureHandle> {
        Self::start_inner(endpoint, CaptureSink::Runtime(sink), source, shared)
    }
    fn start_inner(
        endpoint: AudioEndpoint,
        sink: CaptureSink,
        source: AudioSource,
        shared: AudioShared,
    ) -> AppResult<CaptureHandle> {
        if endpoint.direction != AudioDirection::Input {
            return Err(audio_error("capture endpoint is not an input"));
        }
        let device = exact_input(&endpoint)?;
        let supported = device.default_input_config().map_err(device_error)?;
        let rate = supported.sample_rate().0;
        let channels = supported.channels();
        if !(8000..=192000).contains(&rate) || !(1..=32).contains(&channels) {
            return Err(audio_error("unsupported native capture configuration"));
        }
        let config: StreamConfig = supported.clone().into();
        let (tx, rx) = sync_mpsc::sync_channel(CAPTURE_INGRESS_BLOCKS);
        let discontinuity_generation = Arc::new(AtomicU64::new(0));
        let dropped_frames = Arc::new(AtomicU64::new(0));
        let disconnected = Arc::new(AtomicBool::new(false));
        let (pool_tx, pool_rx) = make_pool(supported.sample_format(), channels);
        let worker_pool = pool_tx.clone();
        let worker_shared = shared.clone();
        let worker_dropped = dropped_frames.clone();
        let worker_lost = disconnected.clone();
        let worker_generation = discontinuity_generation.clone();
        let worker = thread::Builder::new()
            .name("vox-capture-convert".into())
            .spawn(move || {
                process_capture(
                    rx,
                    sink,
                    source,
                    rate,
                    channels,
                    worker_dropped,
                    worker_lost,
                    worker_pool,
                    worker_shared,
                    worker_generation,
                )
            })
            .map_err(|_| audio_error("capture worker could not start"))?;
        let mut handle = CaptureHandle {
            stream: None,
            worker: Some(worker),
            shared: shared.clone(),
            dropped_frames,
            disconnected: disconnected.clone(),
        };
        let stream = match supported.sample_format() {
            CpalSampleFormat::I16 => build_stream::<i16>(
                &device,
                &config,
                tx,
                pool_rx,
                pool_tx,
                shared,
                disconnected,
                discontinuity_generation,
            ),
            CpalSampleFormat::F32 => build_stream::<f32>(
                &device,
                &config,
                tx,
                pool_rx,
                pool_tx,
                shared,
                disconnected,
                discontinuity_generation,
            ),
            CpalSampleFormat::U16 => build_stream::<u16>(
                &device,
                &config,
                tx,
                pool_rx,
                pool_tx,
                shared,
                disconnected,
                discontinuity_generation,
            ),
            _ => Err(audio_error("unsupported native capture sample type")),
        }?;
        handle.stream = Some(stream);
        handle
            .stream
            .as_ref()
            .unwrap()
            .play()
            .map_err(device_error)?;
        Ok(handle)
    }
}
fn build_stream<T: CaptureSample>(
    device: &Device,
    config: &StreamConfig,
    tx: sync_mpsc::SyncSender<RawBlock>,
    pool: crossbeam_channel::Receiver<RawSamples>,
    returns: crossbeam_channel::Sender<RawSamples>,
    shared: AudioShared,
    lost: Arc<AtomicBool>,
    discontinuity_generation: Arc<AtomicU64>,
) -> AppResult<Stream> {
    let error_lost = lost.clone();
    let error_shared = shared.clone();
    let channels = usize::from(config.channels);
    let rate = config.sample_rate.0;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if lost.load(Ordering::Acquire)
                    || shared.gates.capture_suppressed.load(Ordering::Acquire)
                {
                    return;
                }
                let Some(mut buffer) = pool.try_recv().ok() else {
                    record_native_loss(&shared.counters.native_blocks, &discontinuity_generation);
                    return;
                };
                let Some(len) = T::copy(data, &mut buffer) else {
                    record_native_loss(&shared.counters.native_blocks, &discontinuity_generation);
                    let _ = returns.try_send(buffer);
                    return;
                };
                let duration = Duration::from_secs_f64((len / channels) as f64 / f64::from(rate));
                let captured_at = SystemTime::now()
                    .checked_sub(duration)
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                let block = RawBlock {
                    samples: buffer,
                    len,
                    generation: discontinuity_generation.load(Ordering::Acquire),
                    captured_at,
                };
                if let Err(error) = tx.try_send(block) {
                    record_native_loss(&shared.counters.native_blocks, &discontinuity_generation);
                    let block = match error {
                        sync_mpsc::TrySendError::Full(b)
                        | sync_mpsc::TrySendError::Disconnected(b) => b,
                    };
                    let _ = returns.try_send(block.samples);
                }
            },
            move |_| {
                error_lost.store(true, Ordering::Release);
                error_shared.suppress();
            },
            None,
        )
        .map_err(device_error)
}
fn exact_input(endpoint: &AudioEndpoint) -> AppResult<Device> {
    let mut found = None;
    for device in cpal::default_host().input_devices().map_err(device_error)? {
        if device
            .name()
            .is_ok_and(|n| n == endpoint.id || n == endpoint.display_name)
        {
            if found.is_some() {
                return Err(audio_error("ambiguous native input name"));
            }
            found = Some(device);
        }
    }
    found.ok_or_else(|| audio_error("selected native input is unavailable"))
}
struct CaptureConverter {
    resampler: Resampler,
    from: PcmSpec,
    source: AudioSource,
    generation: Option<u64>,
    base: Option<SystemTime>,
    emitted: u64,
    sequence: u64,
}
impl CaptureConverter {
    fn new(rate: u32, source: AudioSource) -> AppResult<Self> {
        let from = PcmSpec::pcm16_mono(rate);
        Ok(Self {
            resampler: Resampler::new(from, PcmSpec::pcm16_mono(16000))?,
            from,
            source,
            generation: None,
            base: None,
            emitted: 0,
            sequence: 0,
        })
    }
    fn block(
        &mut self,
        samples: &[i16],
        generation: u64,
        at: SystemTime,
    ) -> AppResult<Vec<AudioFrame>> {
        if self.generation != Some(generation) {
            if self.generation.is_some() {
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| audio_error("capture sequence exhausted"))?;
            }
            self.resampler = Resampler::new(self.from, PcmSpec::pcm16_mono(16000))?;
            self.base = Some(at);
            self.emitted = 0;
            self.generation = Some(generation);
        }
        let samples = self.resampler.process(samples)?;
        self.frames(samples)
    }
    fn flush(&mut self) -> AppResult<Vec<AudioFrame>> {
        let tail = self.resampler.flush()?;
        self.frames(tail)
    }
    fn frames(&mut self, samples: Vec<i16>) -> AppResult<Vec<AudioFrame>> {
        let mut out = Vec::with_capacity(samples.len().div_ceil(1280));
        for chunk in samples.chunks(1280) {
            let at = self
                .base
                .unwrap_or(SystemTime::now())
                .checked_add(sample_offset(self.emitted, 16000))
                .ok_or_else(|| audio_error("capture clock exhausted"))?;
            out.push(AudioFrame::at(
                self.source,
                self.sequence,
                at,
                PcmSpec::pcm16_mono(16000),
                chunk.to_vec(),
            )?);
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or_else(|| audio_error("capture sequence exhausted"))?;
            self.emitted = self
                .emitted
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| audio_error("capture clock exhausted"))?;
        }
        Ok(out)
    }
}
fn process_capture(
    rx: sync_mpsc::Receiver<RawBlock>,
    sink: CaptureSink,
    source: AudioSource,
    rate: u32,
    channels: u16,
    dropped: Arc<AtomicU64>,
    lost: Arc<AtomicBool>,
    pool: crossbeam_channel::Sender<RawSamples>,
    shared: AudioShared,
    discontinuity_generation: Arc<AtomicU64>,
) {
    let Ok(mut converter) = CaptureConverter::new(rate, source) else {
        lost.store(true, Ordering::Release);
        return;
    };
    loop {
        if shared.gates.abort_pending.load(Ordering::Acquire) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(2)) {
            Ok(block) => {
                if block.generation != discontinuity_generation.load(Ordering::Acquire) {
                    add(&shared.counters.native_blocks, 1);
                    let _ = pool.try_send(block.samples);
                    continue;
                }
                let mono = downmix_to_i16(&block.samples, block.len, channels);
                let _ = pool.try_send(block.samples);
                match converter.block(&mono, block.generation, block.captured_at) {
                    Ok(frames) => {
                        for frame in frames {
                            if block.generation != discontinuity_generation.load(Ordering::Acquire)
                            {
                                add(&shared.counters.native_blocks, 1);
                                break;
                            }
                            if shared.gates.abort_pending.load(Ordering::Acquire)
                                || !sink.send(frame, &dropped)
                            {
                                return;
                            }
                        }
                    }
                    Err(_) => {
                        lost.store(true, Ordering::Release);
                        shared.suppress();
                        break;
                    }
                }
            }
            Err(sync_mpsc::RecvTimeoutError::Disconnected) => break,
            Err(sync_mpsc::RecvTimeoutError::Timeout) => {
                if shared.gates.capture_suppressed.load(Ordering::Acquire) {
                    break;
                }
            }
        }
    }
    if !shared.gates.abort_pending.load(Ordering::Acquire)
        && converter.generation == Some(discontinuity_generation.load(Ordering::Acquire))
    {
        if let Ok(frames) = converter.flush() {
            for frame in frames {
                if !sink.send(frame, &dropped) {
                    break;
                }
            }
        }
    }
}
fn record_native_loss(counter: &AtomicU64, discontinuity_generation: &AtomicU64) {
    add(counter, 1);
    let _ =
        discontinuity_generation.try_update(Ordering::Release, Ordering::Relaxed, |generation| {
            Some(generation.saturating_add(1))
        });
}
fn make_pool(
    format: CpalSampleFormat,
    channels: u16,
) -> (
    crossbeam_channel::Sender<RawSamples>,
    crossbeam_channel::Receiver<RawSamples>,
) {
    let (tx, rx) = crossbeam_channel::bounded(CAPTURE_INGRESS_BLOCKS);
    for _ in 0..CAPTURE_INGRESS_BLOCKS {
        let n = CAPTURE_BUFFER_SAMPLES * usize::from(channels);
        let value = match format {
            CpalSampleFormat::F32 => RawSamples::F32(vec![0.0; n]),
            CpalSampleFormat::U16 => RawSamples::U16(vec![0; n]),
            _ => RawSamples::I16(vec![0; n]),
        };
        let _ = tx.try_send(value);
    }
    (tx, rx)
}
#[cfg(test)]
fn send_frame(
    sink: &mpsc::Sender<AudioFrame>,
    dropped: &AtomicU64,
    source: AudioSource,
    sequence: u64,
    samples: Vec<i16>,
) {
    if let Ok(frame) = AudioFrame::new(source, sequence, PcmSpec::pcm16_mono(16000), samples) {
        if sink.try_send(frame).is_err() {
            add(dropped, 1);
        }
    }
}
fn downmix_to_i16(samples: &RawSamples, len: usize, channels: u16) -> Vec<i16> {
    let channels = usize::from(channels.max(1));
    match samples {
        RawSamples::I16(samples) => samples[..len]
            .chunks_exact(channels)
            .map(|frame| {
                (frame.iter().map(|sample| i64::from(*sample)).sum::<i64>() / channels as i64)
                    as i16
            })
            .collect(),
        RawSamples::F32(samples) => samples[..len]
            .chunks_exact(channels)
            .map(|frame| {
                let mono = frame
                    .iter()
                    .map(|sample| sample.clamp(-1.0, 1.0))
                    .sum::<f32>()
                    / channels as f32;
                (mono * 32767.0)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect(),
        RawSamples::U16(samples) => samples[..len]
            .chunks_exact(channels)
            .map(|frame| {
                let mono = frame
                    .iter()
                    .map(|sample| *sample as f32 / 65535.0)
                    .sum::<f32>()
                    / channels as f32;
                ((mono * 2.0 - 1.0) * 32767.0)
                    .round()
                    .clamp(i16::MIN as f32, i16::MAX as f32) as i16
            })
            .collect(),
    }
}

fn device_error(error: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("audio device error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_shutdown_stats_include_terminal_dropped_frame() {
        let (sink, mut receiver) = mpsc::channel(1);
        let dropped_chunks = AtomicU64::new(0);
        let dropped_frames = AtomicU64::new(0);
        send_frame(&sink, &dropped_frames, AudioSource::Microphone, 1, vec![1]);

        let stats = CaptureStats::after_stop(
            || {
                // The capture worker flushes a final frame during stop. A full
                // sink drops it, leaving no later received sequence to show a gap.
                send_frame(&sink, &dropped_frames, AudioSource::Microphone, 2, vec![2]);
            },
            &dropped_chunks,
            &dropped_frames,
        );

        assert_eq!(receiver.try_recv().unwrap().sequence(), 1);
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            stats,
            CaptureStats {
                dropped_chunks: 0,
                dropped_frames: 1
            }
        );
    }

    #[test]
    fn capture_folds_interleaved_channels_to_mono() {
        let mono = downmix_to_i16(&RawSamples::I16(vec![100, 300, -100, 100]), 4, 2);
        assert_eq!(mono, vec![200, 0]);
    }

    #[test]
    fn capture_converts_float_samples_to_pcm16() {
        let mono = downmix_to_i16(&RawSamples::F32(vec![-1.0, 0.0, 1.0]), 3, 1);
        assert_eq!(mono, vec![-32767, 0, 32767]);
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[test]
    fn low_rate_callback_and_tail_are_split_with_sample_cursor_times() {
        let mut converter = CaptureConverter::new(8000, AudioSource::System).unwrap();
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut frames = converter.block(&vec![900; 8192], 0, at).unwrap();
        frames.extend(converter.flush().unwrap());
        assert_eq!(
            frames.iter().map(|f| f.samples().len()).sum::<usize>(),
            16384
        );
        let mut cursor = 0;
        for (sequence, frame) in frames.iter().enumerate() {
            assert!(frame.samples().len() <= 1280);
            assert_eq!(frame.source(), AudioSource::System);
            assert_eq!(frame.sequence(), sequence as u64);
            assert_eq!(
                frame.captured_at(),
                at + Duration::from_nanos(cursor * 1_000_000_000 / 16000)
            );
            cursor += frame.samples().len() as u64;
        }
    }
    #[test]
    fn raw_loss_drops_converter_carry_even_without_a_newer_block() {
        let (tx, rx) = sync_mpsc::sync_channel(8);
        let (out, mut frames) = mpsc::channel(50);
        let shared = AudioShared::new(BoundedAudioQueue::default());
        let discontinuity_generation = Arc::new(AtomicU64::new(0));
        let worker_shared = shared.clone();
        let worker_generation = discontinuity_generation.clone();
        let (returned, returned_rx) = crossbeam_channel::bounded(8);
        let worker = thread::spawn(move || {
            process_capture(
                rx,
                CaptureSink::Legacy(out),
                AudioSource::Microphone,
                48000,
                1,
                Arc::default(),
                Arc::default(),
                returned,
                worker_shared,
                worker_generation,
            )
        });
        tx.send(RawBlock {
            samples: RawSamples::I16(vec![2000; 3]),
            len: 3,
            generation: 0,
            captured_at: SystemTime::UNIX_EPOCH,
        })
        .unwrap();
        // Receiving the returned raw pool block proves this block reached conversion.
        returned_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        shared.counters.native_blocks.store(1, Ordering::Release);
        discontinuity_generation.store(1, Ordering::Release);
        shared.stop_capture();
        drop(tx);
        worker.join().unwrap();
        assert!(
            frames.try_recv().is_err(),
            "obsolete resampler tail was published after raw loss"
        );
    }
    #[test]
    fn stale_native_blocks_are_counted_without_rejecting_the_fresh_generation() {
        let (tx, rx) = sync_mpsc::sync_channel(8);
        let (out, mut frames) = mpsc::channel(50);
        let shared = AudioShared::new(BoundedAudioQueue::default());
        shared.counters.native_blocks.store(1, Ordering::Release);
        let discontinuity_generation = Arc::new(AtomicU64::new(1));
        for (generation, sample) in [(0, 100), (0, 200), (1, 3000)] {
            tx.send(RawBlock {
                samples: RawSamples::I16(vec![sample; 128]),
                len: 128,
                generation,
                captured_at: SystemTime::UNIX_EPOCH,
            })
            .unwrap();
        }
        drop(tx);
        let (returned, _) = crossbeam_channel::bounded(8);
        let worker_shared = shared.clone();
        thread::spawn(move || {
            process_capture(
                rx,
                CaptureSink::Legacy(out),
                AudioSource::Microphone,
                16000,
                1,
                Arc::default(),
                Arc::default(),
                returned,
                worker_shared,
                discontinuity_generation,
            )
        })
        .join()
        .unwrap();

        assert_eq!(shared.counters.native_blocks.load(Ordering::Acquire), 3);
        let emitted = frames
            .try_recv()
            .expect("fresh generation should be emitted");
        assert!(emitted.samples().iter().all(|sample| *sample > 1000));
        assert!(frames.try_recv().is_err());
    }
    #[test]
    fn sample_cursor_does_not_overflow_after_thirteen_days() {
        let mut converter = CaptureConverter::new(16000, AudioSource::Microphone).unwrap();
        converter.base = Some(SystemTime::UNIX_EPOCH);
        converter.emitted = 16000 * 86400 * 365;
        let frames = converter.frames(vec![0]).unwrap();
        assert_eq!(
            frames[0].captured_at(),
            SystemTime::UNIX_EPOCH + Duration::from_secs(86400 * 365)
        );
    }
    #[test]
    fn generation_change_resets_real_resampler_before_new_audio() {
        let at = SystemTime::UNIX_EPOCH;
        let mut converted = CaptureConverter::new(48000, AudioSource::Microphone).unwrap();
        converted.block(&vec![24000; 37], 0, at).unwrap();
        let mut actual = converted
            .block(&vec![0; 4096], 1, at + Duration::from_secs(1))
            .unwrap();
        actual.extend(converted.flush().unwrap());
        assert!(actual.iter().flat_map(|f| f.samples()).all(|s| *s == 0));
    }
}
