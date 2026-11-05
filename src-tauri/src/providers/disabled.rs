//! Safe fallback for provider identities without an enabled adapter.
use async_trait::async_trait;
use std::time::Duration;

use super::{
    ProviderCapabilities, ProviderError, ProviderResult, ProviderSessionConfig, ProviderStream,
    TranslationProvider,
};
use crate::{audio::PcmSpec, domain::session::ProviderId};

pub struct DisabledProvider {
    id: ProviderId,
}
impl DisabledProvider {
    pub fn new(id: ProviderId) -> Self {
        Self { id }
    }
}
#[async_trait]
impl TranslationProvider for DisabledProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            language_pairs: vec![],
            input_audio: PcmSpec::pcm16_mono(16_000),
            output_audio: PcmSpec::pcm16_mono(24_000),
            // No native chunk recommendation exists for an unknown provider.
            native_chunk_duration: Duration::ZERO,
            supports_tts: false,
            supports_bidirectional: false,
            ready: false,
            unavailable_reason: Some("未联调：此服务商没有已启用的同传适配器".into()),
        }
    }
    async fn connect(&self, _: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        Err(ProviderError::Unavailable)
    }
}

/// Protocol exercises are deliberately absent from production builds. Enabling
/// either service still requires a transport/lifecycle implementation and a
/// separately credentialed acceptance test.
#[cfg(test)]
pub(super) mod fixtures {
    use super::*;
    use crate::{
        audio::AudioFrame,
        domain::{audio::AudioSource, session::SessionDirection, translation::TimedTranslation},
        providers::{ProviderEvent, ProviderEventMetadata},
    };
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde_json::Value;

    const MAX_MESSAGE_BYTES: usize = 768 * 1024;
    const MAX_TEXT_BYTES: usize = 64 * 1024;
    // Palabra documents <=512 KiB base64 (<=384 KiB raw). Aliyun fixtures
    // use the same client-side safety budget, not an asserted server limit.
    const MAX_BASE64_BYTES: usize = 512 * 1024;

    pub fn object(bytes: &[u8]) -> ProviderResult<Value> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(ProviderError::Protocol);
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| ProviderError::Protocol)?;
        if !value.is_object() {
            return Err(ProviderError::Protocol);
        }
        Ok(value)
    }

    pub fn string(value: &Value, max: usize) -> ProviderResult<&str> {
        let text = value.as_str().ok_or(ProviderError::Protocol)?;
        if text.len() > max {
            return Err(ProviderError::Protocol);
        }
        Ok(text)
    }

    pub fn text(value: &Value) -> ProviderResult<String> {
        Ok(string(value, MAX_TEXT_BYTES)?.to_owned())
    }

    pub fn native_id(value: &Value) -> ProviderResult<String> {
        let id = string(value, 256)?;
        if id.is_empty() || id.chars().any(char::is_control) {
            return Err(ProviderError::Protocol);
        }
        Ok(id.to_owned())
    }

    pub fn metadata(config: &ProviderSessionConfig, kind: &str) -> ProviderEventMetadata {
        ProviderEventMetadata {
            session_id: config.session_id,
            direction: config.direction,
            provider_event_type: Some(kind.into()),
            timestamp: None,
            sentence_id: None,
            local_correlation_id: None,
            log_id: None,
        }
    }

    pub fn subtitle(
        metadata: ProviderEventMetadata,
        text: TimedTranslation,
        translated: bool,
        finality: bool,
    ) -> ProviderEvent {
        match (translated, finality) {
            (false, false) => ProviderEvent::SourceInterim { metadata, text },
            (false, true) => ProviderEvent::SourceFinal { metadata, text },
            (true, false) => ProviderEvent::TranslationInterim { metadata, text },
            (true, true) => ProviderEvent::TranslationFinal { metadata, text },
        }
    }

    pub fn audio(
        metadata: ProviderEventMetadata,
        encoded: &Value,
        sequence: u64,
    ) -> ProviderResult<ProviderEvent> {
        let encoded = string(encoded, MAX_BASE64_BYTES)?;
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| ProviderError::Protocol)?;
        if bytes.is_empty() || bytes.len() % 2 != 0 || bytes.len() > 384 * 1024 {
            return Err(ProviderError::Protocol);
        }
        let samples = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let source = if metadata.direction == SessionDirection::TargetToSource {
            AudioSource::System
        } else {
            AudioSource::Microphone
        };
        let frame = AudioFrame::new(source, sequence, PcmSpec::pcm16_mono(24_000), samples)
            .map_err(|_| ProviderError::Protocol)?;
        Ok(ProviderEvent::Audio { metadata, frame })
    }

    pub fn times(transcription: &Value) -> ProviderResult<(Option<Duration>, Option<Duration>)> {
        let Some(segments) = transcription.get("segments") else {
            return Ok((None, None));
        };
        let segments = segments.as_array().ok_or(ProviderError::Protocol)?;
        let mut first = None;
        let mut last = None;
        for segment in segments {
            let start = seconds(&segment["start"])?;
            let end = seconds(&segment["end"])?;
            if end < start {
                return Err(ProviderError::Protocol);
            }
            // The schema supplies per-segment ranges, not a guarantee of
            // disjoint ordered segments. Keep their enclosing relative range.
            first = Some(first.map_or(start, |earliest: Duration| earliest.min(start)));
            last = Some(last.map_or(end, |latest: Duration| latest.max(end)));
        }
        Ok((first, last))
    }

    fn seconds(value: &Value) -> ProviderResult<Duration> {
        let value = value.as_f64().ok_or(ProviderError::Protocol)?;
        Duration::try_from_secs_f64(value).map_err(|_| ProviderError::Protocol)
    }
}
