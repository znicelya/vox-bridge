use std::time::Duration;

use tokio::time::timeout;

use crate::{
    audio::{AudioFrame, PcmSpec},
    domain::{
        audio::AudioSource,
        launch::{DirectionPhase, RuntimeOutcome, RuntimePhase},
        session::SessionDirection,
        transcript::{DiagnosticPayload, GapCause, RuntimeFailureCode},
    },
    events::{EventBusError, SessionEvent},
    providers::{
        ProviderError, ProviderEvent,
        fixture::{FixtureScript, FixtureStep},
    },
    session::test_support::{RuntimeHarness, final_text, metadata, wait_phase},
};

#[tokio::test]
async fn provider_tail_is_saved_and_played_before_stop_completes() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::WaitForFinish,
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "pump"),
            }),
            FixtureStep::Emit(ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("zh", "泵"),
            }),
            FixtureStep::Emit(ProviderEvent::Audio {
                metadata: metadata(),
                frame: AudioFrame::new(
                    AudioSource::Microphone,
                    0,
                    PcmSpec::pcm16_mono(24_000),
                    vec![7; 1920],
                )
                .unwrap(),
            }),
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let prepared = h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap();
    let run = h.runtime.start(prepared);
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let run2 = run.clone();
    let stopped = tokio::spawn(async move { run2.stop().await });
    let played = timeout(Duration::from_secs(1), h.audio.next_playback("m1-render"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(played.samples(), &[7; 1920]);
    assert_eq!(stopped.await.unwrap(), RuntimeOutcome::Completed);
    assert_eq!(
        h.store.transcript_page(h.id, None, 50).unwrap().rows.len(),
        2
    );
    assert_eq!(h.provider.finish_counts(), [1, 0]);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn capture_before_started_is_cleared_and_never_admitted() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Delay(Duration::from_millis(50)),
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    tokio::time::sleep(Duration::from_millis(10)).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                10,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    wait_phase(&run, RuntimePhase::Running).await;
    assert!(
        h.provider
            .sent_sequences(SessionDirection::SourceToTarget)
            .is_empty()
    );
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                11,
                PcmSpec::pcm16_mono(16_000),
                vec![2; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if h.provider.sent_sequences(SessionDirection::SourceToTarget) == vec![0] {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn call_directions_route_translated_audio_to_independent_outputs() {
    let script = |sample| {
        FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::Emit(ProviderEvent::Audio {
                    metadata: metadata(),
                    frame: AudioFrame::new(
                        AudioSource::Microphone,
                        0,
                        PcmSpec::pcm16_mono(24_000),
                        vec![sample; 1920],
                    )
                    .unwrap(),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap()
    };
    let h = RuntimeHarness::call(script(3), script(9)).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    assert_eq!(
        h.audio.next_playback("m1-render").await.unwrap().samples()[0],
        3
    );
    assert_eq!(
        h.audio
            .next_playback("physical-output")
            .await
            .unwrap()
            .samples()[0],
        9
    );
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn reverse_live_swaps_languages_routes_to_m1_and_opens_one_stream() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("zh", "你好"),
            }),
            FixtureStep::Emit(ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("en", "hello"),
            }),
            FixtureStep::Emit(ProviderEvent::Audio {
                metadata: metadata(),
                frame: AudioFrame::new(
                    AudioSource::Microphone,
                    0,
                    PcmSpec::pcm16_mono(24_000),
                    vec![5; 1920],
                )
                .unwrap(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live_reverse(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        h.audio.next_playback("m1-render").await.unwrap().samples()[0],
        5
    );
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
    assert_eq!(h.provider.connect_counts(), [0, 1]);
    let rows = h.store.transcript_page(h.id, None, 50).unwrap().rows;
    assert_eq!(
        rows.iter()
            .map(|row| row.language.as_str())
            .collect::<Vec<_>>(),
        ["zh", "en"]
    );
}

#[tokio::test]
async fn provider_gap_reopens_audio_without_opening_a_new_provider_stream() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(ProviderEvent::Gap {
                metadata: metadata(),
            }),
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let snapshot = run.status().borrow().clone();
            if snapshot.directions[0].phase == DirectionPhase::Running
                && snapshot.directions[0].stream_epoch == 1
                && h.audio.open_ports() == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                1,
                PcmSpec::pcm16_mono(16_000),
                vec![2; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if h.provider.sent_sequences(SessionDirection::SourceToTarget) == vec![0, 1] {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn restored_device_does_not_reconnect_until_explicit_resume() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    assert!(h.bus.is_healthy());
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio.lose("live-input");
    wait_phase(&run, RuntimePhase::Paused).await;
    h.audio.restore("live-input");
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    run.resume_direction(SessionDirection::SourceToTarget)
        .await
        .unwrap();
    wait_phase(&run, RuntimePhase::Running).await;
    assert_eq!(h.provider.connect_counts(), [2, 0]);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn call_stays_starting_until_both_directions_are_ready() {
    let forward = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let reverse = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Delay(Duration::from_millis(50)),
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(forward, reverse).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(run.status().borrow().phase, RuntimePhase::Starting);
    wait_phase(&run, RuntimePhase::Running).await;
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn playback_overflow_replaces_only_audio_and_keeps_provider_stream() {
    let audio = ProviderEvent::Audio {
        metadata: metadata(),
        frame: AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![4; 1920],
        )
        .unwrap(),
    };
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::EmitRepeated {
                event: audio,
                count: 10,
            },
            FixtureStep::Emit(ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("zh", "tail"),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if h.store.transcript_page(h.id, None, 50).unwrap().rows.len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert_eq!(run.status().borrow().directions[0].stream_epoch, 0);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
    assert!(
        h.store
            .diagnostic_page(h.id, None, 100)
            .unwrap()
            .rows
            .iter()
            .any(|row| matches!(
                row.payload,
                DiagnosticPayload::Gap {
                    cause: GapCause::OutputOverflow,
                    dropped_frames: 1
                }
            ))
    );
}

#[tokio::test]
async fn packet_admission_is_paced_at_the_packet_duration() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 2 },
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 2560],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while h.provider.sent_at(SessionDirection::SourceToTarget).len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let sent = h.provider.sent_at(SessionDirection::SourceToTarget);
    assert!(sent[1].duration_since(sent[0]) >= Duration::from_millis(70));
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn accepted_capture_tail_is_bounded_by_one_total_drain_deadline() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 5_120],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while h
            .provider
            .sent_sequences(SessionDirection::SourceToTarget)
            .is_empty()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let release = h.provider.delay_send(SessionDirection::SourceToTarget);
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Interrupted]
        }
    );
    release.release();
    assert_eq!(h.provider.finish_counts(), [0, 0]);
    assert_eq!(h.audio.open_ports(), 0);
    assert!(
        h.store
            .diagnostic_page(h.id, None, 50)
            .unwrap()
            .rows
            .iter()
            .any(|row| matches!(
                row.payload,
                DiagnosticPayload::Gap {
                    cause: GapCause::CaptureOverflow,
                    dropped_frames,
                } if dropped_frames > 0
            ))
    );
}

#[tokio::test]
async fn queue_full_drops_only_rejected_packet_and_records_admission_loss() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    h.provider.queue_full_once(SessionDirection::SourceToTarget);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    for sequence in 0..2 {
        h.audio
            .send_capture(
                "live-input",
                AudioFrame::new(
                    AudioSource::Microphone,
                    sequence,
                    PcmSpec::pcm16_mono(16_000),
                    vec![sequence as i16; 1280],
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while h.provider.sent_sequences(SessionDirection::SourceToTarget) != vec![1] {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        h.provider
            .queue_full_count(SessionDirection::SourceToTarget),
        1
    );
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert_eq!(run.status().borrow().phase, RuntimePhase::Running);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
    let diagnostics = h.store.diagnostic_page(h.id, None, 50).unwrap().rows;
    assert!(diagnostics.iter().any(|row| matches!(
        row.payload,
        DiagnosticPayload::Gap {
            cause: GapCause::CaptureOverflow,
            dropped_frames: 1
        }
    )));
}

#[tokio::test]
async fn not_started_before_delivered_gap_keeps_final_and_reuses_connection() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "before gap"),
            }),
            FixtureStep::Emit(ProviderEvent::Gap {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(250)),
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.provider.delay_gap(SessionDirection::SourceToTarget);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while h.store.transcript_page(h.id, None, 50).unwrap().rows.len() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                1,
                PcmSpec::pcm16_mono(16_000),
                vec![2; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while h
            .provider
            .not_started_count(SessionDirection::SourceToTarget)
            != 1
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(run.status().borrow().directions[0].stream_epoch, 0);
    release.release();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let snapshot = run.status().borrow().clone();
            if snapshot.directions[0].phase == DirectionPhase::Running
                && snapshot.directions[0].stream_epoch == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                2,
                PcmSpec::pcm16_mono(16_000),
                vec![3; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while h.provider.sent_sequences(SessionDirection::SourceToTarget) != vec![0, 2] {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn stop_owns_delayed_audio_open_and_closes_late_success() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.audio.delay_open("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    tokio::time::sleep(Duration::from_millis(10)).await;
    let stopped = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    release.send(Ok(())).unwrap();
    assert_eq!(stopped.await.unwrap(), RuntimeOutcome::Completed);
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(run.status().borrow().phase, RuntimePhase::Stopped);
}

#[tokio::test]
async fn delayed_audio_open_failure_preserves_cleanup_timeout_category() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [FixtureStep::Emit(ProviderEvent::SessionStarted {
            metadata: metadata(),
        })],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.audio.delay_open("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    tokio::time::sleep(Duration::from_millis(10)).await;
    release
        .send(Err(RuntimeFailureCode::CleanupTimeout))
        .unwrap();
    wait_phase(&run, RuntimePhase::Stopped).await;
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::CleanupTimeout]
        }
    );
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn stop_flushes_latency_capture_and_playback_diagnostics() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(ProviderEvent::Audio {
                metadata: metadata(),
                frame: AudioFrame::new(
                    AudioSource::Microphone,
                    0,
                    PcmSpec::pcm16_mono(24_000),
                    vec![6; 1920],
                )
                .unwrap(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let played = h.audio.next_playback("m1-render").await.unwrap();
    assert_eq!(played.samples()[0], 6);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
    let diagnostics = h.store.diagnostic_page(h.id, None, 50).unwrap().rows;
    assert!(diagnostics.iter().any(|row| matches!(
        row.payload,
        DiagnosticPayload::Latency {
            capture_to_send_ms: Some(_),
            ..
        }
    )));
    assert!(
        diagnostics
            .iter()
            .any(|row| matches!(row.payload, DiagnosticPayload::CaptureAccounting { ref statistics } if statistics.finalized))
    );
    assert!(diagnostics.iter().any(|row| matches!(
        row.payload,
        DiagnosticPayload::PlaybackAccounting {
            boundary: Some(_),
            ref statistics,
        } if statistics.finalized && statistics.tail_disposition_known
    )));
}

#[tokio::test]
async fn unconsumed_provider_tail_fails_with_uncertain_playback_accounting() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
            FixtureStep::Emit(ProviderEvent::Audio {
                metadata: metadata(),
                frame: AudioFrame::new(
                    AudioSource::Microphone,
                    0,
                    PcmSpec::pcm16_mono(24_000),
                    vec![6; 1920],
                )
                .unwrap(),
            }),
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::PlaybackTimeout]
        }
    );
    assert_eq!(h.audio.open_ports(), 0);
    assert!(
        h.store
            .diagnostic_page(h.id, None, 50)
            .unwrap()
            .rows
            .iter()
            .any(|row| matches!(
                row.payload,
                DiagnosticPayload::PlaybackAccounting { boundary: None, .. }
            ))
    );
}

#[tokio::test]
async fn provider_failure_diagnostic_preserves_remote_code_and_log_id() {
    let mut failed_metadata = metadata();
    failed_metadata.log_id = Some("provider-log-17".into());
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(ProviderEvent::Failed {
                metadata: failed_metadata,
                error: ProviderError::RemoteFailure { code: 429 },
            }),
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Paused).await;
    assert!(
        h.store
            .diagnostic_page(h.id, None, 50)
            .unwrap()
            .rows
            .iter()
            .any(|row| matches!(
                row.payload,
                DiagnosticPayload::Failure {
                    code: RuntimeFailureCode::Provider,
                    provider_code: Some(429),
                    log_id: Some(ref log_id),
                } if log_id == "provider-log-17"
            ))
    );
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Provider]
        }
    );
}

#[tokio::test]
async fn capture_overflow_reopens_audio_without_reconnecting_provider() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    for sequence in 0..30 {
        h.audio
            .send_capture(
                "live-input",
                AudioFrame::new(
                    AudioSource::Microphone,
                    sequence,
                    PcmSpec::pcm16_mono(16_000),
                    vec![sequence as i16; 1280],
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while h.audio.total_opens() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert_eq!(run.status().borrow().directions[0].stream_epoch, 0);
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
    assert!(h
        .store
        .diagnostic_page(h.id, None, 100)
        .unwrap()
        .rows
        .iter()
        .any(|row| matches!(row.payload, DiagnosticPayload::Gap { cause: GapCause::CaptureOverflow, dropped_frames } if dropped_frames > 0)));
}

#[tokio::test]
async fn missing_provider_terminal_fails_without_claiming_completion() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.provider.delay_terminal(SessionDirection::SourceToTarget);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::FinishTimeout]
        }
    );
    release.release();
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn missing_audio_close_ack_reports_cleanup_timeout() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.audio.delay_close("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::CleanupTimeout]
        }
    );
    let _ = release.send(());
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn transcript_storage_failure_pauses_and_emits_local_storage_diagnostic() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(50)),
            FixtureStep::Emit(ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("zh", "cannot persist"),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let mut events = h.bus.subscribe(h.id);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.store.delete_session(h.id).unwrap();
    wait_phase(&run, RuntimePhase::Paused).await;
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                SessionEvent::Diagnostic { diagnostic }
                    if matches!(diagnostic.payload, DiagnosticPayload::Failure { code: RuntimeFailureCode::Storage, .. })
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Storage]
        }
    );
}

#[tokio::test]
async fn one_call_direction_failure_keeps_peer_live_and_resume_is_scoped() {
    let forward = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(50)),
            FixtureStep::Emit(ProviderEvent::Audio {
                metadata: metadata(),
                frame: AudioFrame::new(
                    AudioSource::Microphone,
                    0,
                    PcmSpec::pcm16_mono(24_000),
                    vec![8; 1920],
                )
                .unwrap(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let reverse = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(20)),
            FixtureStep::Disconnect,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(forward, reverse).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let snapshot = run.status().borrow().clone();
            if snapshot.phase == RuntimePhase::Running
                && snapshot.directions.iter().any(|direction| {
                    direction.direction == SessionDirection::TargetToSource
                        && direction.phase == DirectionPhase::Paused
                })
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        h.audio.next_playback("m1-render").await.unwrap().samples()[0],
        8
    );
    assert_eq!(h.provider.connect_counts(), [1, 1]);
    run.resume_direction(SessionDirection::TargetToSource)
        .await
        .unwrap();
    assert_eq!(h.provider.connect_counts(), [1, 2]);
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Provider]
        }
    );
}

#[tokio::test]
async fn concurrent_stop_drains_saturated_events_with_lagged_ui_once() {
    let interim = ProviderEvent::TranslationInterim {
        metadata: metadata(),
        text: final_text("zh", "streaming"),
    };
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::EmitRepeated {
                event: interim,
                count: 520,
            },
            FixtureStep::WaitForFinish,
            FixtureStep::Emit(ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("zh", "complete"),
            }),
            FixtureStep::Emit(ProviderEvent::Audio {
                metadata: metadata(),
                frame: AudioFrame::new(
                    AudioSource::Microphone,
                    0,
                    PcmSpec::pcm16_mono(24_000),
                    vec![2; 1920],
                )
                .unwrap(),
            }),
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let mut lagged = h.bus.subscribe(h.id);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture(
            "live-input",
            AudioFrame::new(
                AudioSource::Microphone,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![1; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let first = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    let second = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    assert_eq!(
        h.audio.next_playback("m1-render").await.unwrap().samples()[0],
        2
    );
    assert_eq!(first.await.unwrap(), RuntimeOutcome::Completed);
    assert_eq!(second.await.unwrap(), RuntimeOutcome::Completed);
    assert_eq!(h.provider.finish_counts(), [1, 0]);
    assert_eq!(
        h.store.transcript_page(h.id, None, 50).unwrap().rows.len(),
        1
    );
    assert!(matches!(
        lagged.recv().await.map_err(EventBusError::from),
        Err(EventBusError::Lagged(_))
    ));
}

#[tokio::test]
async fn cancel_request_does_not_pause_until_terminal_cleanup_is_observed() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.provider.delay_terminal(SessionDirection::SourceToTarget);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio.lose("live-input");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_ne!(run.status().borrow().phase, RuntimePhase::Paused);
    release.release();
    wait_phase(&run, RuntimePhase::Paused).await;
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::DeviceUnavailable]
        }
    );
}
