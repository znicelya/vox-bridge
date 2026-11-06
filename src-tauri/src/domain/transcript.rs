//! Audio-free directed text and safe runtime diagnostics.
use super::session::{ProviderId, SessionDirection};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptKind {
    Source,
    Translation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptDetails {
    pub kind: TranscriptKind,
    pub provider: ProviderId,
    pub provider_config_id: Uuid,
    pub stream_epoch: u64,
    pub provider_event_type: Option<String>,
    pub provider_timestamp_ms: Option<u64>,
    pub sentence_id: Option<String>,
    pub local_correlation_id: Option<u64>,
    pub log_id: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptUpdate {
    pub id: Uuid,
    pub session_id: Uuid,
    pub direction: SessionDirection,
    pub sequence: u64,
    pub occurred_at_ms: i64,
    pub language: String,
    pub text: String,
    pub is_final: bool,
    pub details: Option<TranscriptDetails>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptPage {
    pub rows: Vec<TranscriptUpdate>,
    pub next_cursor: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailureCode {
    Provider,
    Storage,
    DeviceUnavailable,
    AudioBackendUnavailable,
    StartupTimeout,
    FinishTimeout,
    PlaybackTimeout,
    CleanupTimeout,
    Interrupted,
    InvalidEvent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapCause {
    ProviderReconnect,
    CaptureOverflow,
    OutputOverflow,
    DeviceLoss,
    ManualResume,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackBoundary {
    CallbackConsumed,
    PulseServerDrained,
    Discarded,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureStatistics {
    pub dropped_native_blocks: u64,
    pub dropped_normalized_frames: u64,
    pub finalized: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePlaybackStatistics {
    pub sample_rate_hz: u32,
    pub generated_mono_samples: u64,
    pub callback_consumed_mono_samples: u64,
    pub discarded_mono_samples: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackStatistics {
    pub input_sample_rate_hz: u32,
    pub accepted_for_processing_samples: u64,
    pub discarded_before_conversion_samples: u64,
    pub native: Option<NativePlaybackStatistics>,
    pub finalized: bool,
    pub tail_disposition_known: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStatistics {
    pub capture: CaptureStatistics,
    pub playback: PlaybackStatistics,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticPayload {
    CaptureAccounting {
        statistics: CaptureStatistics,
    },
    PlaybackAccounting {
        boundary: Option<PlaybackBoundary>,
        statistics: PlaybackStatistics,
    },
    Gap {
        cause: GapCause,
        dropped_frames: u64,
    },
    TranscriptOverflow {
        dropped_interims: u64,
    },
    Failure {
        code: RuntimeFailureCode,
        provider_code: Option<i32>,
        log_id: Option<String>,
    },
    Latency {
        capture_to_send_ms: Option<u64>,
        response_to_enqueue_ms: Option<u64>,
        enqueue_to_callback_ms: Option<u64>,
        provider_processing_ms: Option<u64>,
    },
    Playback {
        boundary: PlaybackBoundary,
        accepted_samples: u64,
        discarded_samples: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDiagnostic {
    pub id: Uuid,
    pub session_id: Uuid,
    pub direction: Option<SessionDirection>,
    pub stream_epoch: Option<u64>,
    pub occurred_at_ms: i64,
    pub payload: DiagnosticPayload,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPage {
    pub rows: Vec<RuntimeDiagnostic>,
    pub next_cursor: Option<i64>,
}

// Matches the largest currently supported provider envelope (Volc AST).
pub(crate) const MAX_TRANSCRIPT_BYTES: usize = 262_144;

pub(crate) fn invalid_transcript() -> AppError {
    AppError::InvalidRequest("invalid directed transcript metadata".into())
}
pub(crate) fn validate_log_id(log_id: &Option<String>) -> AppResult<()> {
    if log_id.as_ref().is_some_and(|id| {
        id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    }) {
        return Err(invalid_transcript());
    }
    Ok(())
}
impl TranscriptUpdate {
    pub(crate) fn validate(&self) -> AppResult<()> {
        let details = self.details.as_ref().ok_or_else(invalid_transcript)?;
        if self.direction == SessionDirection::Bidirectional
            || self.sequence > i64::MAX as u64
            || self.language.len() != 2
            || !self.language.bytes().all(|b| b.is_ascii_alphabetic())
            || details.provider.as_str().len() > 128
            || matches!((details.start_ms, details.end_ms), (Some(start), Some(end)) if end < start)
        {
            return Err(invalid_transcript());
        }
        let bytes = self
            .text
            .len()
            .saturating_add(self.language.len())
            .saturating_add(details.provider_event_type.as_ref().map_or(0, String::len))
            .saturating_add(details.sentence_id.as_ref().map_or(0, String::len))
            .saturating_add(details.log_id.as_ref().map_or(0, String::len));
        if bytes > MAX_TRANSCRIPT_BYTES {
            return Err(invalid_transcript());
        }
        validate_log_id(&details.log_id)
    }
}
impl RuntimeDiagnostic {
    pub(crate) fn validate(&self) -> AppResult<()> {
        if self.direction == Some(SessionDirection::Bidirectional) {
            return Err(AppError::InvalidRequest(
                "diagnostics require one direction or session scope".into(),
            ));
        }
        if let DiagnosticPayload::Failure { log_id, .. } = &self.payload {
            validate_log_id(log_id)?;
        }
        Ok(())
    }
}
