use super::{
    audio::{AudioFrame, AudioSource},
    session::{LanguagePair, ProviderId, SessionConfig, SessionDirection},
};
use crate::error::AppError;

#[test]
fn session_config_round_trips_without_losing_direction() {
    let config = SessionConfig::call(LanguagePair::new("zh", "en").unwrap()).unwrap();
    let json = serde_json::to_string(&config).unwrap();
    let decoded: SessionConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.direction, SessionDirection::Bidirectional);
}

#[test]
fn audio_frame_rejects_non_16khz_shape() {
    let result = AudioFrame::new(AudioSource::Microphone, 1, 48_000, vec![0; 960]);

    assert!(matches!(result, Err(AppError::InvalidRequest(_))));
}

#[test]
fn language_codes_are_validated_and_normalized() {
    let pair = LanguagePair::new("ZH", "en").unwrap();

    assert_eq!(pair.source(), "zh");
    assert!(LanguagePair::new("english", "en").is_err());
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
