use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::{
    FromSample, Sample, SampleFormat as CpalSampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Consumer, Producer, Split},
};
use tokio::sync::mpsc;

use crate::{
    audio::{AudioFrame, PcmSpec, Resampler},
    error::{AppError, AppResult},
};

use super::platform::{AudioDirection, AudioEndpoint};

const PLAYBACK_BUFFER_SECONDS: usize = 5;
const EVENT_CAPACITY: usize = 16;

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

const STATE_RUNNING: u8 = 0;
const STATE_UNDERFLOW: u8 = 1;
const STATE_DISCONNECTED: u8 = 2;

pub struct PlaybackHandle {
    stream: Option<Stream>,
    worker: Option<JoinHandle<()>>,
    state: Arc<AtomicU8>,
    stopped: Arc<AtomicBool>,
    events: mpsc::Receiver<PlaybackEvent>,
    dropped_samples: Arc<AtomicU64>,
}

impl PlaybackHandle {
    pub fn state(&self) -> PlaybackState {
        decode_state(self.state.load(Ordering::Acquire))
    }

    pub fn is_disconnected(&self) -> bool {
        self.state() == PlaybackState::DeviceDisconnected
    }

    pub fn dropped_samples(&self) -> u64 {
        self.dropped_samples.load(Ordering::Relaxed)
    }

    pub fn try_recv_event(&mut self) -> Result<PlaybackEvent, mpsc::error::TryRecvError> {
        self.events.try_recv()
    }

    pub fn stop(mut self) {
        self.stopped.store(true, Ordering::Release);
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for PlaybackHandle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct AudioPlayback;

impl AudioPlayback {
    pub fn start(
        endpoint: AudioEndpoint,
        source: mpsc::Receiver<AudioFrame>,
    ) -> AppResult<PlaybackHandle> {
        if endpoint.direction != AudioDirection::Output {
            return Err(AppError::InvalidRequest(format!(
                "audio playback endpoint {} is not an output",
                endpoint.id
            )));
        }

        let device = find_output_device(&endpoint)?;
        let supported = device.default_output_config().map_err(device_error)?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        if channels == 0 {
            return Err(AppError::InvalidRequest(
                "audio playback device has no channels".into(),
            ));
        }
        let config: StreamConfig = supported.clone().into();
        let capacity_samples = (sample_rate as usize).saturating_mul(PLAYBACK_BUFFER_SECONDS);
        let ring = HeapRb::<i16>::new(capacity_samples);
        let (producer, consumer) = ring.split();
        let state = Arc::new(AtomicU8::new(STATE_RUNNING));
        let stopped = Arc::new(AtomicBool::new(false));
        let dropped_samples = Arc::new(AtomicU64::new(0));
        let (event_tx, events) = mpsc::channel(EVENT_CAPACITY);

        let worker_buffer = producer;
        let worker_state = state.clone();
        let worker_stopped = stopped.clone();
        let worker_dropped = dropped_samples.clone();
        let worker = thread::Builder::new()
            .name("vox-audio-playback-convert".into())
            .spawn(move || {
                process_playback_frames(
                    source,
                    worker_buffer,
                    capacity_samples,
                    sample_rate,
                    worker_state,
                    worker_stopped,
                    worker_dropped,
                );
            })
            .map_err(|error| {
                AppError::Internal(format!("failed to start playback worker: {error}"))
            })?;

        let stream = match supported.sample_format() {
            CpalSampleFormat::I16 => build_playback_stream::<i16>(
                &device,
                &config,
                consumer,
                state.clone(),
                event_tx.clone(),
            ),
            CpalSampleFormat::F32 => build_playback_stream::<f32>(
                &device,
                &config,
                consumer,
                state.clone(),
                event_tx.clone(),
            ),
            CpalSampleFormat::U16 => build_playback_stream::<u16>(
                &device,
                &config,
                consumer,
                state.clone(),
                event_tx.clone(),
            ),
            format => {
                return Err(AppError::InvalidRequest(format!(
                    "unsupported audio playback sample format: {format:?}"
                )));
            }
        }?;
        stream.play().map_err(device_error)?;

        Ok(PlaybackHandle {
            stream: Some(stream),
            worker: Some(worker),
            state,
            stopped,
            events,
            dropped_samples,
        })
    }
}

fn build_playback_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut consumer: HeapCons<i16>,
    state: Arc<AtomicU8>,
    events: mpsc::Sender<PlaybackEvent>,
) -> AppResult<Stream>
where
    T: Sample + FromSample<f32> + cpal::SizedSample + Send + 'static,
{
    let channels = usize::from(config.channels);
    let error_state = state.clone();
    let error_events = events.clone();
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                if state.load(Ordering::Acquire) == STATE_DISCONNECTED {
                    output.fill(T::from_sample(0.0));
                    return;
                }
                let mut underflow = false;
                for frame in output.chunks_mut(channels) {
                    let sample = consumer.try_pop().unwrap_or_else(|| {
                        underflow = true;
                        0
                    });
                    let sample = sample as f32 / 32768.0;
                    let converted = T::from_sample(sample);
                    frame.fill(converted);
                }
                if underflow {
                    if mark_underflow(&state) {
                        let _ = events.try_send(PlaybackEvent::AudioUnderflow);
                    }
                } else {
                    let _ = clear_underflow(&state);
                }
            },
            move |error| {
                tracing::error!(%error, "audio playback device disconnected");
                if error_state.swap(STATE_DISCONNECTED, Ordering::AcqRel) != STATE_DISCONNECTED {
                    let _ = error_events.try_send(PlaybackEvent::DeviceDisconnected);
                }
            },
            None,
        )
        .map_err(device_error)
}

fn find_output_device(endpoint: &AudioEndpoint) -> AppResult<cpal::Device> {
    let host = cpal::default_host();
    let devices = host.output_devices().map_err(device_error)?;
    for device in devices {
        if device
            .name()
            .is_ok_and(|name| name == endpoint.id || name == endpoint.display_name)
        {
            return Ok(device);
        }
    }
    Err(AppError::InvalidRequest(format!(
        "audio output endpoint {} is unavailable",
        endpoint.id
    )))
}

fn process_playback_frames(
    mut source: mpsc::Receiver<AudioFrame>,
    mut producer: HeapProd<i16>,
    _capacity_samples: usize,
    device_rate: u32,
    state: Arc<AtomicU8>,
    stopped: Arc<AtomicBool>,
    dropped_samples: Arc<AtomicU64>,
) {
    while !stopped.load(Ordering::Acquire) && state.load(Ordering::Acquire) != STATE_DISCONNECTED {
        match source.try_recv() {
            Ok(frame) => {
                let spec = frame.spec();
                let samples = if spec.sample_rate() == device_rate {
                    frame.samples().to_vec()
                } else {
                    match Resampler::convert(
                        frame.samples(),
                        spec,
                        PcmSpec::pcm16_mono(device_rate),
                    ) {
                        Ok(samples) => samples,
                        Err(error) => {
                            tracing::error!(%error, "playback resampling failed");
                            continue;
                        }
                    }
                };
                let mut overflow = 0;
                for sample in samples.iter().copied() {
                    if producer.try_push(sample).is_err() {
                        overflow += 1;
                    }
                }
                if overflow > 0 {
                    dropped_samples.fetch_add(overflow as u64, Ordering::Relaxed);
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => thread::sleep(Duration::from_millis(2)),
            Err(mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
}

fn device_error(error: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("audio device error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_playback_ring_discards_oldest_samples() {
        let ring = HeapRb::<i16>::new(4);
        let (mut producer, mut consumer) = ring.split();
        for sample in [1, 2, 3, 4] {
            producer.try_push(sample).unwrap();
        }
        for sample in [5, 6] {
            let _ = producer.try_pop();
            producer.try_push(sample).unwrap();
        }
        assert_eq!(consumer.try_pop(), Some(1));
    }

    #[test]
    fn playback_state_distinguishes_underflow_from_device_loss() {
        assert_eq!(decode_state(STATE_RUNNING), PlaybackState::Running);
        assert_eq!(decode_state(STATE_UNDERFLOW), PlaybackState::AudioUnderflow);
        assert_eq!(
            decode_state(STATE_DISCONNECTED),
            PlaybackState::DeviceDisconnected
        );
    }

    #[test]
    fn disconnect_is_terminal_when_underflow_recovery_races() {
        let state = AtomicU8::new(STATE_RUNNING);
        assert!(mark_underflow(&state));
        state.store(STATE_DISCONNECTED, Ordering::Release);

        assert!(!clear_underflow(&state));
        assert!(!mark_underflow(&state));
        assert_eq!(
            decode_state(state.load(Ordering::Acquire)),
            PlaybackState::DeviceDisconnected
        );
    }

    #[test]
    fn underflow_transition_emits_once_until_audio_resumes() {
        let state = AtomicU8::new(STATE_RUNNING);
        assert!(mark_underflow(&state));
        assert!(!mark_underflow(&state));
        assert!(clear_underflow(&state));
        assert!(mark_underflow(&state));
    }
}

fn decode_state(state: u8) -> PlaybackState {
    match state {
        STATE_UNDERFLOW => PlaybackState::AudioUnderflow,
        STATE_DISCONNECTED => PlaybackState::DeviceDisconnected,
        _ => PlaybackState::Running,
    }
}

fn mark_underflow(state: &AtomicU8) -> bool {
    state
        .compare_exchange(
            STATE_RUNNING,
            STATE_UNDERFLOW,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}

fn clear_underflow(state: &AtomicU8) -> bool {
    state
        .compare_exchange(
            STATE_UNDERFLOW,
            STATE_RUNNING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
}
