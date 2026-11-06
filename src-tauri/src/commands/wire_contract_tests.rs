//! Shared, synthetic, audio-free producer/consumer fixtures. Build actual public DTOs;
//! do not deserialize the expected JSON and serialize it back as the assertion.
use std::time::{Duration, UNIX_EPOCH};

use crate::{
    app::{LaunchValidation, ProviderConfigView},
    audio::{
        frame::{PcmSpec, SampleFormat},
        platform::{
            AudioDirection, AudioEndpoint, DriverStatus,
            inventory::{AudioBackendKind, AudioInventory, EndpointRole, RuntimeEndpoint},
        },
    },
    commands::subscriptions::SubscriptionTicket,
    config::{EmbeddingConfig, ProviderConfig, TavilyConfig},
    domain::{
        launch::{
            DeviceSelection, DirectionPhase, DirectionSnapshot, RuntimeOutcome, RuntimePhase,
            RuntimeSnapshot, SessionContext, SessionLaunch,
        },
        session::{LanguagePair, ProviderId, SessionConfig, SessionDirection, SessionMode},
        transcript::*,
    },
    events::{AudioSessionState, SessionEvent},
    export::ExportFormat,
    providers::ProviderCapabilities,
    session::{SessionRecord, SessionStatus},
};
use serde_json::{Value, json};
use uuid::Uuid;

fn id(n: u32) -> Uuid {
    Uuid::parse_str(&format!("00000000-0000-0000-0000-{n:012}")).unwrap()
}
fn languages() -> LanguagePair {
    LanguagePair::new("en", "zh").unwrap()
}
fn endpoints() -> Vec<RuntimeEndpoint> {
    [
        (
            "input",
            "Microphone",
            AudioDirection::Input,
            EndpointRole::Input,
            AudioBackendKind::Cpal,
        ),
        (
            "m1",
            "VoxBridge Input",
            AudioDirection::Output,
            EndpointRole::M1Render,
            AudioBackendKind::Pulse,
        ),
        (
            "m2",
            "VoxBridge Output",
            AudioDirection::Input,
            EndpointRole::M2Capture,
            AudioBackendKind::Pulse,
        ),
        (
            "physical",
            "Speakers",
            AudioDirection::Output,
            EndpointRole::PhysicalOutput,
            AudioBackendKind::Cpal,
        ),
    ]
    .into_iter()
    .map(
        |(endpoint_id, display_name, direction, role, backend)| RuntimeEndpoint {
            endpoint: AudioEndpoint {
                id: endpoint_id.into(),
                display_name: display_name.into(),
                direction,
                sample_rates: vec![16000, 24000, 48000],
                channels: 1,
                ready: true,
            },
            role,
            backend,
        },
    )
    .collect()
}
fn transcript(n: u32, kind: TranscriptKind, is_final: bool) -> TranscriptUpdate {
    let source = kind == TranscriptKind::Source;
    TranscriptUpdate {
        id: id(n),
        session_id: id(2),
        direction: SessionDirection::TargetToSource,
        sequence: (n - 9).into(),
        occurred_at_ms: 1_730_000_000_123,
        language: if source { "zh" } else { "en" }.into(),
        text: if source { "泵" } else { "pump" }.into(),
        is_final,
        details: Some(TranscriptDetails {
            kind,
            provider: ProviderId::new("volcengine").unwrap(),
            provider_config_id: id(1),
            stream_epoch: 2,
            provider_event_type: Some(if source { "650" } else { "652" }.into()),
            provider_timestamp_ms: None,
            sentence_id: None,
            local_correlation_id: Some(7),
            log_id: None,
            start_ms: None,
            end_ms: None,
        }),
    }
}
fn diagnostics() -> Vec<RuntimeDiagnostic> {
    vec![
        DiagnosticPayload::CaptureAccounting {
            statistics: CaptureStatistics {
                dropped_native_blocks: 2,
                dropped_normalized_frames: 3,
                finalized: true,
            },
        },
        DiagnosticPayload::PlaybackAccounting {
            boundary: None,
            statistics: PlaybackStatistics {
                input_sample_rate_hz: 24000,
                accepted_for_processing_samples: 240,
                discarded_before_conversion_samples: 24,
                native: Some(NativePlaybackStatistics {
                    sample_rate_hz: 48000,
                    generated_mono_samples: 480,
                    callback_consumed_mono_samples: 400,
                    discarded_mono_samples: 80,
                }),
                finalized: true,
                tail_disposition_known: false,
            },
        },
        DiagnosticPayload::Gap {
            cause: GapCause::ProviderReconnect,
            dropped_frames: 4,
        },
        DiagnosticPayload::TranscriptOverflow {
            dropped_interims: 5,
        },
        DiagnosticPayload::Failure {
            code: RuntimeFailureCode::Provider,
            provider_code: Some(-17),
            log_id: Some("synthetic-log".into()),
        },
        DiagnosticPayload::Latency {
            capture_to_send_ms: None,
            response_to_enqueue_ms: Some(3),
            enqueue_to_callback_ms: None,
            provider_processing_ms: None,
        },
        DiagnosticPayload::Playback {
            boundary: PlaybackBoundary::CallbackConsumed,
            accepted_samples: 240,
            discarded_samples: 24,
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, payload)| RuntimeDiagnostic {
        id: id(20 + index as u32),
        session_id: id(2),
        direction: if index == 4 {
            None
        } else {
            Some(SessionDirection::TargetToSource)
        },
        stream_epoch: if index == 4 { None } else { Some(2) },
        occurred_at_ms: 1_730_000_000_123,
        payload,
    })
    .collect()
}

#[test]
fn wire_contract_real_runtime_dtos_match_shared_json() {
    let expected: Value =
        serde_json::from_str(include_str!("../../../tests/contracts/runtime-wire.json")).unwrap();
    let live = SessionLaunch {
        config: SessionConfig::live(languages(), SessionDirection::SourceToTarget).unwrap(),
        provider_config_id: id(1),
        devices: DeviceSelection::Live {
            input_id: "input".into(),
            m1_output_id: "m1".into(),
        },
    };
    let call = SessionLaunch {
        config: SessionConfig::call(languages()).unwrap(),
        provider_config_id: id(1),
        devices: DeviceSelection::Call {
            a_input_id: "input".into(),
            m2_input_id: "m2".into(),
            m1_output_id: "m1".into(),
            physical_output_id: "physical".into(),
        },
    };
    let record = SessionRecord {
        id: id(2),
        started_at: 1_730_000_000,
        ended_at: None,
        mode: SessionMode::Call,
        direction: SessionDirection::Bidirectional,
        source_language: "en".into(),
        target_language: "zh".into(),
        status: SessionStatus::Starting,
        context: Some(SessionContext {
            launch: call.clone(),
            provider: ProviderId::new("volcengine").unwrap(),
            provider_name: "Fixture provider".into(),
            endpoints: endpoints(),
        }),
        runtime: Some(RuntimeSnapshot {
            phase: RuntimePhase::Starting,
            directions: [
                SessionDirection::SourceToTarget,
                SessionDirection::TargetToSource,
            ]
            .into_iter()
            .map(|direction| DirectionSnapshot {
                direction,
                phase: DirectionPhase::Starting,
                stream_epoch: 0,
                failure: None,
            })
            .collect(),
            outcome: None,
        }),
    };
    let terminal = SessionRecord {
        status: SessionStatus::Failed,
        ended_at: Some(1_730_000_010),
        runtime: None,
        ..record.clone()
    };
    let legacy_record = SessionRecord {
        context: None,
        ..terminal.clone()
    };
    let interim = transcript(10, TranscriptKind::Source, false);
    let source = transcript(10, TranscriptKind::Source, true);
    let translation = transcript(11, TranscriptKind::Translation, true);
    let legacy = TranscriptUpdate {
        id: id(12),
        details: None,
        occurred_at_ms: 1_730_000_000_000,
        text: "Legacy".into(),
        ..source.clone()
    };
    let diagnostics = diagnostics();
    let occurred_at = UNIX_EPOCH + Duration::new(1_730_000_000, 123_000_000);
    let events = vec![
        SessionEvent::Status {
            event_id: id(30),
            session_id: id(2),
            occurred_at,
            status: "running".into(),
        },
        SessionEvent::AudioState {
            event_id: id(31),
            session_id: id(2),
            occurred_at,
            state: AudioSessionState::Paused {
                reason: "Fixture pause".into(),
            },
        },
        SessionEvent::Transcript {
            update: source.clone(),
        },
        SessionEvent::Diagnostic {
            diagnostic: diagnostics[4].clone(),
        },
        SessionEvent::ResyncNeeded {
            session_id: id(2),
            missed_events: 513,
        },
    ];
    let inventory = AudioInventory {
        generation: id(40),
        driver: DriverStatus::Ready,
        endpoints: endpoints(),
    };
    let ticket = SubscriptionTicket {
        id: id(3),
        event_name: format!("voxbridge://session/{}", id(3)),
    };
    let transcript_page = TranscriptPage {
        rows: vec![source.clone(), translation.clone(), legacy.clone()],
        next_cursor: Some(200),
    };
    let diagnostic_page = DiagnosticPage {
        rows: diagnostics.clone(),
        next_cursor: None,
    };
    let capabilities = ProviderCapabilities {
        language_pairs: vec![languages()],
        input_audio: PcmSpec::pcm16_mono(16000),
        output_audio: PcmSpec::pcm16_mono(24000),
        native_chunk_duration: Duration::from_millis(80),
        supports_tts: true,
        supports_bidirectional: true,
        ready: true,
        unavailable_reason: None,
    };
    let mut provider = ProviderConfig::new(
        id(1),
        "volcengine",
        "wss://openspeech.bytedance.com/api/v4/ast/v2/translate",
        None,
        Some("fixture-model".into()),
        vec!["en".into(), "zh".into()],
        None,
    )
    .unwrap()
    .with_display_name("Fixture provider".into())
    .unwrap();
    provider.embedding = Some(EmbeddingConfig {
        model: "fixture-embedding".into(),
        dimensions: Some(1024),
    });
    provider.tavily = Some(TavilyConfig {
        endpoint: "https://example.invalid/search".into(),
        search_depth: None,
    });
    let provider_view = ProviderConfigView {
        config: provider.public_view(),
        credential_configured: false,
    };
    let validation = LaunchValidation {
        context: record.context.clone().unwrap(),
        input_audio: PcmSpec::pcm16_mono(16000),
        output_audio: PcmSpec::pcm16_mono(24000),
    };
    let actual = json!({
        "liveLaunch": live, "callLaunch": call, "callRecord": record,
        "terminalRecord": terminal, "legacyRecord": legacy_record,
        "reverseSourceInterim": interim, "reverseSourceFinal": source,
        "reverseTranslationFinal": translation, "legacyRow": legacy,
        "diagnostics": diagnostics, "events": events, "inventory": inventory,
        "driverStatuses": [DriverStatus::NeedsPrivileges, DriverStatus::NeedsApproval("Approve fixture driver".into()), DriverStatus::NeedsRestart, DriverStatus::Failed("Fixture unavailable".into()), DriverStatus::Ready],
        "ticket": ticket, "transcriptPage": transcript_page, "diagnosticPage": diagnostic_page,
        "capabilities": capabilities, "providerConfigView": provider_view, "launchValidation": validation,
        "enumVariants": {
            "sessionStatus": [SessionStatus::Created, SessionStatus::Starting, SessionStatus::Running, SessionStatus::Stopping, SessionStatus::Paused, SessionStatus::Failed, SessionStatus::Completed],
            "directionPhase": [DirectionPhase::Starting, DirectionPhase::Running, DirectionPhase::Reconnecting, DirectionPhase::Paused, DirectionPhase::Stopped],
            "runtimePhase": [RuntimePhase::Starting, RuntimePhase::Running, RuntimePhase::Paused, RuntimePhase::Stopping, RuntimePhase::Stopped],
            "failureCode": [RuntimeFailureCode::Provider, RuntimeFailureCode::Storage, RuntimeFailureCode::DeviceUnavailable, RuntimeFailureCode::AudioBackendUnavailable, RuntimeFailureCode::StartupTimeout, RuntimeFailureCode::FinishTimeout, RuntimeFailureCode::PlaybackTimeout, RuntimeFailureCode::CleanupTimeout, RuntimeFailureCode::Interrupted, RuntimeFailureCode::InvalidEvent],
            "gapCause": [GapCause::ProviderReconnect, GapCause::CaptureOverflow, GapCause::OutputOverflow, GapCause::DeviceLoss, GapCause::ManualResume],
            "playbackBoundary": [PlaybackBoundary::CallbackConsumed, PlaybackBoundary::PulseServerDrained, PlaybackBoundary::Discarded],
            "sampleFormat": [SampleFormat::I16, SampleFormat::F32],
            "exportFormat": [ExportFormat::Markdown, ExportFormat::Json, ExportFormat::PlainText],
            "audioState": [AudioSessionState::Running, AudioSessionState::Paused { reason: "Fixture pause".into() }],
        },
        "outcomes": [RuntimeOutcome::Completed, RuntimeOutcome::Failed { codes: vec![RuntimeFailureCode::Provider, RuntimeFailureCode::CleanupTimeout] }],
    });
    assert_eq!(
        actual.as_object().unwrap().len(),
        expected.as_object().unwrap().len()
    );
    for (name, value) in actual.as_object().unwrap() {
        assert_eq!(*value, expected[name], "shared wire fixture: {name}");
    }
}
