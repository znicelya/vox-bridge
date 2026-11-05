//! Qwen-Audio realtime schema, pending translation-specific acceptance.
//!
//! docs/同声传译/阿里百炼/doc.md:49 says output_audio.language controls
//! best-effort TTS only and does not set the response text language. These
//! conversation events do not establish a verified translation capability.
use super::{
    ProviderCapabilities, ProviderError, ProviderResult, ProviderSessionConfig, ProviderStream,
    TranslationProvider,
};
use crate::{audio::PcmSpec, domain::session::ProviderId};
use async_trait::async_trait;
use std::time::Duration;

pub const ENDPOINT_TEMPLATE: &str =
    "wss://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/api-ws/v1/realtime?model={model}";
pub const CONFIG_FIELDS: &[&str] = &["WorkspaceId", "model", "API_KEY"];
pub const AUTH_FIELDS: &[&str] = &["API_KEY"];
pub const AUTHENTICATION: &str = "Authorization: Bearer <API_KEY>";
/// Raw signed 16-bit little-endian mono PCM, Base64 in JSON.
pub const WIRE_AUDIO_FORMAT: &str = "pcm";
pub struct AliyunProvider;
#[async_trait]
impl TranslationProvider for AliyunProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("aliyun").unwrap()
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            // The checked-in document does not establish guaranteed text
            // translation pairs; never offer an invented language matrix.
            language_pairs: vec![],
            input_audio: PcmSpec::pcm16_mono(16_000),
            output_audio: PcmSpec::pcm16_mono(24_000),
            // doc.md:1502 recommends 100 ms; this is not a server maximum.
            native_chunk_duration: Duration::from_millis(100),
            supports_tts: true,
            supports_bidirectional: false,
            ready: false,
            unavailable_reason: Some("未联调：Qwen-Audio 文档描述语音对话；目标语种仅影响 TTS，尚未验证同传文本与双向通话".into()),
        }
    }
    async fn connect(&self, _: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        Err(ProviderError::Unavailable)
    }
}

/// Exercises wire normalization only. Target text is labeled with the requested
/// language for the fixture; this is not proof that Qwen obeys that language.
#[cfg(test)]
pub(super) fn parse_fixture(
    bytes: &[u8],
    config: &ProviderSessionConfig,
    sequence: u64,
) -> ProviderResult<Vec<super::ProviderEvent>> {
    use super::{ProviderEvent, disabled::fixtures as f};
    use crate::domain::translation::{TimedTranslation, Translation};
    let value = f::object(bytes)?;
    let kind = f::string(&value["type"], 128)?;
    let metadata = f::metadata(config, kind);
    let event = match kind {
        // SessionCreated precedes the mandatory configuration step. Only the
        // applied configuration acknowledgement is represented as started.
        "session.updated" => ProviderEvent::SessionStarted { metadata },
        "conversation.item.input_audio_transcription.completed"
        | "response.audio_transcript.done" => {
            let translated = kind == "response.audio_transcript.done";
            let text = TimedTranslation {
                translation: Translation {
                    text: f::text(&value["transcript"])?,
                    language: if translated {
                        config.languages.target()
                    } else {
                        config.languages.source()
                    }
                    .into(),
                },
                start: None,
                end: None,
            };
            f::subtitle(metadata, text, translated, true)
        }
        "response.audio.delta" => f::audio(metadata, &value["delta"], sequence)?,
        // A rejected command need not terminate the connection. Return a safe
        // parse result without inventing a terminal session event; a future
        // transport must apply the service's separate error/lifecycle rules.
        "error" => return Err(ProviderError::Protocol),
        // response.done completes one model turn, not the WebSocket session.
        _ => return Ok(vec![]),
    };
    Ok(vec![event])
}
