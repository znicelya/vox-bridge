use std::time::Duration;

use uuid::Uuid;

use crate::{
    audio::frame::{AudioFrame, PcmSpec},
    domain::{
        audio::AudioSource,
        session::{LanguagePair, SessionDirection, SessionMode},
    },
};

use super::{
    fixture::{FixtureProvider, FixtureScript, FixtureStep},
    *,
};

fn config() -> ProviderSessionConfig {
    ProviderSessionConfig {
        session_id: Uuid::new_v4(),
        direction: SessionDirection::SourceToTarget,
        mode: SessionMode::Live,
        languages: LanguagePair::new("en", "zh").unwrap(),
        target_audio: PcmSpec::pcm16_mono(16_000),
        provider_config_id: Uuid::new_v4(),
    }
}

fn frame(sequence: u64) -> AudioFrame {
    AudioFrame::new(
        AudioSource::Microphone,
        sequence,
        PcmSpec::pcm16_mono(16_000),
        vec![1; 1280],
    )
    .unwrap()
}

fn metadata(native: &str) -> ProviderEventMetadata {
    ProviderEventMetadata {
        session_id: Uuid::nil(),
        direction: SessionDirection::SourceToTarget,
        provider_event_type: Some(native.into()),
        timestamp: Some(Duration::from_millis(80)),
        sentence_id: None,
    }
}

fn started() -> ProviderEvent {
    ProviderEvent::SessionStarted {
        metadata: metadata("150"),
    }
}

fn script(steps: impl IntoIterator<Item = FixtureStep>) -> FixtureProvider {
    FixtureProvider::new(FixtureScript::new(steps).unwrap())
}

async fn event(stream: &dyn ProviderConnection) -> Option<ProviderEvent> {
    tokio::time::timeout(Duration::from_secs(1), stream.next_event())
        .await
        .expect("event wait must terminate")
        .unwrap()
}

#[tokio::test]
async fn provider_contract_rejects_audio_until_actual_handshake() {
    let provider: Box<dyn TranslationProvider> = Box::new(script([
        FixtureStep::Delay(Duration::from_millis(30)),
        FixtureStep::Emit(started()),
        FixtureStep::ReceiveAudio { count: 1 },
    ]));
    let stream = provider.connect(config()).await.unwrap();
    assert_eq!(
        stream.send_audio(frame(1)).await,
        Err(ProviderError::NotStarted)
    );
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::SessionStarted { .. })
    ));
    stream.send_audio(frame(2)).await.unwrap();
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::Finished { .. })
    ));
    assert_eq!(event(&*stream).await, None);
    assert_eq!(
        stream.send_audio(frame(3)).await,
        Err(ProviderError::Closed)
    );
}

#[tokio::test]
async fn provider_contract_receive_does_not_block_concurrent_audio() {
    let provider = FixtureProvider::new(FixtureScript::started_then_final_text());
    let stream = provider.connect(config()).await.unwrap();
    assert!(event(&*stream).await.unwrap().is_session_started());
    let (received, sent) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(stream.next_event(), async {
            tokio::task::yield_now().await;
            stream.send_audio(frame(1)).await
        })
    })
    .await
    .expect("a pending receive must not exclude sends");
    sent.unwrap();
    assert!(received.unwrap().unwrap().is_final_translation());
}

#[tokio::test]
async fn provider_contract_audio_admission_is_bounded_and_cancel_bypasses_it() {
    let stream = script([
        FixtureStep::Emit(started()),
        FixtureStep::Delay(Duration::from_secs(3600)),
    ])
    .connect(config())
    .await
    .unwrap();
    event(&*stream).await.unwrap();
    for sequence in 0..64 {
        stream.send_audio(frame(sequence)).await.unwrap();
    }
    assert_eq!(
        stream.send_audio(frame(64)).await,
        Err(ProviderError::QueueFull)
    );
    tokio::time::timeout(Duration::from_secs(1), stream.cancel())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Cancelled,
            ..
        })
    ));
    assert_eq!(event(&*stream).await, None);
    assert_eq!(
        stream.send_audio(frame(65)).await,
        Err(ProviderError::Closed)
    );
}

#[tokio::test]
async fn provider_contract_script_builder_stops_at_its_bound() {
    let mut visited = 0;
    let result = FixtureScript::new(std::iter::from_fn(|| {
        visited += 1;
        assert!(visited <= 65, "must not collect an unbounded script");
        Some(FixtureStep::Emit(started()))
    }));
    assert!(matches!(result, Err(ProviderError::InvalidScript)));
    assert_eq!(visited, 65);
}

#[tokio::test]
async fn provider_contract_output_backpressure_does_not_block_finish_or_cancel() {
    let stream = script([
        FixtureStep::Emit(started()),
        FixtureStep::EmitRepeated {
            event: ProviderEvent::Muted {
                metadata: metadata("250"),
            },
            count: 65,
        },
        FixtureStep::ReceiveAudio { count: 64 },
        FixtureStep::WaitForFinish,
    ])
    .connect(config())
    .await
    .unwrap();
    event(&*stream).await.unwrap();
    // Let the actor run until its output queue fills. A third party is not
    // consuming output; it must stop before the ReceiveAudio instruction.
    tokio::task::yield_now().await;
    for sequence in 0..64 {
        stream.send_audio(frame(sequence)).await.unwrap();
    }
    tokio::task::yield_now().await;
    assert_eq!(
        stream.send_audio(frame(64)).await,
        Err(ProviderError::QueueFull)
    );
    tokio::time::timeout(Duration::from_secs(1), stream.finish())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stream.send_audio(frame(65)).await,
        Err(ProviderError::Closed)
    );
    tokio::time::timeout(Duration::from_secs(1), stream.cancel())
        .await
        .unwrap()
        .unwrap();
    for _ in 0..64 {
        assert!(matches!(
            event(&*stream).await,
            Some(ProviderEvent::Muted { .. })
        ));
    }
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Cancelled,
            ..
        })
    ));
    assert_eq!(event(&*stream).await, None);
}

#[tokio::test]
async fn provider_contract_finish_preserves_final_events_and_is_idempotent() {
    let stream = script([
        FixtureStep::Emit(started()),
        FixtureStep::WaitForFinish,
        FixtureStep::Emit(ProviderEvent::Muted {
            metadata: metadata("250"),
        }),
        FixtureStep::Emit(ProviderEvent::Finished {
            metadata: metadata("152"),
        }),
        FixtureStep::Emit(started()),
    ])
    .connect(config())
    .await
    .unwrap();
    event(&*stream).await.unwrap();
    stream.finish().await.unwrap();
    stream.finish().await.unwrap();
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::Muted { .. })
    ));
    let terminal = event(&*stream).await.unwrap();
    assert!(matches!(terminal, ProviderEvent::Finished { .. }));
    assert_eq!(
        terminal.metadata().provider_event_type.as_deref(),
        Some("152")
    );
    stream.cancel().await.unwrap();
    assert_eq!(event(&*stream).await, None);
}

#[tokio::test]
async fn provider_contract_failure_and_disconnect_are_single_terminal_events() {
    for (step, expected) in [
        (
            FixtureStep::Emit(ProviderEvent::Failed {
                metadata: metadata("153"),
                error: ProviderError::Authentication,
            }),
            ProviderError::Authentication,
        ),
        (FixtureStep::Disconnect, ProviderError::Disconnected),
    ] {
        let stream = script([
            FixtureStep::Emit(started()),
            FixtureStep::Delay(Duration::from_millis(5)),
            step,
            FixtureStep::Emit(started()),
        ])
        .connect(config())
        .await
        .unwrap();
        event(&*stream).await.unwrap();
        assert!(
            matches!(event(&*stream).await, Some(ProviderEvent::Failed { error, .. }) if error == expected)
        );
        assert_eq!(
            stream.send_audio(frame(1)).await,
            Err(ProviderError::Closed)
        );
        assert_eq!(event(&*stream).await, None);
        stream.cancel().await.unwrap();
        assert_eq!(event(&*stream).await, None);
    }
}

#[tokio::test]
async fn provider_contract_rejects_data_before_handshake_and_duplicate_handshakes() {
    for steps in [
        vec![FixtureStep::Emit(ProviderEvent::Muted {
            metadata: metadata("250"),
        })],
        vec![FixtureStep::Emit(started()), FixtureStep::Emit(started())],
    ] {
        let stream = script(steps).connect(config()).await.unwrap();
        let mut received = event(&*stream).await.unwrap();
        if received.is_session_started() {
            received = event(&*stream).await.unwrap();
        }
        assert!(matches!(
            received,
            ProviderEvent::Failed {
                error: ProviderError::Protocol,
                ..
            }
        ));
        assert_eq!(event(&*stream).await, None);
    }
}

#[tokio::test]
async fn provider_contract_call_directions_have_independent_queues_and_lifecycle() {
    let provider = script([
        FixtureStep::Emit(started()),
        FixtureStep::ReceiveAudio { count: 1 },
        FixtureStep::Emit(ProviderEvent::Muted {
            metadata: metadata("250"),
        }),
        FixtureStep::WaitForFinish,
    ]);
    let mut incoming = config();
    incoming.mode = SessionMode::Call;
    let mut outgoing = incoming.clone();
    outgoing.direction = SessionDirection::TargetToSource;
    outgoing.languages = LanguagePair::new("zh", "en").unwrap();
    let first = provider.connect(incoming.clone()).await.unwrap();
    let second = provider.connect(outgoing).await.unwrap();
    event(&*first).await.unwrap();
    let second_started = event(&*second).await.unwrap();
    assert_eq!(second_started.metadata().session_id, incoming.session_id);
    assert_eq!(
        second_started.metadata().direction,
        SessionDirection::TargetToSource
    );
    first.cancel().await.unwrap();
    assert!(matches!(
        event(&*first).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Cancelled,
            ..
        })
    ));
    second.send_audio(frame(1)).await.unwrap();
    assert!(matches!(
        event(&*second).await,
        Some(ProviderEvent::Muted { .. })
    ));
    second.finish().await.unwrap();
    assert!(matches!(
        event(&*second).await,
        Some(ProviderEvent::Finished { .. })
    ));
}

#[tokio::test]
async fn provider_contract_preserves_normalized_payloads_and_native_correlation() {
    use crate::domain::translation::{TimedTranslation, Translation};
    let text = TimedTranslation {
        translation: Translation {
            text: "hello".into(),
            language: "en".into(),
        },
        start: Some(Duration::from_millis(20)),
        end: Some(Duration::from_millis(80)),
    };
    let mut native = metadata("652");
    native.sentence_id = Some("wire-7".into());
    let stream = script([
        FixtureStep::Emit(started()),
        FixtureStep::Emit(ProviderEvent::SourceInterim {
            metadata: metadata("651"),
            text: text.clone(),
        }),
        FixtureStep::Emit(ProviderEvent::SourceFinal {
            metadata: native,
            text: text.clone(),
        }),
        FixtureStep::Emit(ProviderEvent::TranslationInterim {
            metadata: metadata("654"),
            text: text.clone(),
        }),
        FixtureStep::Emit(ProviderEvent::TranslationFinal {
            metadata: metadata("655"),
            text: text.clone(),
        }),
        FixtureStep::Delay(Duration::from_millis(5)),
        FixtureStep::Emit(ProviderEvent::Audio {
            metadata: metadata("352"),
            frame: frame(9),
        }),
    ])
    .connect(config())
    .await
    .unwrap();
    assert_eq!(event(&*stream).await.unwrap().metadata().sentence_id, None);
    assert!(
        matches!(event(&*stream).await, Some(ProviderEvent::SourceInterim { text: actual, .. }) if actual == text)
    );
    let source_final = event(&*stream).await.unwrap();
    assert_eq!(
        source_final.metadata().sentence_id.as_deref(),
        Some("wire-7")
    );
    assert_eq!(
        source_final.metadata().timestamp,
        Some(Duration::from_millis(80))
    );
    assert!(
        matches!(source_final, ProviderEvent::SourceFinal { text: actual, .. } if actual == text)
    );
    assert!(
        matches!(event(&*stream).await, Some(ProviderEvent::TranslationInterim { text: actual, .. }) if actual == text)
    );
    assert!(event(&*stream).await.unwrap().is_final_translation());
    match event(&*stream).await.unwrap() {
        ProviderEvent::Audio { metadata, frame } => {
            assert_eq!(metadata.provider_event_type.as_deref(), Some("352"));
            assert_eq!(frame.sequence(), 9);
            assert_eq!(frame.samples(), &[1; 1280]);
            assert_eq!(frame.duration(), Duration::from_millis(80));
        }
        other => panic!("expected audio, received {other:?}"),
    }
}

#[tokio::test]
async fn provider_contract_rejects_unsupported_sessions_and_oversized_frames() {
    let provider = script([FixtureStep::Emit(started()), FixtureStep::WaitForFinish]);
    let mut unsupported = config();
    unsupported.languages = LanguagePair::new("de", "fr").unwrap();
    assert!(matches!(
        provider.connect(unsupported).await,
        Err(ProviderError::UnsupportedLanguagePair)
    ));
    let mut unsupported = config();
    unsupported.target_audio = PcmSpec::pcm16_mono(24_000);
    assert!(matches!(
        provider.connect(unsupported).await,
        Err(ProviderError::UnsupportedAudioFormat)
    ));
    let stream = provider.connect(config()).await.unwrap();
    event(&*stream).await.unwrap();
    let wrong_spec = AudioFrame::new(
        AudioSource::Microphone,
        1,
        PcmSpec::pcm16_mono(24_000),
        vec![1; 1280],
    )
    .unwrap();
    assert_eq!(
        stream.send_audio(wrong_spec).await,
        Err(ProviderError::UnsupportedAudioFormat)
    );
    let oversized = AudioFrame::new(
        AudioSource::Microphone,
        2,
        PcmSpec::pcm16_mono(16_000),
        vec![1; 1281],
    )
    .unwrap();
    assert_eq!(
        stream.send_audio(oversized).await,
        Err(ProviderError::InvalidAudioFrame)
    );
    stream.send_audio(frame(3)).await.unwrap();
    stream.cancel().await.unwrap();
}

#[test]
fn provider_contract_capabilities_disable_unready_and_unsupported_call_modes() {
    let provider = script([]);
    let mut capabilities = provider.capabilities();
    let mut call = config();
    call.mode = SessionMode::Call;
    capabilities.supports_bidirectional = false;
    assert_eq!(
        capabilities.validate(&call),
        Err(ProviderError::UnsupportedBidirectional)
    );
    capabilities.ready = false;
    capabilities.unavailable_reason = Some("fixture disabled".into());
    assert_eq!(
        capabilities.validate(&config()),
        Err(ProviderError::Unavailable)
    );
}

#[test]
fn provider_contract_bounds_script_payload_bytes() {
    let mut oversized = metadata("150");
    oversized.sentence_id = Some("x".repeat(65_537));
    assert!(matches!(
        FixtureScript::new([FixtureStep::Emit(ProviderEvent::SessionStarted {
            metadata: oversized
        })]),
        Err(ProviderError::InvalidScript)
    ));
}

#[tokio::test]
async fn provider_contract_cancelled_receive_does_not_consume_the_next_event() {
    let stream = script([
        FixtureStep::Emit(started()),
        FixtureStep::ReceiveAudio { count: 1 },
        FixtureStep::Emit(ProviderEvent::Muted {
            metadata: metadata("250"),
        }),
    ])
    .connect(config())
    .await
    .unwrap();
    event(&*stream).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(5), stream.next_event())
            .await
            .is_err()
    );
    stream.send_audio(frame(1)).await.unwrap();
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::Muted { .. })
    ));
}

#[tokio::test]
async fn provider_contract_script_exhaustion_requires_a_completed_handshake() {
    let stream = script([]).connect(config()).await.unwrap();
    assert!(matches!(
        event(&*stream).await,
        Some(ProviderEvent::Failed {
            error: ProviderError::Protocol,
            ..
        })
    ));
    assert_eq!(event(&*stream).await, None);
}
