//! Palabra's documented WebSocket profile; real transport remains disabled.
use super::{
    ProviderCapabilities, ProviderError, ProviderResult, ProviderSessionConfig, ProviderStream,
    TranslationProvider,
};
use crate::{
    audio::PcmSpec,
    domain::session::{LanguagePair, ProviderId},
};
use async_trait::async_trait;
use std::time::Duration;

pub const ENDPOINT_TEMPLATE: &str =
    "wss://streaming.palabra.ai/streaming-api/{random_hash}/v1/speech-to-speech/stream";
pub const AUTH_FIELDS: &[&str] = &["API_KEY"];
pub const AUTHENTICATION: &str = "Authorization header (or token query parameter)";
pub const WIRE_AUDIO_FORMAT: &str = "pcm_s16le";

// The separate Recognition and Translation columns in Supported languages.html.
// This is the product's two-letter subset; three-letter/regional source choices
// and additional regional target choices are not silently recoded.
const SOURCE_LANGUAGES: &[&str] = &[
    "ar", "hy", "eu", "be", "bn", "bg", "ca", "zh", "hr", "cs", "da", "nl", "en", "et", "fi", "fr",
    "gl", "de", "el", "he", "hi", "hu", "id", "ga", "it", "ja", "kk", "ko", "lv", "lt", "mk", "ms",
    "mt", "mr", "mn", "no", "fa", "pl", "pt", "ro", "ru", "sr", "sk", "sl", "es", "sw", "sv", "ta",
    "th", "tr", "uk", "ur", "ug", "vi", "cy",
];
const TARGET_LANGUAGES: &[&str] = &[
    "ar", "hy", "az", "be", "bs", "bg", "ca", "zh", "hr", "cs", "da", "nl", "en", "et", "fi", "fr",
    "gl", "de", "el", "he", "hi", "hu", "is", "id", "it", "ja", "kk", "ko", "lv", "lt", "mk", "ms",
    "no", "pl", "pt", "ro", "ru", "sr", "sk", "sl", "es", "sw", "sv", "ta", "th", "tr", "uk", "ur",
    "vi", "cy",
];

/// The canonical product choice `zh` currently means Simplified Chinese.
pub fn native_target_language(language: &str) -> ProviderResult<&str> {
    if !TARGET_LANGUAGES.contains(&language) {
        return Err(ProviderError::UnsupportedLanguagePair);
    }
    Ok(if language == "zh" {
        "zh-hans"
    } else {
        language
    })
}

pub struct PalabraProvider;
#[async_trait]
impl TranslationProvider for PalabraProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("palabra").unwrap()
    }
    fn capabilities(&self) -> ProviderCapabilities {
        let language_pairs = SOURCE_LANGUAGES
            .iter()
            .flat_map(|source| {
                TARGET_LANGUAGES
                    .iter()
                    .filter(move |target| source != *target)
                    .map(move |target| {
                        LanguagePair::new(source, target).expect("documented two-letter language")
                    })
            })
            .collect();
        ProviderCapabilities {
            language_pairs,
            // API accepts 16–48 kHz input; this profile chooses the documented
            // 24 kHz example. A future adapter must resample canonical 16 kHz.
            input_audio: PcmSpec::pcm16_mono(24_000),
            output_audio: PcmSpec::pcm16_mono(24_000),
            native_chunk_duration: Duration::from_millis(320),
            supports_tts: true,
            supports_bidirectional: true,
            ready: false,
            unavailable_reason: Some(
                "未联调：真实集成未在首版启用；本配置使用 24 kHz PCM16，建议按 320 ms 实时发送"
                    .into(),
            ),
        }
    }
    async fn connect(&self, _: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        Err(ProviderError::Unavailable)
    }
}
#[cfg(test)]
pub(super) fn set_task_fixture(languages: &LanguagePair) -> ProviderResult<serde_json::Value> {
    if !SOURCE_LANGUAGES.contains(&languages.source()) || languages.source() == languages.target() {
        return Err(ProviderError::UnsupportedLanguagePair);
    }
    let target = native_target_language(languages.target())?;
    Ok(serde_json::json!({
        "message_type":"set_task",
        "data": {
            "input_stream": {"content_type":"audio","source":{"type":"ws","format":"pcm_s16le","sample_rate":24000,"channels":1}},
            "output_stream": {"content_type":"audio","target":{"type":"ws","format":"pcm_s16le"}},
            "pipeline": {
                "transcription":{"source_language":languages.source()},
                "translations":[{"target_language":target,"translate_partial_transcriptions":true,"speech_generation":{}}],
                "allowed_message_types":["partial_transcription","validated_transcription","partial_translated_transcription","translated_transcription"]
            }
        }
    }))
}
#[cfg(test)]
pub(super) fn parse_fixture(
    bytes: &[u8],
    config: &ProviderSessionConfig,
    sequence: u64,
) -> ProviderResult<Vec<super::ProviderEvent>> {
    use super::{ProviderEvent, disabled::fixtures as f};
    use crate::domain::translation::{TimedTranslation, Translation};
    let value = f::object(bytes)?;
    let kind = f::string(&value["message_type"], 128)?;
    let data = match &value["data"] {
        serde_json::Value::String(encoded) => f::object(encoded.as_bytes())?,
        data if data.is_object() => data.clone(),
        _ => return Err(ProviderError::Protocol),
    };
    let mut metadata = f::metadata(config, kind);
    let event = match kind {
        "task_status"
            if data["command"] == "set_task"
                && data["task_status"] == "running"
                && matches!(
                    data["event"].as_str(),
                    Some("started" | "updated" | "unchanged" | "resumed")
                ) =>
        {
            ProviderEvent::SessionStarted { metadata }
        }
        "end_of_stream" => ProviderEvent::Finished { metadata },
        "partial_transcription"
        | "validated_transcription"
        | "partial_translated_transcription"
        | "translated_transcription" => {
            let translated = matches!(
                kind,
                "partial_translated_transcription" | "translated_transcription"
            );
            let finality = matches!(kind, "validated_transcription" | "translated_transcription");
            let transcription = &data["transcription"];
            validate_language(&transcription["language"], config, translated)?;
            metadata.sentence_id = Some(f::native_id(&transcription["transcription_id"])?);
            let (start, end) = f::times(transcription)?;
            metadata.timestamp = start;
            let text = TimedTranslation {
                translation: Translation {
                    text: f::text(&transcription["text"])?,
                    language: if translated {
                        config.languages.target()
                    } else {
                        config.languages.source()
                    }
                    .into(),
                },
                start,
                end,
            };
            f::subtitle(metadata, text, translated, finality)
        }
        "output_audio_data" => {
            validate_language(&data["language"], config, true)?;
            metadata.sentence_id = Some(f::native_id(&data["transcription_id"])?);
            f::audio(metadata, &data["data"], sequence)?
        }
        // Validation errors can reject one command while the task stays alive.
        // Keep their diagnostic bodies out of the normalized result and leave
        // lifecycle handling to a separately implemented live transport.
        "error" => return Err(ProviderError::Protocol),
        _ => return Ok(vec![]),
    };
    Ok(vec![event])
}

#[cfg(test)]
fn validate_language(
    value: &serde_json::Value,
    config: &ProviderSessionConfig,
    translated: bool,
) -> ProviderResult<()> {
    let expected = if translated {
        native_target_language(config.languages.target())?
    } else {
        config.languages.source()
    };
    if value.as_str() != Some(expected) {
        return Err(ProviderError::Protocol);
    }
    Ok(())
}
