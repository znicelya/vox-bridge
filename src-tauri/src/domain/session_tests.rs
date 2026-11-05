use super::{
    audio::{AudioFrame, SAMPLE_RATE_HZ},
    session::{LanguagePair, ProviderId, SessionConfig, SessionDirection},
};
use crate::error::AppError;

#[test]
fn session_config_round_trips_without_losing_direction() {
    let config = SessionConfig::call(LanguagePair::new("zh", "en").unwrap()).unwrap();
    let json = serde_json::to_string(&config).unwrap();
    let decoded: SessionConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.direction(), SessionDirection::Bidirectional);
}

#[test]
fn session_config_deserialization_rejects_invalid_direction_for_mode() {
    let invalid_live =
        r#"{"mode":"live","direction":"bidirectional","languages":{"source":"zh","target":"en"}}"#;
    let invalid_call = r#"{"mode":"call","direction":"source_to_target","languages":{"source":"zh","target":"en"}}"#;

    assert!(serde_json::from_str::<SessionConfig>(invalid_live).is_err());
    assert!(serde_json::from_str::<SessionConfig>(invalid_call).is_err());
}

#[test]
fn deserialization_rejects_invalid_language_and_provider_ids() {
    assert!(serde_json::from_str::<LanguagePair>(r#"{"source":"eng","target":"en"}"#).is_err());
    assert!(serde_json::from_str::<ProviderId>("\"  \"").is_err());
}

#[test]
fn audio_frame_rejects_non_mono_shape() {
    let result = crate::audio::PcmSpec::new(16_000, 2, crate::audio::SampleFormat::I16);

    assert!(matches!(result, Err(AppError::InvalidRequest(_))));
    assert_eq!(SAMPLE_RATE_HZ, 16_000);
}

#[test]
fn language_codes_are_validated_and_normalized() {
    let pair = LanguagePair::new("ZH", "en").unwrap();

    assert_eq!(pair.source(), "zh");
    assert!(LanguagePair::new("english", "en").is_err());
}

#[test]
fn audio_frame_deserialization_rejects_empty_samples() {
    let invalid = r#"{"source":"microphone","sequence":1,"captured_at":{"secs_since_epoch":1,"nanos_since_epoch":0},"samples":[]}"#;
    assert!(serde_json::from_str::<AudioFrame>(invalid).is_err());
}

#[test]
fn live_sessions_reject_bidirectional_direction() {
    let pair = LanguagePair::new("zh", "en").unwrap();

    assert!(SessionConfig::live(pair, SessionDirection::Bidirectional).is_err());
}

#[test]
fn provider_id_must_not_be_empty() {
    assert!(ProviderId::new("  ").is_err());
    assert_eq!(ProviderId::new("openai").unwrap().as_str(), "openai");
}
