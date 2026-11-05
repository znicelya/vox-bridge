//! Bounded, stateful normalization of the official AST response schema.
use super::{errors::remote_error, protocol::*};
use crate::{
    audio::frame::AudioFrame,
    domain::{
        audio::AudioSource,
        session::SessionDirection,
        translation::{TimedTranslation, Translation},
    },
    providers::*,
};
use std::{collections::VecDeque, time::Duration};

pub const MAX_MESSAGE_BYTES: usize = 262_144;
/// At most two seconds of native 24 kHz float32 in one event.
pub const MAX_AUDIO_BYTES: usize = 192_000;
const CORRELATION_WINDOW: usize = 64;

#[derive(Clone, Copy)]
struct Boundary {
    start: Duration,
    id: u64,
}

/// IDs are local to this mapper. Only actual boundary timestamps establish
/// correspondence; audio without a TTS boundary has no sentence correlation.
pub struct VolcEventMapper {
    config: ProviderSessionConfig,
    log_id: Option<String>,
    boundaries: VecDeque<Boundary>,
    source: Option<Boundary>,
    translation: Option<Boundary>,
    tts: Option<Boundary>,
    next_id: u64,
    audio_sequence: u64,
}
impl VolcEventMapper {
    pub fn new(config: ProviderSessionConfig) -> Self {
        Self {
            config,
            log_id: None,
            boundaries: VecDeque::new(),
            source: None,
            translation: None,
            tts: None,
            next_id: 0,
            audio_sequence: 0,
        }
    }
    pub(super) fn set_log_id(&mut self, log_id: Option<String>) {
        self.log_id = log_id;
    }
    pub(super) fn reset_connection(&mut self) {
        self.boundaries.clear();
        self.source = None;
        self.translation = None;
        self.tts = None;
        self.log_id = None;
    }
    pub(super) fn metadata(&self) -> ProviderEventMetadata {
        ProviderEventMetadata {
            session_id: self.config.session_id,
            direction: self.config.direction,
            provider_event_type: None,
            timestamp: None,
            sentence_id: None,
            local_correlation_id: None,
            log_id: self.log_id.clone(),
        }
    }
    fn boundary(&mut self, start: Duration) -> Boundary {
        if let Some(boundary) = self.boundaries.iter().rev().find(|b| b.start == start) {
            return *boundary;
        }
        let boundary = Boundary {
            start,
            id: self.next_id,
        };
        self.next_id = self.next_id.saturating_add(1);
        if self.boundaries.len() == CORRELATION_WINDOW {
            self.boundaries.pop_front();
        }
        self.boundaries.push_back(boundary);
        boundary
    }
    pub fn map(&mut self, bytes: &[u8]) -> ProviderResult<Vec<ProviderEvent>> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(ProviderError::Protocol);
        }
        let response = decode_response(bytes)?;
        if response.start_time < 0
            || response.end_time < 0
            || response.muted_duration_ms < 0
            || (matches!(response.event, 652 | 655 | 351)
                && response.end_time < response.start_time)
            || (response.end_time != 0 && response.end_time < response.start_time)
        {
            return Err(ProviderError::Protocol);
        }
        if response.response_meta.as_ref().is_some_and(|m| {
            !m.session_id.is_empty() && m.session_id != self.config.session_id.to_string()
        }) {
            return Err(ProviderError::Protocol);
        }
        let mut metadata = self.metadata();
        metadata.provider_event_type = Some(response.event.to_string());
        let code = response.response_meta.as_ref().map_or(0, |m| m.status_code);
        if response.event == 153 || !matches!(code, 0 | 20_000_000) {
            return Ok(vec![ProviderEvent::Failed {
                metadata,
                error: remote_error(code),
            }]);
        }
        let event = match response.event {
            150 => ProviderEvent::SessionStarted { metadata },
            152 => ProviderEvent::Finished { metadata },
            250 => ProviderEvent::Muted {
                metadata,
                duration: Some(Duration::from_millis(response.muted_duration_ms as u64)),
            },
            650..=655 => {
                let translated = response.event >= 653;
                let finality = matches!(response.event, 652 | 655);
                let start = Duration::from_millis(response.start_time as u64);
                let active = if translated {
                    self.translation
                } else {
                    self.source
                };
                // Proto3 scalar fields have no presence bit. Interim packets
                // normally omit timing; zero alone cannot establish a boundary.
                let boundary = if matches!(response.event, 651 | 654) && response.start_time == 0 {
                    active
                } else {
                    Some(self.boundary(start))
                };
                if translated {
                    self.translation = if finality { None } else { boundary };
                } else {
                    self.source = if finality { None } else { boundary };
                }
                metadata.timestamp = boundary.map(|boundary| boundary.start);
                metadata.local_correlation_id = boundary.map(|boundary| boundary.id);
                let text = TimedTranslation {
                    translation: Translation {
                        text: response.text,
                        language: if translated {
                            self.config.languages.target()
                        } else {
                            self.config.languages.source()
                        }
                        .into(),
                    },
                    start: metadata.timestamp,
                    end: finality.then(|| Duration::from_millis(response.end_time as u64)),
                };
                match (translated, finality) {
                    (false, false) => ProviderEvent::SourceInterim { metadata, text },
                    (false, true) => ProviderEvent::SourceFinal { metadata, text },
                    (true, false) => ProviderEvent::TranslationInterim { metadata, text },
                    (true, true) => ProviderEvent::TranslationFinal { metadata, text },
                }
            }
            350 => {
                self.tts = Some(self.boundary(Duration::from_millis(response.start_time as u64)));
                return Ok(vec![]);
            }
            351 | 352 => {
                let samples = normalize_audio(&response.data)?;
                // Event351 carries its own sentence start timestamp. It can
                // establish correlation even when350 was absent or stale.
                let boundary = if response.event == 351 {
                    Some(self.boundary(Duration::from_millis(response.start_time as u64)))
                } else {
                    self.tts
                };
                if let Some(boundary) = boundary {
                    metadata.timestamp = Some(boundary.start);
                    metadata.local_correlation_id = Some(boundary.id);
                }
                if response.event == 351 {
                    self.tts = None;
                }
                if samples.is_empty() {
                    return Ok(vec![]);
                }
                let source = if self.config.direction == SessionDirection::TargetToSource {
                    AudioSource::System
                } else {
                    AudioSource::Microphone
                };
                let frame = AudioFrame::new(
                    source,
                    self.audio_sequence,
                    NORMALIZED_OUTPUT_AUDIO,
                    samples,
                )
                .map_err(|_| ProviderError::InvalidAudioFrame)?;
                self.audio_sequence = self.audio_sequence.saturating_add(1);
                ProviderEvent::Audio { metadata, frame }
            }
            // Billing and unsupported events are bounded by the envelope limit,
            // deliberately ignored, and never logged or copied into diagnostics.
            _ => return Ok(vec![]),
        };
        Ok(vec![event])
    }
}
fn normalize_audio(data: &[u8]) -> ProviderResult<Vec<i16>> {
    if data.len() > MAX_AUDIO_BYTES || !data.len().is_multiple_of(4) {
        return Err(ProviderError::InvalidAudioFrame);
    }
    data.chunks_exact(4)
        .map(|bytes| {
            let sample = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if !sample.is_finite() {
                return Err(ProviderError::InvalidAudioFrame);
            }
            Ok((sample.clamp(-1.0, 1.0) * 32768.0)
                .round()
                .clamp(-32768.0, 32767.0) as i16)
        })
        .collect()
}
