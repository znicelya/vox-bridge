use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc as sync_mpsc,
    },
    thread::{self, JoinHandle},
};

use cpal::{
    Device, FromSample, Sample, SampleFormat as CpalSampleFormat, SizedSample, Stream,
    StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use tokio::sync::mpsc;

use crate::{
    audio::{AudioFrame, PcmSpec, Resampler, SampleFormat},
    domain::audio::{AudioSource, SAMPLE_RATE_HZ},
    error::{AppError, AppResult},
};

use super::platform::{AudioDirection, AudioEndpoint};

const CAPTURE_INGRESS_BLOCKS: usize = 8;

enum RawSamples {
    I16(Vec<i16>),
    F32(Vec<f32>),
    U16(Vec<u16>),
}

struct RawBlock {
    samples: RawSamples,
    len: usize,
}

const CAPTURE_BUFFER_SAMPLES: usize = 8192;

trait CaptureSample: SizedSample + Copy {}
impl CaptureSample for i16 {}
impl CaptureSample for f32 {}
impl CaptureSample for u16 {}

pub struct CaptureHandle {
    stream: Option<Stream>,
    worker: Option<JoinHandle<()>>,
    dropped_chunks: Arc<AtomicU64>,
    dropped_frames: Arc<AtomicU64>,
    disconnected: Arc<AtomicBool>,
}

impl CaptureHandle {
    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }

    pub fn is_disconnected(&self) -> bool {
        self.disconnected.load(Ordering::Acquire)
    }

    pub fn stop(mut self) {
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.stream.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct AudioCapture;

impl AudioCapture {
    pub fn start(
        endpoint: AudioEndpoint,
        sink: mpsc::Sender<AudioFrame>,
    ) -> AppResult<CaptureHandle> {
        if endpoint.direction != AudioDirection::Input {
            return Err(AppError::InvalidRequest(format!(
                "audio capture endpoint {} is not an input",
                endpoint.id
            )));
        }

        let device = find_input_device(&endpoint)?;
        let supported = device.default_input_config().map_err(device_error)?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        if channels == 0 {
            return Err(AppError::InvalidRequest(
                "audio capture device has no channels".into(),
            ));
        }
        let config: StreamConfig = supported.clone().into();
        let (ingress_tx, ingress_rx) = sync_mpsc::sync_channel(CAPTURE_INGRESS_BLOCKS);
        let dropped_chunks = Arc::new(AtomicU64::new(0));
        let dropped_frames = Arc::new(AtomicU64::new(0));
        let disconnected = Arc::new(AtomicBool::new(false));

        let worker_dropped_frames = dropped_frames.clone();
        let worker_disconnected = disconnected.clone();
        let source = if endpoint.display_name == "VoxBridge Input" {
            AudioSource::System
        } else {
            AudioSource::Microphone
        };
        let (pool_tx, pool_rx) = make_pool(supported.sample_format(), channels);
        let worker_pool = pool_tx.clone();
        let worker = thread::Builder::new()
            .name("vox-audio-capture-convert".into())
            .spawn(move || {
                process_capture_blocks(
                    ingress_rx,
                    sink,
                    source,
                    sample_rate,
                    channels,
                    worker_dropped_frames,
                    worker_disconnected,
                    worker_pool,
                );
            })
            .map_err(|error| {
                AppError::Internal(format!("failed to start capture worker: {error}"))
            })?;

        let stream = match supported.sample_format() {
            CpalSampleFormat::I16 => build_capture_stream::<i16>(
                &device,
                &config,
                ingress_tx.clone(),
                pool_rx.clone(),
                pool_tx.clone(),
                dropped_chunks.clone(),
                disconnected.clone(),
            ),
            CpalSampleFormat::F32 => build_capture_stream::<f32>(
                &device,
                &config,
                ingress_tx.clone(),
                pool_rx.clone(),
                pool_tx.clone(),
                dropped_chunks.clone(),
                disconnected.clone(),
            ),
            CpalSampleFormat::U16 => build_capture_stream::<u16>(
                &device,
                &config,
                ingress_tx.clone(),
                pool_rx.clone(),
                pool_tx.clone(),
                dropped_chunks.clone(),
                disconnected.clone(),
            ),
            format => {
                return Err(AppError::InvalidRequest(format!(
                    "unsupported audio capture sample format: {format:?}"
                )));
            }
        }?;
        stream.play().map_err(device_error)?;

        Ok(CaptureHandle {
            stream: Some(stream),
            worker: Some(worker),
            dropped_chunks,
            dropped_frames,
            disconnected,
        })
    }
}

fn build_capture_stream<T: CaptureSample + Sample + FromSample<T> + Send + 'static>(
    device: &Device,
    config: &StreamConfig,
    ingress: sync_mpsc::SyncSender<RawBlock>,
    pool: crossbeam_channel::Receiver<RawSamples>,
    pool_return: crossbeam_channel::Sender<RawSamples>,
    dropped_chunks: Arc<AtomicU64>,
    disconnected: Arc<AtomicBool>,
) -> AppResult<Stream> {
    let error_state = disconnected.clone();
    let error = move |error| {
        error_state.store(true, Ordering::Release);
        tracing::error!(%error, "audio capture device disconnected");
    };
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if disconnected.load(Ordering::Acquire) {
                    return;
                }
                let Some(mut buffer) = pool.try_recv().ok() else {
                    dropped_chunks.fetch_add(1, Ordering::Relaxed);
                    return;
                };
                let copied = match &mut buffer {
                    RawSamples::I16(samples) => {
                        let n = data.len().min(samples.len());
                        samples[..n].copy_from_slice(unsafe {
                            std::slice::from_raw_parts(data.as_ptr() as *const i16, n)
                        });
                        n
                    }
                    RawSamples::F32(samples) => {
                        let n = data.len().min(samples.len());
                        samples[..n].copy_from_slice(unsafe {
                            std::slice::from_raw_parts(data.as_ptr() as *const f32, n)
                        });
                        n
                    }
                    RawSamples::U16(samples) => {
                        let n = data.len().min(samples.len());
                        samples[..n].copy_from_slice(unsafe {
                            std::slice::from_raw_parts(data.as_ptr() as *const u16, n)
                        });
                        n
                    }
                };
                if copied < data.len() {
                    dropped_chunks.fetch_add(1, Ordering::Relaxed);
                    let _ = pool_return.try_send(buffer);
                    return;
                }
                if let Err(error) = ingress.try_send(RawBlock {
                    samples: buffer,
                    len: copied,
                }) {
                    dropped_chunks.fetch_add(1, Ordering::Relaxed);
                    let block = match error {
                        sync_mpsc::TrySendError::Full(block)
                        | sync_mpsc::TrySendError::Disconnected(block) => block,
                    };
                    let _ = pool_return.try_send(block.samples);
                }
            },
            error,
            None,
        )
        .map_err(device_error)
}

fn find_input_device(endpoint: &AudioEndpoint) -> AppResult<Device> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(device_error)?;
    for device in devices {
        if device
            .name()
            .is_ok_and(|name| name == endpoint.id || name == endpoint.display_name)
        {
            return Ok(device);
        }
    }
    Err(AppError::InvalidRequest(format!(
        "audio input endpoint {} is unavailable",
        endpoint.id
    )))
}

fn process_capture_blocks(
    receiver: sync_mpsc::Receiver<RawBlock>,
    sink: mpsc::Sender<AudioFrame>,
    source: AudioSource,
    sample_rate: u32,
    channels: u16,
    dropped_frames: Arc<AtomicU64>,
    disconnected: Arc<AtomicBool>,
    pool: crossbeam_channel::Sender<RawSamples>,
) {
    let from = match PcmSpec::new(sample_rate, 1, SampleFormat::I16) {
        Ok(spec) => spec,
        Err(error) => {
            tracing::error!(%error, "invalid capture device sample rate");
            return;
        }
    };
    let mut resampler = match Resampler::new(from, PcmSpec::pcm16_mono(SAMPLE_RATE_HZ)) {
        Ok(resampler) => resampler,
        Err(error) => {
            tracing::error!(%error, "failed to configure capture resampler");
            return;
        }
    };
    let mut sequence = 0;
    while let Ok(block) = receiver.recv() {
        let mono = downmix_to_i16(&block.samples, block.len, channels);
        let _ = pool.try_send(block.samples);
        if mono.is_empty() {
            continue;
        }
        // Buffers are recycled by the callback-side pool in the stream owner.
        let converted = match resampler.process(&mono) {
            Ok(converted) => converted,
            Err(error) => {
                tracing::error!(%error, "capture resampling failed");
                disconnected.store(true, Ordering::Release);
                break;
            }
        };
        if !converted.is_empty() {
            sequence += 1;
            send_frame(&sink, &dropped_frames, source, sequence, converted);
        }
    }
    if let Ok(tail) = resampler.flush() {
        if !tail.is_empty() {
            sequence += 1;
            send_frame(&sink, &dropped_frames, source, sequence, tail);
        }
    }
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
        let buffer = match format {
            CpalSampleFormat::I16 => {
                RawSamples::I16(vec![0; CAPTURE_BUFFER_SAMPLES * channels as usize])
            }
            CpalSampleFormat::F32 => {
                RawSamples::F32(vec![0.0; CAPTURE_BUFFER_SAMPLES * channels as usize])
            }
            CpalSampleFormat::U16 => {
                RawSamples::U16(vec![0; CAPTURE_BUFFER_SAMPLES * channels as usize])
            }
            _ => RawSamples::I16(vec![0; CAPTURE_BUFFER_SAMPLES]),
        };
        let _ = tx.try_send(buffer);
    }
    (tx, rx)
}

fn send_frame(
    sink: &mpsc::Sender<AudioFrame>,
    dropped_frames: &AtomicU64,
    source: AudioSource,
    sequence: u64,
    samples: Vec<i16>,
) {
    match AudioFrame::new(
        source,
        sequence,
        PcmSpec::pcm16_mono(SAMPLE_RATE_HZ),
        samples,
    ) {
        Ok(frame) => {
            if sink.try_send(frame).is_err() {
                dropped_frames.fetch_add(1, Ordering::Relaxed);
            }
        }
        Err(error) => tracing::error!(%error, "failed to construct captured audio frame"),
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
