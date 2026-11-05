use std::{collections::HashMap, time::Duration};

use prost::Message;

use crate::{
    audio::frame::PcmSpec,
    domain::session::SessionDirection,
    providers::{ProviderError, ProviderResult, ProviderSessionConfig},
};

// Official imports contain additional messages used by other ByteDance services.
#[allow(dead_code)]
mod wire {
    include!(concat!(env!("OUT_DIR"), "/volc_ast.rs"));
}

pub use wire::data::speech::{
    ast::{ReqParams, TranslateRequest, TranslateResponse},
    common::{Billing, BillingItem, RequestMeta, ResponseMeta},
    event::Type as EventType,
    understanding::{Audio, Corpus},
};

#[derive(Clone, Default)]
pub struct StartSessionOptions {
    pub speaker: Option<Speaker>,
    pub corpus: Option<Corpus>,
}

#[derive(Clone)]
pub struct Speaker {
    pub id: String,
    pub is_custom: bool,
    pub tts_resource_id: String,
}

/// Native wire samples; these must be normalized before creating AudioFrame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSampleFormat {
    Float32Le,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeOutputAudio {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: NativeSampleFormat,
}

pub const NATIVE_OUTPUT_AUDIO: NativeOutputAudio = NativeOutputAudio {
    sample_rate: 24_000,
    channels: 1,
    sample_format: NativeSampleFormat::Float32Le,
};

pub const INPUT_AUDIO: PcmSpec = PcmSpec::pcm16_mono(16_000);
pub const NORMALIZED_OUTPUT_AUDIO: PcmSpec = PcmSpec::pcm16_mono(24_000);
pub const NATIVE_INPUT_CHUNK_DURATION: Duration = Duration::from_millis(80);
/// 80 ms of 16 kHz mono PCM16; a final partial chunk is also accepted.
pub const MAX_INPUT_AUDIO_BYTES: usize = 2_560;

/// Config carries the already-directed language pair and application session ID.
/// Credentials belong in the WebSocket handshake, never in these parameters.
pub fn encode_start_session(
    config: &ProviderSessionConfig,
    options: &StartSessionOptions,
) -> ProviderResult<Vec<u8>> {
    validate_start_session(config, options)?;

    let mut params = ReqParams {
        mode: "s2s".into(),
        source_language: config.languages.source().into(),
        target_language: config.languages.target().into(),
        corpus: options.corpus.clone(),
        ..Default::default()
    };
    if let Some(speaker) = &options.speaker {
        params.speaker_id = speaker.id.clone();
        params.is_custom_speaker = speaker.is_custom;
        params.tts_resource_id = speaker.tts_resource_id.clone();
    }
    Ok(TranslateRequest {
        request_meta: Some(RequestMeta {
            session_id: config.session_id.to_string(),
            ..Default::default()
        }),
        event: EventType::StartSession as i32,
        source_audio: Some(Audio {
            // The service requires this container name even for headerless PCM.
            format: "wav".into(),
            codec: "raw".into(),
            rate: INPUT_AUDIO.sample_rate() as i32,
            bits: 16,
            channel: INPUT_AUDIO.channels() as i32,
            ..Default::default()
        }),
        target_audio: Some(Audio {
            format: "pcm".into(),
            rate: NATIVE_OUTPUT_AUDIO.sample_rate as i32,
            bits: match NATIVE_OUTPUT_AUDIO.sample_format {
                NativeSampleFormat::Float32Le => 32,
            },
            channel: NATIVE_OUTPUT_AUDIO.channels as i32,
            ..Default::default()
        }),
        request: Some(params),
        ..Default::default()
    }
    .encode_to_vec())
}

/// Headerless, little-endian PCM16 only. Chunking and sample conversion are
/// handled by the adapter before calling this protobuf boundary.
pub fn encode_task_request(pcm: &[u8]) -> ProviderResult<Vec<u8>> {
    if pcm.is_empty() || !pcm.len().is_multiple_of(2) || pcm.len() > MAX_INPUT_AUDIO_BYTES {
        return Err(ProviderError::InvalidAudioFrame);
    }
    Ok(TranslateRequest {
        event: EventType::TaskRequest as i32,
        source_audio: Some(Audio {
            binary_data: pcm.to_vec(),
            ..Default::default()
        }),
        ..Default::default()
    }
    .encode_to_vec())
}

pub fn encode_finish_session() -> Vec<u8> {
    TranslateRequest {
        event: EventType::FinishSession as i32,
        ..Default::default()
    }
    .encode_to_vec()
}

pub fn decode_request(bytes: &[u8]) -> ProviderResult<TranslateRequest> {
    TranslateRequest::decode(bytes).map_err(|_| ProviderError::Protocol)
}

/// Preserve native fields and unknown event numbers for the actor to classify.
/// This layer does not interpret text, status codes, timestamps, or audio bytes.
pub fn decode_response(bytes: &[u8]) -> ProviderResult<TranslateResponse> {
    TranslateResponse::decode(bytes).map_err(|_| ProviderError::Protocol)
}

fn validate_start_session(
    config: &ProviderSessionConfig,
    options: &StartSessionOptions,
) -> ProviderResult<()> {
    if config.session_id.is_nil() {
        return Err(ProviderError::Protocol);
    }
    if config.direction == SessionDirection::Bidirectional {
        return Err(ProviderError::InvalidDirection);
    }
    if config.target_audio != NORMALIZED_OUTPUT_AUDIO {
        return Err(ProviderError::UnsupportedAudioFormat);
    }
    if let Some(speaker) = &options.speaker {
        let known_speaker = matches!(
            speaker.id.as_str(),
            "zh_female_vv_uranus_bigtts" | "zh_male_jingqiangkanye_emo_mars_bigtts"
        );
        let known_resource = matches!(
            speaker.tts_resource_id.as_str(),
            "seed-tts-1.0" | "seed-tts-2.0" | "seed-icl-1.0" | "seed-icl-2.0"
        );
        if speaker.id.trim().is_empty() || (!speaker.is_custom && !known_speaker) || !known_resource
        {
            return Err(ProviderError::Protocol);
        }
    }

    const LANG_8: [&str; 8] = ["zh", "en", "de", "fr", "es", "id", "ja", "pt"];
    const LANG_20: [&str; 20] = [
        "zh", "en", "de", "fr", "es", "id", "ja", "pt", "ko", "tr", "ms", "nl", "ro", "pl", "cs",
        "ar", "th", "vi", "ru", "it",
    ];
    let source = config.languages.source();
    let target = config.languages.target();
    let valid_languages = if options.speaker.is_some() {
        LANG_20.contains(&source) && matches!(target, "zh" | "en")
    } else {
        LANG_8.contains(&source)
            && LANG_8.contains(&target)
            && (matches!(source, "zh" | "en") || matches!(target, "zh" | "en"))
    };
    if !valid_languages {
        return Err(ProviderError::UnsupportedLanguagePair);
    }
    if let Some(corpus) = &options.corpus {
        validate_corpus(corpus)?;
    }
    Ok(())
}

fn validate_corpus(corpus: &Corpus) -> ProviderResult<()> {
    let corrections: HashMap<String, String> = if corpus.correct_words.is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(&corpus.correct_words).map_err(|_| ProviderError::Protocol)?
    };
    if corpus.hot_words_list.len() + corpus.glossary_list.len() + corrections.len() > 1_000
        || corpus
            .hot_words_list
            .iter()
            .any(|word| word.trim().is_empty())
        || corpus
            .glossary_list
            .iter()
            .chain(corrections.iter())
            .any(|(source, target)| source.trim().is_empty() || target.trim().is_empty())
    {
        return Err(ProviderError::Protocol);
    }
    Ok(())
}
