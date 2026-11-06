use super::*;
#[test]
fn credential_actions_have_explicit_tags_and_no_implicit_keep() {
    for json in [
        r#"{"action":"keep"}"#,
        r#"{"action":"clear"}"#,
        r#"{"action":"replace","api_key":"synthetic-only"}"#,
    ] {
        assert!(serde_json::from_str::<crate::app::CredentialChange>(json).is_ok());
    }
    assert!(serde_json::from_str::<crate::app::CredentialChange>("{}").is_err());
    assert!(
        serde_json::from_str::<crate::app::CredentialChange>(r#"{"action":"replace"}"#).is_err()
    );
    assert!(
        command_error(AppError::Configuration(
            "invalid provider credential".into()
        ))
        .starts_with("[CONFIGURATION_ERROR]")
    );
}

#[test]
fn command_wire_fixtures_pin_envelopes_mixed_audio_serde_and_safe_capabilities() {
    use crate::{
        audio::{
            PcmSpec,
            platform::{
                AudioDirection,
                inventory::{AudioBackendKind, EndpointRole},
            },
        },
        domain::{launch::RuntimeOutcome, session::LanguagePair},
    };
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/runtime-command-wire.json"
    ))
    .unwrap();
    let launch: SessionLaunch =
        serde_json::from_value(fixture["launchEnvelope"]["launch"].clone()).unwrap();
    assert_eq!(
        serde_json::json!({"launch":launch}),
        fixture["launchEnvelope"]
    );
    let request: SaveProviderRequest =
        serde_json::from_value(fixture["saveEnvelope"]["request"].clone()).unwrap();
    assert_eq!(request.config.id(), Uuid::from_u128(1));
    assert!(matches!(
        request.credential,
        crate::app::CredentialChange::Replace { .. }
    ));
    assert_eq!(
        fixture["idArguments"],
        serde_json::json!({"configId":Uuid::from_u128(1), "sessionId":Uuid::from_u128(2), "subscriberId":"opaque-window-token", "subscriptionId":Uuid::from_u128(3)})
    );
    assert_eq!(
        fixture["audioSerialization"],
        serde_json::json!({"driver":DriverStatus::Ready, "approval":DriverStatus::NeedsApproval("Approve the driver".into()), "failed":DriverStatus::Failed("Unavailable".into()), "direction":AudioDirection::Input, "role":EndpointRole::M1Render, "backend":AudioBackendKind::Pulse})
    );
    let capabilities = ProviderCapabilities {
        language_pairs: vec![LanguagePair::new("en", "zh").unwrap()],
        input_audio: PcmSpec::pcm16_mono(16_000),
        output_audio: PcmSpec::pcm16_mono(24_000),
        native_chunk_duration: std::time::Duration::from_millis(80),
        supports_tts: true,
        supports_bidirectional: true,
        ready: true,
        unavailable_reason: None,
    };
    assert_eq!(
        serde_json::to_value(capabilities).unwrap(),
        fixture["capabilities"]
    );
    for value in fixture["outcomes"].as_array().unwrap() {
        let outcome: RuntimeOutcome = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(outcome).unwrap(), *value);
    }
}
