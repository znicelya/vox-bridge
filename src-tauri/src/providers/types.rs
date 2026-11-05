use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    audio::frame::{AudioFrame, PcmSpec},
    domain::{
        session::{LanguagePair, ProviderId, SessionDirection, SessionMode},
        translation::TimedTranslation,
    },
};

pub type ProviderResult<T> = Result<T, ProviderError>;
pub type ProviderStream = Box<dyn ProviderConnection>;

/// Errors contain classifications, never credentials or remote payloads.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("provider is not ready")]
    Unavailable,
    #[error("unsupported language pair")]
    UnsupportedLanguagePair,
    #[error("unsupported PCM format")]
    UnsupportedAudioFormat,
    #[error("bidirectional sessions are unsupported")]
    UnsupportedBidirectional,
    #[error("each provider stream requires a single direction")]
    InvalidDirection,
    #[error("provider handshake has not completed")]
    NotStarted,
    #[error("provider input is closed")]
    Closed,
    #[error("provider audio queue is full")]
    QueueFull,
    #[error("provider session was cancelled")]
    Cancelled,
    #[error("provider disconnected before completion")]
    Disconnected,
    #[error("provider authentication failed")]
    Authentication,
    #[error("invalid provider protocol event")]
    Protocol,
    #[error("fixture script exceeds its bounds")]
    InvalidScript,
    #[error("audio frame exceeds the native chunk size")]
    InvalidAudioFrame,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderSessionConfig {
    pub session_id: Uuid,
    /// The direction of this individual stream, even in a call session.
    pub direction: SessionDirection,
    pub mode: SessionMode,
    pub languages: LanguagePair,
    pub target_audio: PcmSpec,
    pub provider_config_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub language_pairs: Vec<LanguagePair>,
    pub input_audio: PcmSpec,
    pub output_audio: PcmSpec,
    pub native_chunk_duration: Duration,
    pub supports_tts: bool,
    pub supports_bidirectional: bool,
    pub ready: bool,
    pub unavailable_reason: Option<String>,
}

impl ProviderCapabilities {
    pub fn validate(&self, config: &ProviderSessionConfig) -> ProviderResult<()> {
        if !self.ready {
            return Err(ProviderError::Unavailable);
        }
        if config.direction == SessionDirection::Bidirectional {
            return Err(ProviderError::InvalidDirection);
        }
        if config.mode == SessionMode::Call && !self.supports_bidirectional {
            return Err(ProviderError::UnsupportedBidirectional);
        }
        if !self.language_pairs.contains(&config.languages) {
            return Err(ProviderError::UnsupportedLanguagePair);
        }
        if config.target_audio != self.output_audio
            || config.target_audio.validate().is_err()
            || self.input_audio.validate().is_err()
        {
            return Err(ProviderError::UnsupportedAudioFormat);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderEventMetadata {
    pub session_id: Uuid,
    pub direction: SessionDirection,
    /// Native numeric event IDs are represented as decimal strings.
    pub provider_event_type: Option<String>,
    /// Time relative to the provider session, never an assumed wall clock.
    pub timestamp: Option<Duration>,
    /// Only a sentence ID actually present on the wire; never a local ID.
    pub sentence_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderEvent {
    SessionStarted {
        metadata: ProviderEventMetadata,
    },
    SourceInterim {
        metadata: ProviderEventMetadata,
        text: TimedTranslation,
    },
    SourceFinal {
        metadata: ProviderEventMetadata,
        text: TimedTranslation,
    },
    TranslationInterim {
        metadata: ProviderEventMetadata,
        text: TimedTranslation,
    },
    TranslationFinal {
        metadata: ProviderEventMetadata,
        text: TimedTranslation,
    },
    Audio {
        metadata: ProviderEventMetadata,
        frame: AudioFrame,
    },
    Muted {
        metadata: ProviderEventMetadata,
    },
    Finished {
        metadata: ProviderEventMetadata,
    },
    Failed {
        metadata: ProviderEventMetadata,
        error: ProviderError,
    },
}

impl ProviderEvent {
    pub fn is_session_started(&self) -> bool {
        matches!(self, Self::SessionStarted { .. })
    }

    pub fn is_final_translation(&self) -> bool {
        matches!(self, Self::TranslationFinal { .. })
    }

    pub fn metadata(&self) -> &ProviderEventMetadata {
        match self {
            Self::SessionStarted { metadata }
            | Self::SourceInterim { metadata, .. }
            | Self::SourceFinal { metadata, .. }
            | Self::TranslationInterim { metadata, .. }
            | Self::TranslationFinal { metadata, .. }
            | Self::Audio { metadata, .. }
            | Self::Muted { metadata }
            | Self::Finished { metadata }
            | Self::Failed { metadata, .. } => metadata,
        }
    }

    pub(crate) fn metadata_mut(&mut self) -> &mut ProviderEventMetadata {
        match self {
            Self::SessionStarted { metadata }
            | Self::SourceInterim { metadata, .. }
            | Self::SourceFinal { metadata, .. }
            | Self::TranslationInterim { metadata, .. }
            | Self::TranslationFinal { metadata, .. }
            | Self::Audio { metadata, .. }
            | Self::Muted { metadata }
            | Self::Finished { metadata }
            | Self::Failed { metadata, .. } => metadata,
        }
    }
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> ProviderCapabilities;
    async fn connect(&self, config: ProviderSessionConfig) -> ProviderResult<ProviderStream>;
}

/// Separate input and output synchronization permits concurrent send/receive.
/// `send_audio` admits one frame or returns `QueueFull`; it never grows a queue.
/// `finish` seals input and requests graceful completion. `cancel` aborts it.
/// Both control methods are idempotent and bypass a saturated audio queue.
/// `next_event` yields exactly one terminal event, then `None` forever. Queued
/// events precede the terminal event. Dropping its future must not lose an event.
#[async_trait]
pub trait ProviderConnection: Send + Sync {
    async fn send_audio(&self, frame: AudioFrame) -> ProviderResult<()>;
    async fn finish(&self) -> ProviderResult<()>;
    async fn cancel(&self) -> ProviderResult<()>;
    async fn next_event(&self) -> ProviderResult<Option<ProviderEvent>>;
}
