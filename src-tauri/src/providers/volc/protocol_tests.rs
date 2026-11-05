use prost::Message;
use uuid::Uuid;

use super::protocol::*;
use crate::{
    audio::frame::PcmSpec,
    domain::session::{LanguagePair, SessionDirection, SessionMode},
    providers::{ProviderError, ProviderSessionConfig},
};

fn config() -> ProviderSessionConfig {
    ProviderSessionConfig {
        session_id: Uuid::parse_str("00112233-4455-4677-8899-aabbccddeeff").unwrap(),
        direction: SessionDirection::SourceToTarget,
        mode: SessionMode::Live,
        languages: LanguagePair::new("zh", "en").unwrap(),
        target_audio: PcmSpec::pcm16_mono(24_000),
        provider_config_id: Uuid::new_v4(),
    }
}

#[test]
fn volc_start_session_uses_event_100_and_documented_native_audio() {
    let bytes = encode_start_session(&config(), &StartSessionOptions::default()).unwrap();
    let message = TranslateRequest::decode(bytes.as_slice()).unwrap();
    assert_eq!(message.event, 100);
    let meta = message.request_meta.unwrap();
    assert_eq!(meta.session_id, "00112233-4455-4677-8899-aabbccddeeff");
    assert!(Uuid::parse_str(&meta.session_id).is_ok());
    let source = message.source_audio.unwrap();
    assert_eq!(source.format, "wav");
    assert_eq!(source.codec, "raw");
    assert_eq!((source.rate, source.bits, source.channel), (16_000, 16, 1));
    assert!(source.data.is_empty());
    assert!(source.binary_data.is_empty());
    let target = message.target_audio.unwrap();
    assert_eq!(target.format, "pcm");
    assert_eq!((target.rate, target.bits, target.channel), (24_000, 32, 1));
    assert_eq!(NATIVE_OUTPUT_AUDIO.sample_rate, target.rate as u32);
    assert_eq!(NATIVE_OUTPUT_AUDIO.channels, target.channel as u16);
    assert_eq!(
        NATIVE_OUTPUT_AUDIO.sample_format,
        NativeSampleFormat::Float32Le
    );
    let params = message.request.unwrap();
    assert_eq!(params.mode, "s2s");
    assert_eq!(params.source_language, "zh");
    assert_eq!(params.target_language, "en");
    assert!(params.speaker_id.is_empty());
    assert!(params.corpus.is_none());
}

#[test]
fn start_session_preserves_optional_speaker_and_corpus_wire_fields() {
    let options = StartSessionOptions {
        speaker: Some(Speaker {
            id: "zh_female_vv_uranus_bigtts".into(),
            is_custom: false,
            tts_resource_id: "seed-tts-2.0".into(),
        }),
        corpus: Some(Corpus {
            hot_words_list: vec!["Vox Bridge".into()],
            boosting_table_id: "hot-id".into(),
            boosting_table_name: "hot-name".into(),
            correct_words: r#"{"Accept":"Receive"}"#.into(),
            regex_correct_table_id: "correct-id".into(),
            regex_correct_table_name: "correct-name".into(),
            glossary_list: [("人工智能".into(), "AI".into())].into(),
            glossary_table_id: "glossary-id".into(),
            glossary_table_name: "glossary-name".into(),
            ..Default::default()
        }),
    };
    let bytes = encode_start_session(&config(), &options).unwrap();
    let params = TranslateRequest::decode(bytes.as_slice())
        .unwrap()
        .request
        .unwrap();
    assert_eq!(params.speaker_id, "zh_female_vv_uranus_bigtts");
    assert!(!params.is_custom_speaker);
    assert_eq!(params.tts_resource_id, "seed-tts-2.0");
    assert_eq!(params.corpus, options.corpus);
}

#[test]
fn start_session_rejects_nil_uuid_and_non_stream_direction() {
    let mut config = config();
    config.session_id = Uuid::nil();
    assert_eq!(
        encode_start_session(&config, &StartSessionOptions::default()),
        Err(ProviderError::Protocol)
    );
    config.session_id = Uuid::new_v4();
    config.direction = SessionDirection::Bidirectional;
    assert_eq!(
        encode_start_session(&config, &StartSessionOptions::default()),
        Err(ProviderError::InvalidDirection)
    );
}

#[test]
fn start_session_requires_normalized_24k_pcm16_output() {
    let mut config = config();
    config.target_audio = PcmSpec::pcm16_mono(16_000);
    assert_eq!(
        encode_start_session(&config, &StartSessionOptions::default()),
        Err(ProviderError::UnsupportedAudioFormat)
    );
}

#[test]
fn start_session_enforces_s2s_language_constraints() {
    let options = StartSessionOptions::default();
    for (source, target, valid) in [
        ("zh", "ja", true),
        ("de", "en", true),
        ("fr", "de", false),
        ("ko", "en", false),
        ("zz", "en", false),
    ] {
        let mut config = config();
        config.languages = LanguagePair::new(source, target).unwrap();
        let result = encode_start_session(&config, &options);
        if valid {
            assert!(result.is_ok(), "{source} -> {target}");
        } else {
            assert_eq!(result, Err(ProviderError::UnsupportedLanguagePair));
        }
    }
    let options = StartSessionOptions {
        speaker: Some(Speaker {
            id: "zh_male_jingqiangkanye_emo_mars_bigtts".into(),
            is_custom: false,
            tts_resource_id: "seed-tts-1.0".into(),
        }),
        ..Default::default()
    };
    for (source, target, valid) in [("ko", "en", true), ("zh", "ja", false)] {
        let mut config = config();
        config.languages = LanguagePair::new(source, target).unwrap();
        assert_eq!(encode_start_session(&config, &options).is_ok(), valid);
    }
}

#[test]
fn start_session_validates_external_speaker_metadata() {
    for (id, is_custom, resource, valid) in [
        ("zh_female_vv_uranus_bigtts", false, "", false),
        ("zh_female_vv_uranus_bigtts", false, "unknown", false),
        ("unknown", false, "seed-tts-1.0", false),
        (" ", true, "seed-icl-1.0", false),
        ("custom-speaker", true, "seed-icl-2.0", true),
    ] {
        let options = StartSessionOptions {
            speaker: Some(Speaker {
                id: id.into(),
                is_custom,
                tts_resource_id: resource.into(),
            }),
            ..Default::default()
        };
        let result = encode_start_session(&config(), &options);
        if valid {
            let request = TranslateRequest::decode(result.unwrap().as_slice())
                .unwrap()
                .request
                .unwrap();
            assert!(request.is_custom_speaker);
            assert_eq!(request.speaker_id, "custom-speaker");
        } else {
            assert_eq!(result, Err(ProviderError::Protocol));
        }
    }
}

#[test]
fn start_session_rejects_malformed_corpus_and_limits_combined_entries() {
    for correct_words in ["not-json", "[]", r#"{"a":3}"#, r#"{"":"b"}"#] {
        let options = StartSessionOptions {
            corpus: Some(Corpus {
                correct_words: correct_words.into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            encode_start_session(&config(), &options),
            Err(ProviderError::Protocol)
        );
    }
    let mut options = StartSessionOptions {
        corpus: Some(Corpus {
            hot_words_list: vec!["hot".into(); 998],
            glossary_list: [("term".into(), "translation".into())].into(),
            correct_words: r#"{"wrong":"right"}"#.into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(encode_start_session(&config(), &options).is_ok());
    options
        .corpus
        .as_mut()
        .unwrap()
        .hot_words_list
        .push("too many".into());
    assert_eq!(
        encode_start_session(&config(), &options),
        Err(ProviderError::Protocol)
    );
    options.corpus = Some(Corpus {
        hot_words_list: vec![" ".into()],
        ..Default::default()
    });
    assert_eq!(
        encode_start_session(&config(), &options),
        Err(ProviderError::Protocol)
    );
}

#[test]
fn task_request_preserves_non_utf8_pcm_at_official_wire_tag_14() {
    // TranslateRequest.event tag 2 = 200; source_audio tag 4 contains only
    // Audio.binary_data tag 14. Samples are i16::MIN and i16::MAX, LE.
    let expected = [
        0x10, 0xc8, 0x01, 0x22, 0x06, 0x72, 0x04, 0x00, 0x80, 0xff, 0x7f,
    ];
    let bytes = encode_task_request(&[0x00, 0x80, 0xff, 0x7f]).unwrap();
    assert_eq!(bytes, expected);
    let request = decode_request(&expected).unwrap();
    assert_eq!(request.event, 200);
    assert_eq!(
        request.source_audio.unwrap().binary_data,
        [0x00, 0x80, 0xff, 0x7f]
    );
    assert!(request.request_meta.is_none());
    assert!(request.request.is_none());
    assert!(request.target_audio.is_none());
}

#[test]
fn task_request_rejects_empty_partial_and_oversized_pcm() {
    for bytes in [vec![], vec![0; 1], vec![0; 2_562]] {
        assert_eq!(
            encode_task_request(&bytes),
            Err(ProviderError::InvalidAudioFrame)
        );
    }
    assert!(encode_task_request(&vec![0; 2_560]).is_ok());
}

#[test]
fn finish_session_encodes_only_event_102() {
    assert_eq!(encode_finish_session(), [0x10, 0x66]);
    assert_eq!(
        decode_request(&[0x10, 0x66]).unwrap(),
        TranslateRequest {
            event: 102,
            ..Default::default()
        }
    );
}

#[test]
fn response_decoder_accepts_documented_event_numbers() {
    // Hand-authored protobuf tag 2 + varint event values from the official docs.
    for (bytes, event) in [
        ([0x10, 0x96, 0x01], EventType::SessionStarted),
        ([0x10, 0x98, 0x01], EventType::SessionFinished),
        ([0x10, 0x99, 0x01], EventType::SessionFailed),
        ([0x10, 0x9a, 0x01], EventType::UsageResponse),
        ([0x10, 0xfa, 0x01], EventType::AudioMuted),
        ([0x10, 0xde, 0x02], EventType::TtsSentenceStart),
        ([0x10, 0xdf, 0x02], EventType::TtsSentenceEnd),
        ([0x10, 0xe0, 0x02], EventType::TtsResponse),
        ([0x10, 0x8a, 0x05], EventType::SourceSubtitleStart),
        ([0x10, 0x8b, 0x05], EventType::SourceSubtitleResponse),
        ([0x10, 0x8c, 0x05], EventType::SourceSubtitleEnd),
        ([0x10, 0x8d, 0x05], EventType::TranslationSubtitleStart),
        ([0x10, 0x8e, 0x05], EventType::TranslationSubtitleResponse),
        ([0x10, 0x8f, 0x05], EventType::TranslationSubtitleEnd),
    ] {
        assert_eq!(decode_response(&bytes).unwrap().event, event as i32);
    }
}

#[test]
fn response_decoder_preserves_trailing_tts_audio_and_subtitle_metadata() {
    let trailing_audio = [
        0x10, 0xdf, 0x02, 0x1a, 0x04, 0x00, 0x00, 0x00, 0x3f, 0x28, 0x50, 0x30, 0xa0, 0x01,
    ];
    let response = decode_response(&trailing_audio).unwrap();
    assert_eq!(response.event, 351);
    assert_eq!(response.data, [0x00, 0x00, 0x00, 0x3f]);
    assert_eq!((response.start_time, response.end_time), (80, 160));
    let subtitle = [
        0x10, 0x8c, 0x05, 0x22, 0x02, b'h', b'i', 0x28, 0x50, 0x30, 0xa0, 0x01, 0x38, 0x01, 0x52,
        0x02, b'e', b'n',
    ];
    let response = decode_response(&subtitle).unwrap();
    assert_eq!(response.text, "hi");
    assert_eq!((response.start_time, response.end_time), (80, 160));
    assert!(response.spk_chg);
    assert_eq!(response.detected_language.as_deref(), Some("en"));
}

#[test]
fn response_decoder_retains_billing_muted_and_failure_details() {
    // response_meta field 1 contains billing field 5, with duration_msec=80.
    let usage = decode_response(&[0x0a, 0x04, 0x2a, 0x02, 0x10, 0x50, 0x10, 0x9a, 0x01]).unwrap();
    assert_eq!(
        usage.response_meta.unwrap().billing.unwrap().duration_msec,
        80
    );
    let muted = decode_response(&[0x10, 0xfa, 0x01, 0x40, 0x50]).unwrap();
    assert_eq!(muted.muted_duration_ms, 80);
    let failure = decode_response(&[
        0x0a, 0x07, 0x18, 0x07, 0x22, 0x03, b'b', b'a', b'd', 0x10, 0x99, 0x01,
    ])
    .unwrap();
    let meta = failure.response_meta.unwrap();
    assert_eq!(meta.status_code, 7);
    assert_eq!(meta.message, "bad");
}

#[test]
fn malformed_protobuf_is_a_payload_free_protocol_error() {
    for bytes in [
        &[0x10, 0x80][..],
        &[0x22, 0x08, b's', b'e', b'c', b'r', b'e', b't'][..],
    ] {
        assert_eq!(decode_request(bytes).unwrap_err(), ProviderError::Protocol);
        assert_eq!(decode_response(bytes).unwrap_err(), ProviderError::Protocol);
    }
}
