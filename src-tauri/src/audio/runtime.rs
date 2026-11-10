//! Send-only audio ports. Native handles never cross their owning thread.
pub use super::platform::inventory::PlatformAudioBackend;
use super::{AudioFrame, PcmSpec, platform::inventory::AudioInventory, queue::BoundedAudioQueue};
use crate::{
    domain::{audio::AudioSource, transcript::*},
    error::{AppError, AppResult},
};
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use tokio::sync::{Notify, mpsc, watch};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AudioOpenRequest {
    pub inventory_generation: Uuid,
    pub input_id: String,
    pub output_id: String,
    pub source: AudioSource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioHealth {
    Ready,
    Lost,
    NeedsRestart,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackReport {
    pub boundary: PlaybackBoundary,
    pub statistics: PlaybackStatistics,
}
#[async_trait]
pub trait AudioControl: Send + Sync {
    fn actual_join_port(&self) -> AppResult<super::join::AudioOwnerJoin> {
        Err(audio_error(
            "audio backend provides no actual owner join source",
        ))
    }
    fn suppress_now(&self);
    fn statistics(&self) -> AudioStatistics;
    /// Actual native ring loss, independent of expected suppression/cleanup discards.
    /// Reading/waiting never enters the native owner's command queue or locks.
    fn playback_overflow_generation(&self) -> u64 {
        0
    }
    async fn playback_overflow_changed(&self, _observed: u64) {
        std::future::pending::<()>().await;
    }
    async fn stop_capture(&self) -> AppResult<()>;
    async fn discard_pending(&self) -> AppResult<()>;
    async fn drain_playback(&self) -> AppResult<PlaybackReport>;
    async fn close(&self) -> AppResult<()>;
}
pub struct AudioPorts {
    pub capture: BoundedAudioQueue,
    pub playback: mpsc::Sender<AudioFrame>,
    pub health: watch::Receiver<AudioHealth>,
    pub control: Arc<dyn AudioControl>,
}
#[async_trait]
pub trait AudioBackend: Send + Sync {
    fn actual_epoch(&self) -> AppResult<u64> {
        Err(audio_error("audio backend provides no actual epoch source"))
    }
    fn begin_close(
        &self,
        _key: super::join::AudioCloseKey,
    ) -> AppResult<super::join::AudioBackendCloseJoin> {
        Err(audio_error(
            "audio backend provides no actual close join source",
        ))
    }
    fn reopen_after_close(&self, _ack: &super::join::AudioBackendCloseAck) -> AppResult<()> {
        Err(audio_error(
            "audio backend provides no actual reopen authority",
        ))
    }
    fn validate_reopen_after_close(
        &self,
        _ack: &super::join::AudioBackendCloseAck,
    ) -> AppResult<()> {
        Err(audio_error(
            "audio backend provides no actual reopen policy source",
        ))
    }
    async fn inventory(&self) -> AppResult<AudioInventory>;
    async fn open(&self, request: AudioOpenRequest) -> AppResult<AudioPorts>;
}
#[derive(Default)]
pub(crate) struct AudioGates {
    pub capture_suppressed: AtomicBool,
    pub playback_suppressed: AtomicBool,
    pub abort_pending: AtomicBool,
}
#[derive(Default)]
pub(crate) struct AudioCounters {
    pub native_blocks: AtomicU64,
    pub capture_finalized: AtomicBool,
    pub accepted: AtomicU64,
    pub pre_conversion_discarded: AtomicU64,
    pub native_rate: AtomicU32,
    pub native_generated: AtomicU64,
    pub native_consumed: AtomicU64,
    pub native_discarded: AtomicU64,
    pub playback_overflow_generation: AtomicU64,
    pub playback_finalized: AtomicBool,
    pub tail_known: AtomicBool,
}
#[derive(Clone)]
pub(crate) struct AudioShared {
    pub gates: Arc<AudioGates>,
    pub counters: Arc<AudioCounters>,
    pub changed: Arc<Notify>,
    capture: BoundedAudioQueue,
}
impl AudioShared {
    pub fn new(capture: BoundedAudioQueue) -> Self {
        Self {
            gates: Arc::default(),
            counters: Arc::default(),
            changed: Arc::new(Notify::new()),
            capture,
        }
    }
    pub fn suppress(&self) {
        self.gates.abort_pending.store(true, Ordering::Release);
        self.gates.capture_suppressed.store(true, Ordering::Release);
        self.gates
            .playback_suppressed
            .store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub fn stop_capture(&self) {
        self.gates.capture_suppressed.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
    pub fn statistics(&self) -> AudioStatistics {
        let c = &self.counters;
        let capture_finalized = c.capture_finalized.load(Ordering::Acquire);
        let playback_finalized = c.playback_finalized.load(Ordering::Acquire);
        let rate = c.native_rate.load(Ordering::Acquire);
        AudioStatistics {
            capture: CaptureStatistics {
                dropped_native_blocks: c.native_blocks.load(Ordering::Acquire),
                dropped_normalized_frames: self.capture.dropped_frames(),
                finalized: capture_finalized,
            },
            playback: PlaybackStatistics {
                input_sample_rate_hz: 24000,
                accepted_for_processing_samples: c.accepted.load(Ordering::Acquire),
                discarded_before_conversion_samples: c
                    .pre_conversion_discarded
                    .load(Ordering::Acquire),
                native: (rate != 0).then(|| NativePlaybackStatistics {
                    sample_rate_hz: rate,
                    generated_mono_samples: c.native_generated.load(Ordering::Acquire),
                    callback_consumed_mono_samples: c.native_consumed.load(Ordering::Acquire),
                    discarded_mono_samples: c.native_discarded.load(Ordering::Acquire),
                }),
                finalized: playback_finalized,
                tail_disposition_known: c.tail_known.load(Ordering::Acquire),
            },
        }
    }
    pub async fn aborted(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.gates.abort_pending.load(Ordering::Acquire) {
                return;
            }
            changed.await;
        }
    }
}
pub(crate) fn add(counter: &AtomicU64, value: u64) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
        Some(n.saturating_add(value))
    });
}
pub(crate) fn audio_error(message: &'static str) -> AppError {
    AudioFailure::device(message).into()
}
/// Cloneable, audio-free result retained by operation acknowledgements.
#[derive(Clone, Debug)]
pub(crate) struct AudioFailure {
    pub code: RuntimeFailureCode,
    pub message: &'static str,
}
impl AudioFailure {
    pub fn new(code: RuntimeFailureCode, message: &'static str) -> Self {
        Self { code, message }
    }
    pub fn device(message: &'static str) -> Self {
        Self::new(RuntimeFailureCode::DeviceUnavailable, message)
    }
    pub fn from_error(error: AppError) -> Self {
        match error {
            AppError::Audio { code, message } => Self { code, message },
            AppError::InvalidRequest(_) => {
                Self::new(RuntimeFailureCode::InvalidEvent, "invalid audio request")
            }
            _ => Self::device("audio operation failed"),
        }
    }
}
impl From<AudioFailure> for AppError {
    fn from(value: AudioFailure) -> Self {
        Self::Audio {
            code: value.code,
            message: value.message,
        }
    }
}
impl From<&'static str> for AudioFailure {
    fn from(message: &'static str) -> Self {
        Self::device(message)
    }
}

pub(crate) fn validate_playback(frame: &AudioFrame) -> AppResult<()> {
    if frame.spec() != PcmSpec::pcm16_mono(24000) || frame.samples().len() > 24000 {
        return Err(AppError::InvalidRequest(
            "invalid playback format or frame size".into(),
        ));
    }
    Ok(())
}

/// Health is terminal for this port; late cleanup cannot downgrade NeedsRestart.
pub(crate) fn mark_lost(health: &watch::Sender<AudioHealth>) {
    health.send_if_modified(|value| {
        if *value == AudioHealth::Ready {
            *value = AudioHealth::Lost;
            true
        } else {
            false
        }
    });
}

/// Sample cursor duration without multiplying the entire lifetime by nanoseconds.
pub(crate) fn sample_offset(samples: u64, rate: u32) -> std::time::Duration {
    let rate = u64::from(rate);
    std::time::Duration::new(
        samples / rate,
        ((samples % rate) * 1_000_000_000 / rate) as u32,
    )
}
