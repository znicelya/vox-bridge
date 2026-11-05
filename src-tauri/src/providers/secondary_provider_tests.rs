use super::{
    aliyun::{self, AliyunProvider},
    disabled::DisabledProvider,
    palabra::{self, PalabraProvider},
    *,
};
use crate::{
    audio::PcmSpec,
    domain::session::{LanguagePair, ProviderId, SessionDirection, SessionMode},
};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

fn config() -> ProviderSessionConfig {
    ProviderSessionConfig {
        session_id: Uuid::new_v4(),
        direction: SessionDirection::TargetToSource,
        mode: SessionMode::Live,
        languages: LanguagePair::new("en", "zh").unwrap(),
        target_audio: PcmSpec::pcm16_mono(24_000),
        provider_config_id: Uuid::new_v4(),
    }
}

#[tokio::test]
async fn secondary_and_unknown_providers_never_open_a_live_stream() {
    let providers: Vec<Box<dyn TranslationProvider>> = vec![
        Box::new(AliyunProvider),
        Box::new(PalabraProvider),
        Box::new(DisabledProvider::new(
            ProviderId::new("unrecognized").unwrap(),
        )),
    ];
    for provider in providers {
        let caps = provider.capabilities();
        assert!(!caps.ready);
        assert!(caps.unavailable_reason.unwrap().contains("未联调"));
        assert!(matches!(
            provider.connect(config()).await,
            Err(ProviderError::Unavailable)
        ));
    }
}

#[test]
fn capabilities_distinguish_native_audio_and_unverified_translation() {
    let aliyun = AliyunProvider.capabilities();
    assert_eq!(aliyun.input_audio, PcmSpec::pcm16_mono(16_000));
    assert_eq!(aliyun.output_audio, PcmSpec::pcm16_mono(24_000));
    assert_eq!(aliyun.native_chunk_duration, Duration::from_millis(100));
    assert!(aliyun.supports_tts);
    assert!(aliyun.language_pairs.is_empty());
    assert!(!aliyun.supports_bidirectional);
    let palabra = PalabraProvider.capabilities();
    assert_eq!(palabra.input_audio, PcmSpec::pcm16_mono(24_000));
    assert_eq!(palabra.output_audio, PcmSpec::pcm16_mono(24_000));
    assert_eq!(palabra.native_chunk_duration, Duration::from_millis(320));
    assert!(palabra.supports_tts);
    assert!(
        palabra
            .language_pairs
            .contains(&LanguagePair::new("en", "az").unwrap())
    );
    assert!(
        !palabra
            .language_pairs
            .contains(&LanguagePair::new("az", "en").unwrap())
    );
    assert!(
        palabra
            .language_pairs
            .contains(&LanguagePair::new("eu", "en").unwrap())
    );
    assert!(
        !palabra
            .language_pairs
            .contains(&LanguagePair::new("en", "eu").unwrap())
    );
}

#[test]
fn palabra_task_uses_native_chinese_target_and_all_subtitle_types() {
    let task = palabra::set_task_fixture(&LanguagePair::new("en", "zh").unwrap()).unwrap();
    assert_eq!(task["message_type"], "set_task");
    assert_eq!(
        task["data"]["pipeline"]["translations"][0]["target_language"],
        "zh-hans"
    );
    assert_eq!(
        task["data"]["pipeline"]["transcription"]["source_language"],
        "en"
    );
    assert_eq!(
        task["data"]["input_stream"]["source"],
        json!({"type":"ws","format":"pcm_s16le","sample_rate":24000,"channels":1})
    );
    assert_eq!(
        task["data"]["output_stream"]["target"]["format"],
        "pcm_s16le"
    );
    assert_eq!(
        task["data"]["pipeline"]["allowed_message_types"],
        json!([
            "partial_transcription",
            "validated_transcription",
            "partial_translated_transcription",
            "translated_transcription"
        ])
    );
    assert!(matches!(
        palabra::set_task_fixture(&LanguagePair::new("az", "en").unwrap()),
        Err(ProviderError::UnsupportedLanguagePair)
    ));
}

#[test]
fn aliyun_maps_documented_finals_and_pcm_without_claiming_turn_end_is_session_end() {
    let cfg = config();
    let source = aliyun::parse_fixture(
        br#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"Hello"}"#,
        &cfg,
        1,
    )
    .unwrap();
    assert!(
        matches!(&source[..], [ProviderEvent::SourceFinal { text, .. }] if text.translation.text == "Hello" && text.translation.language == "en")
    );
    let target = aliyun::parse_fixture(
        br#"{"type":"response.audio_transcript.done","transcript":"Ni hao"}"#,
        &cfg,
        2,
    )
    .unwrap();
    assert!(
        matches!(&target[..], [ProviderEvent::TranslationFinal { text, .. }] if text.translation.text == "Ni hao")
    );
    let audio = aliyun::parse_fixture(
        br#"{"type":"response.audio.delta","delta":"AID/f/8/"}"#,
        &cfg,
        3,
    )
    .unwrap();
    assert!(
        matches!(&audio[..], [ProviderEvent::Audio { frame, .. }] if frame.samples() == [-32768, 32767, 16383] && frame.sequence() == 3 && frame.spec() == PcmSpec::pcm16_mono(24_000))
    );
    assert!(
        aliyun::parse_fixture(
            br#"{"type":"response.done","response":{"status":"completed"}}"#,
            &cfg,
            4
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        aliyun::parse_fixture(br#"{"type":"session.created"}"#, &cfg, 4)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        &aliyun::parse_fixture(br#"{"type":"session.updated"}"#, &cfg, 4).unwrap()[..],
        [ProviderEvent::SessionStarted { .. }]
    ));
}

#[test]
fn palabra_only_set_task_running_acknowledges_start() {
    for event in ["started", "updated", "unchanged", "resumed"] {
        let bytes = serde_json::to_vec(&json!({"message_type":"task_status","data":{"command":"set_task","event":event,"task_status":"running"}})).unwrap();
        assert!(matches!(
            &palabra::parse_fixture(&bytes, &config(), 0).unwrap()[..],
            [ProviderEvent::SessionStarted { .. }]
        ));
    }
    for data in [
        json!({"command":"pause_task","event":"paused","task_status":"paused"}),
        json!({"command":"set_task","event":"started","task_status":"paused"}),
        json!({"command":"interrupt_task","event":"interrupted","task_status":"running"}),
    ] {
        let bytes = serde_json::to_vec(&json!({"message_type":"task_status","data":data})).unwrap();
        assert!(
            palabra::parse_fixture(&bytes, &config(), 0)
                .unwrap()
                .is_empty()
        );
    }
    assert!(matches!(
        &palabra::parse_fixture(
            br#"{"message_type":"end_of_stream","data":{}}"#,
            &config(),
            0
        )
        .unwrap()[..],
        [ProviderEvent::Finished { .. }]
    ));
}

#[test]
fn palabra_subtitles_preserve_native_correlation_direction_and_relative_seconds() {
    let cfg = config();
    for (kind, translated, finality) in [
        ("partial_transcription", false, false),
        ("validated_transcription", false, true),
        ("partial_translated_transcription", true, false),
        ("translated_transcription", true, true),
    ] {
        let payload = json!({"transcription":{"transcription_id":"native-7","translation_part_id":0,"language":if translated {"zh-hans"} else {"en"},"text":"fixture","segments":[{"start":7.36,"end":8.74,"start_timestamp":1784642133.39,"end_timestamp":1784642134.77}]}});
        // Palabra can deliver the inner data as encoded JSON as well as an object.
        let data = if finality {
            json!(payload.to_string())
        } else {
            payload
        };
        let bytes = serde_json::to_vec(&json!({"message_type":kind,"data":data})).unwrap();
        let events = palabra::parse_fixture(&bytes, &cfg, 4).unwrap();
        let event = &events[0];
        assert_eq!(event.metadata().session_id, cfg.session_id);
        assert_eq!(event.metadata().direction, SessionDirection::TargetToSource);
        assert_eq!(event.metadata().provider_event_type.as_deref(), Some(kind));
        assert_eq!(event.metadata().sentence_id.as_deref(), Some("native-7"));
        assert_eq!(event.metadata().local_correlation_id, None);
        let text = match event {
            ProviderEvent::SourceInterim { text, .. } if !translated && !finality => text,
            ProviderEvent::SourceFinal { text, .. } if !translated && finality => text,
            ProviderEvent::TranslationInterim { text, .. } if translated && !finality => text,
            ProviderEvent::TranslationFinal { text, .. } if translated && finality => text,
            _ => panic!("wrong subtitle variant"),
        };
        assert_eq!(
            text.translation.language,
            if translated { "zh" } else { "en" }
        );
        assert!((text.start.unwrap().as_secs_f64() - 7.36).abs() < 0.000_001);
        assert!((text.end.unwrap().as_secs_f64() - 8.74).abs() < 0.000_001);
    }
}

#[test]
fn palabra_audio_retains_native_transcription_id_and_decodes_pcm16() {
    let events = palabra::parse_fixture(br#"{"message_type":"output_audio_data","data":{"transcription_id":"native-7","translation_part_id":"0","language":"zh-hans","last_chunk":true,"data":"AID/f/8/","chunk_generation_delta":120}}"#, &config(), 19).unwrap();
    assert!(
        matches!(&events[..], [ProviderEvent::Audio { metadata, frame }] if metadata.sentence_id.as_deref() == Some("native-7") && frame.samples() == [-32768,32767,16383] && frame.sequence() == 19)
    );
}

#[test]
fn fixture_parsers_reject_malformed_pcm_and_oversized_messages() {
    for audio in ["not-base64!", "AA==", "", "AQ"] {
        let ali =
            serde_json::to_vec(&json!({"type":"response.audio.delta","delta":audio})).unwrap();
        let pal = serde_json::to_vec(&json!({"message_type":"output_audio_data","data":{"data":audio,"language":"zh-hans","transcription_id":"n"}})).unwrap();
        assert!(matches!(
            aliyun::parse_fixture(&ali, &config(), 0),
            Err(ProviderError::Protocol)
        ));
        assert!(matches!(
            palabra::parse_fixture(&pal, &config(), 0),
            Err(ProviderError::Protocol)
        ));
    }
    for invalid in [b"{".to_vec(), vec![b' '; 800 * 1024]] {
        assert!(matches!(
            aliyun::parse_fixture(&invalid, &config(), 0),
            Err(ProviderError::Protocol)
        ));
        assert!(matches!(
            palabra::parse_fixture(&invalid, &config(), 0),
            Err(ProviderError::Protocol)
        ));
    }
    let audio = "A".repeat(524_292);
    let bytes=serde_json::to_vec(&json!({"message_type":"output_audio_data","data":{"data":audio,"language":"zh-hans","transcription_id":"n"}})).unwrap();
    assert!(matches!(
        palabra::parse_fixture(&bytes, &config(), 0),
        Err(ProviderError::Protocol)
    ));
}

#[test]
fn palabra_rejects_invalid_times_and_unrequested_language() {
    for (start, end, language) in [
        (-1.0, 1.0, "zh-hans"),
        (2.0, 1.0, "zh-hans"),
        (0.0, 1.0, "fr"),
        (1e100, 1e101, "zh-hans"),
    ] {
        let bytes=serde_json::to_vec(&json!({"message_type":"translated_transcription","data":{"transcription":{"transcription_id":"n","language":language,"text":"x","segments":[{"start":start,"end":end}]}}})).unwrap();
        assert!(matches!(
            palabra::parse_fixture(&bytes, &config(), 0),
            Err(ProviderError::Protocol)
        ));
    }
}

#[test]
fn palabra_keeps_the_full_time_range_when_native_segments_overlap() {
    let bytes = serde_json::to_vec(&json!({
        "message_type":"validated_transcription",
        "data":{"transcription":{"transcription_id":"native-7","language":"en","text":"fixture",
            "segments":[{"start":8.0,"end":9.5},{"start":7.36,"end":8.74},{"start":9.0,"end":9.6}]}}
    }))
    .unwrap();
    let events = palabra::parse_fixture(&bytes, &config(), 1).unwrap();
    let ProviderEvent::SourceFinal { text, .. } = &events[0] else {
        panic!("wrong subtitle variant");
    };
    assert!((text.start.unwrap().as_secs_f64() - 7.36).abs() < 0.000_001);
    assert!((text.end.unwrap().as_secs_f64() - 9.6).abs() < 0.000_001);
}

#[test]
fn remote_errors_are_payload_free_without_inventing_session_termination() {
    let secret = "SECRET_TRANSCRIPT_DO_NOT_ECHO";
    let ali = serde_json::to_vec(
        &json!({"type":"error","error":{"type":"invalid_request_error","message":secret}}),
    )
    .unwrap();
    let pal = serde_json::to_vec(
        &json!({"message_type":"error","data":{"code":"VALIDATION_ERROR","desc":secret}}),
    )
    .unwrap();
    for result in [
        aliyun::parse_fixture(&ali, &config(), 0),
        palabra::parse_fixture(&pal, &config(), 0),
    ] {
        assert!(matches!(result, Err(ProviderError::Protocol)));
        assert!(!format!("{result:?}").contains(secret));
    }
}

#[test]
fn fixture_payload_boundaries_reject_invalid_ids_text_and_nested_data() {
    let too_long = "x".repeat(65_537);
    let ali =
        serde_json::to_vec(&json!({"type":"response.audio_transcript.done","transcript":too_long}))
            .unwrap();
    assert!(matches!(
        aliyun::parse_fixture(&ali, &config(), 0),
        Err(ProviderError::Protocol)
    ));
    for (id, text) in [
        ("n".to_string(), too_long),
        ("\n".to_string(), "x".to_string()),
        ("x".repeat(257), "x".to_string()),
    ] {
        let pal=serde_json::to_vec(&json!({"message_type":"validated_transcription","data":{"transcription":{"transcription_id":id,"language":"en","text":text}}})).unwrap();
        assert!(matches!(
            palabra::parse_fixture(&pal, &config(), 0),
            Err(ProviderError::Protocol)
        ));
    }
    for data in [json!("[]"), json!("{invalid}"), json!(null)] {
        let bytes = serde_json::to_vec(&json!({"message_type":"task_status","data":data})).unwrap();
        assert!(matches!(
            palabra::parse_fixture(&bytes, &config(), 0),
            Err(ProviderError::Protocol)
        ));
    }
}
