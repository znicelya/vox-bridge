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
    assert_eq!(
        run.stop().await,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::DeviceUnavailable]
        }
    );
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
    // Hold the actual ingress consumer so this case tests normalized queue
    // rejection, independently of how fast a native worker is scheduled.
    let _ingress = h.audio.hold_playback_ingress("live-input");
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
    let final_release = h.provider.delay_final(SessionDirection::SourceToTarget);
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
    assert!(
        timeout(Duration::from_secs(1), final_release.wait_entered())
            .await
            .unwrap()
    );
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
    assert_eq!(
        h.store.transcript_page(h.id, None, 50).unwrap().rows.len(),
        0
    );
    final_release.release();
    eventually(|| h.store.transcript_page(h.id, None, 50).unwrap().rows.len() == 1).await;
    assert_eq!(
        h.store.transcript_page(h.id, None, 50).unwrap().rows[0]
            .details
            .as_ref()
            .unwrap()
            .stream_epoch,
        0
    );
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
    assert_eq!(h.audio.total_opens(), 2);
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
            codes: vec![
                RuntimeFailureCode::CleanupTimeout,
                RuntimeFailureCode::FinishTimeout
            ]
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
    assert_eq!(h.audio.open_ports(), 1);
    let _ = release.send(());
    eventually(|| h.audio.open_ports() == 0).await;
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

fn ready_script() -> FixtureScript {
    FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap()
}

async fn eventually(mut predicate: impl FnMut() -> bool) {
    timeout(Duration::from_secs(2), async {
        while !predicate() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

fn capture(sequence: u64, samples: usize, value: i16) -> AudioFrame {
    AudioFrame::new(
        AudioSource::Microphone,
        sequence,
        PcmSpec::pcm16_mono(16_000),
        vec![value; samples],
    )
    .unwrap()
}

#[tokio::test]
async fn repair_stop_during_replacement_is_persistent_and_owns_late_ports() {
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
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let release = h.audio.delay_open("live-input");
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Reconnecting).await;
    let stop = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = release.send(Ok(()));
    let result = timeout(Duration::from_secs(2), stop).await;
    assert!(
        result.is_ok(),
        "stop notification was consumed during recovery"
    );
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_pending_admission_does_not_block_provider_final_or_stop() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(80)),
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "while send pending"),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = h.provider.delay_send(SessionDirection::SourceToTarget);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let final_count = h.store.transcript_page(h.id, None, 50).unwrap().rows.len();
    let stopped = timeout(Duration::from_secs(2), run.stop()).await;
    release.release();
    if stopped.is_err() {
        let _ = timeout(Duration::from_secs(2), run.stop()).await;
    }
    assert_eq!(final_count, 1, "event consumer blocked behind send_audio");
    assert_eq!(
        h.provider.send_calls(),
        [1, 0],
        "admission future was recreated after an event"
    );
    assert!(stopped.is_ok(), "stop blocked behind send_audio");
}

#[tokio::test]
async fn repair_startup_stop_retains_late_open_failure() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let release = h.audio.delay_open("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    eventually(|| h.provider.connect_counts()[0] == 1).await;
    let stop = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let _ = release.send(Err(RuntimeFailureCode::CleanupTimeout));
    assert_eq!(
        timeout(Duration::from_secs(2), stop)
            .await
            .unwrap()
            .unwrap(),
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::CleanupTimeout]
        }
    );
}

#[tokio::test]
async fn repair_ready_peer_failure_terminates_initial_aggregate_startup() {
    let h = RuntimeHarness::call(ready_script(), ready_script()).unwrap();
    let release = h.audio.delay_open("m2-capture");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Running).await;
    h.audio.lose("a-input");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let phase = run.status().borrow().phase.clone();
    let _ = release.send(Ok(()));
    let _ = timeout(Duration::from_secs(2), run.stop()).await;
    assert_eq!(phase, RuntimePhase::Stopping);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_resume_stop_and_saturation_keep_call_peer_responsive() {
    let peer = FixtureScript::with_output_audio(
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
                    vec![9; 1920],
                )
                .unwrap(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let forward = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(40)),
            FixtureStep::Disconnect,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(forward, peer).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Paused).await;
    let release = h.provider.delay_started(SessionDirection::SourceToTarget);
    let resume = tokio::spawn({
        let run = run.clone();
        async move { run.resume_direction(SessionDirection::SourceToTarget).await }
    });
    assert!(
        timeout(Duration::from_secs(1), release.wait_entered())
            .await
            .unwrap()
    );
    let mut duplicates = Vec::new();
    for _ in 0..8 {
        duplicates.push(tokio::spawn({
            let run = run.clone();
            async move { run.resume_direction(SessionDirection::SourceToTarget).await }
        }));
    }
    let mut rejected = 0;
    for duplicate in duplicates {
        if matches!(
            timeout(Duration::from_millis(100), duplicate).await,
            Ok(Ok(Err(_)))
        ) {
            rejected += 1;
        }
    }
    h.audio
        .send_capture(
            "m2-capture",
            AudioFrame::new(
                AudioSource::System,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![4; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let played = timeout(
        Duration::from_secs(1),
        h.audio.next_playback("physical-output"),
    )
    .await
    .unwrap()
    .unwrap();
    let stopped = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let phase = run.status().borrow().phase.clone();
    release.release();
    let result = timeout(Duration::from_secs(2), stopped).await;
    let _ = timeout(Duration::from_secs(2), resume).await;
    assert_eq!(
        rejected, 8,
        "multiple resumes admitted or supervisor blocked"
    );
    assert_eq!(played.samples(), &[9; 1920]);
    assert_eq!(phase, RuntimePhase::Stopping);
    assert!(result.is_ok(), "stop consumed during resumed handshake");
    assert_eq!(h.provider.connect_counts(), [2, 1]);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_failed_resume_open_and_roles_restore_paused_snapshot() {
    use crate::audio::platform::inventory::EndpointRole;
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio.lose("live-input");
    wait_phase(&run, RuntimePhase::Paused).await;
    h.audio.restore("live-input");
    h.audio.set_role("m1-render", EndpointRole::PhysicalOutput);
    assert!(
        run.resume_direction(SessionDirection::SourceToTarget)
            .await
            .is_err()
    );
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    h.audio.set_role("m1-render", EndpointRole::M1Render);
    let release = h.audio.delay_open("live-input");
    let resume = tokio::spawn({
        let run = run.clone();
        async move { run.resume_direction(SessionDirection::SourceToTarget).await }
    });
    eventually(|| h.provider.connect_counts()[0] == 2).await;
    let _ = release.send(Err(RuntimeFailureCode::DeviceUnavailable));
    assert!(resume.await.unwrap().is_err());
    let phase = run.status().borrow().directions[0].phase.clone();
    let _ = run.stop().await;
    assert_eq!(phase, DirectionPhase::Paused);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_failed_resume_handshake_closes_new_ports() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio.lose("live-input");
    wait_phase(&run, RuntimePhase::Paused).await;
    h.audio.restore("live-input");
    let release = h.provider.delay_started(SessionDirection::SourceToTarget);
    let resume = tokio::spawn({
        let run = run.clone();
        async move { run.resume_direction(SessionDirection::SourceToTarget).await }
    });
    assert!(
        timeout(Duration::from_secs(1), release.wait_entered())
            .await
            .unwrap()
    );
    h.audio.lose("m1-render");
    release.release();
    assert!(
        timeout(Duration::from_secs(2), resume)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let phase = run.status().borrow().directions[0].phase.clone();
    let _ = run.stop().await;
    assert_eq!(phase, DirectionPhase::Paused);
    assert_eq!(h.audio.open_ports(), 0);
}

async fn check_pending_control_keeps_final_consumer_live_and_bounded(cancel: bool) {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(60)),
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "operation pending"),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let release = if cancel {
        h.provider.delay_cancel(SessionDirection::SourceToTarget)
    } else {
        h.provider.delay_finish(SessionDirection::SourceToTarget)
    };
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let stopped = if cancel {
        h.audio.lose("live-input");
        None
    } else {
        Some(tokio::spawn({
            let run = run.clone();
            async move { run.stop().await }
        }))
    };
    assert!(
        timeout(Duration::from_secs(1), release.wait_entered())
            .await
            .unwrap()
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    let count = h.store.transcript_page(h.id, None, 50).unwrap().rows.len();
    let bounded = if let Some(stopped) = stopped {
        timeout(Duration::from_secs(2), stopped).await.is_ok()
    } else {
        timeout(Duration::from_secs(2), async {
            eventually(|| run.status().borrow().phase == RuntimePhase::Paused).await;
            run.stop().await
        })
        .await
        .is_ok()
    };
    release.release();
    if !bounded {
        let _ = timeout(Duration::from_secs(2), run.stop()).await;
    }
    assert_eq!(count, 1, "pending provider control blocked final consumer");
    assert!(bounded, "provider control exceeded its deadline");
    assert_eq!(h.audio.open_ports(), 0);
    if cancel {
        assert_eq!(h.provider.cancel_counts(), [1, 0]);
    } else {
        assert_eq!(h.provider.finish_counts(), [1, 0]);
    }
}

#[tokio::test]
async fn repair_pending_finish_keeps_final_consumer_live_and_bounded() {
    check_pending_control_keeps_final_consumer_live_and_bounded(false).await;
}
#[tokio::test]
async fn repair_pending_cancel_keeps_final_consumer_live_and_bounded() {
    check_pending_control_keeps_final_consumer_live_and_bounded(true).await;
}

#[tokio::test]
async fn repair_stop_keeps_packet_pacing_and_counts_total_deadline_remainder() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture("live-input", capture(0, 12_800, 1))
        .await
        .unwrap();
    eventually(|| h.provider.sent_at(SessionDirection::SourceToTarget).len() == 1).await;
    let result = run.stop().await;
    let sent = h.provider.sent_at(SessionDirection::SourceToTarget);
    assert!(sent.len() >= 4 && sent.len() <= 6);
    for pair in sent.windows(2) {
        assert!(pair[1].duration_since(pair[0]) >= Duration::from_millis(70));
    }
    assert!(
        matches!(result, RuntimeOutcome::Failed { codes } if codes.contains(&RuntimeFailureCode::Interrupted))
    );
    let gaps = h.store.diagnostic_page(h.id, None, 100).unwrap().rows;
    assert!(gaps.iter().any(|row| matches!(row.payload, DiagnosticPayload::Gap { cause: GapCause::CaptureOverflow, dropped_frames } if dropped_frames == (10 - sent.len()) as u64)));
}

#[tokio::test]
async fn repair_two_real_boundaries_during_replacement_advance_each_epoch() {
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
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "first epoch"),
            }),
            FixtureStep::Emit(ProviderEvent::Gap {
                metadata: metadata(),
            }),
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "second epoch"),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let close = h.audio.delay_close("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    eventually(|| h.store.transcript_page(h.id, None, 50).unwrap().rows.len() == 2).await;
    let epochs = h
        .store
        .transcript_page(h.id, None, 50)
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.details.unwrap().stream_epoch)
        .collect::<Vec<_>>();
    let _ = close.send(());
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Running).await;
    let result = run.stop().await;
    assert_eq!(epochs, [1, 2]);
    assert_eq!(h.audio.total_opens(), 2);
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert_eq!(result, RuntimeOutcome::Completed);
}

#[tokio::test]
async fn repair_provider_failure_during_pending_replacement_settles_late_open() {
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
            FixtureStep::Delay(Duration::from_millis(60)),
            FixtureStep::Disconnect,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let open = h.audio.delay_open("live-input");
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Reconnecting).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let phase_before_release = run.status().borrow().directions[0].phase.clone();
    let _ = open.send(Ok(()));
    wait_phase(&run, RuntimePhase::Paused).await;
    let result = run.stop().await;
    assert!(
        matches!(result,RuntimeOutcome::Failed { codes } if codes.contains(&RuntimeFailureCode::Provider))
    );
    assert_eq!(
        phase_before_release,
        DirectionPhase::Reconnecting,
        "failure published before replacement was settled"
    );
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(h.audio.total_opens(), 2);
    assert_eq!(h.provider.connect_counts(), [1, 0]);
}

#[tokio::test]
async fn repair_replacement_close_failure_prevents_new_open_and_retains_accounting() {
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
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let close = h.audio.delay_close_result("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Reconnecting).await;
    let _ = close.send(Err(RuntimeFailureCode::DeviceUnavailable));
    wait_phase(&run, RuntimePhase::Paused).await;
    let result = run.stop().await;
    assert!(
        matches!(result,RuntimeOutcome::Failed { codes } if codes.contains(&RuntimeFailureCode::DeviceUnavailable))
    );
    assert_eq!(h.audio.total_opens(), 1);
    assert_eq!(
        h.audio.open_calls(),
        1,
        "failed close must not attempt a replacement open"
    );
    assert_eq!(
        h.audio.open_ports(),
        1,
        "failed acknowledgement must keep quarantine"
    );
    assert!(!h.audio.playback_statistics("m1-render").unwrap().finalized);
}

#[tokio::test]
async fn repair_early_stop_observes_failure_after_open_deadline_inside_rollback_allowance() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let open = h.audio.delay_open("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    eventually(|| h.provider.connect_counts()[0] == 1).await;
    let stopped = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    tokio::time::sleep(Duration::from_millis(3200)).await;
    let remained_owned = !stopped.is_finished();
    let _ = open.send(Err(RuntimeFailureCode::DeviceUnavailable));
    let outcome = timeout(Duration::from_secs(2), stopped)
        .await
        .unwrap()
        .unwrap();
    assert!(
        remained_owned,
        "early stop truncated the remaining open plus rollback budget"
    );
    assert_eq!(
        outcome,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::DeviceUnavailable]
        }
    );
}

#[tokio::test]
async fn repair_two_provider_failures_in_distinct_epochs_are_both_persisted() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(30)),
            FixtureStep::Emit(ProviderEvent::Failed {
                metadata: metadata(),
                error: ProviderError::RemoteFailure { code: 42 },
            }),
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    wait_phase(&run, RuntimePhase::Paused).await;
    run.resume_direction(SessionDirection::SourceToTarget)
        .await
        .unwrap();
    wait_phase(&run, RuntimePhase::Paused).await;
    let result = run.stop().await;
    let rows = h.store.diagnostic_page(h.id, None, 100).unwrap().rows;
    let epochs = rows
        .iter()
        .filter_map(|row| match row.payload {
            DiagnosticPayload::Failure {
                code: RuntimeFailureCode::Provider,
                provider_code: Some(42),
                ..
            } => row.stream_epoch,
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(epochs, [0, 1]);
    assert_eq!(
        result,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Provider]
        }
    );
}

async fn check_capture_replacement_preserves_peer_and_accounting(local_backlog: bool) {
    let output = |sample| ProviderEvent::Audio {
        metadata: metadata(),
        frame: AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![sample; 1920],
        )
        .unwrap(),
    };
    let forward = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(output(5)),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(output(9)),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let peer = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(output(4)),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(forward, peer).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    eventually(|| {
        h.audio
            .playback_statistics("m1-render")
            .is_some_and(|s| s.accepted_for_processing_samples == 1920)
    })
    .await;
    let open = h.audio.delay_open("a-input");
    let send = h.provider.delay_send(SessionDirection::SourceToTarget);
    if local_backlog {
        h.audio
            .send_capture("a-input", capture(0, 16000, 2))
            .await
            .unwrap();
        assert!(
            timeout(Duration::from_secs(1), send.wait_entered())
                .await
                .unwrap()
        );
        h.audio
            .send_capture("a-input", capture(1, 8000, 3))
            .await
            .unwrap();
    } else {
        h.audio
            .send_capture("a-input", capture(0, 640, 2))
            .await
            .unwrap();
        tokio::task::yield_now().await;
        h.audio.inject_capture_loss("a-input", 3, 2);
        h.audio
            .send_capture("a-input", capture(1, 640, 3))
            .await
            .unwrap();
    }
    let recovering = timeout(Duration::from_secs(1), async {
        while run.status().borrow().directions[0].phase != DirectionPhase::Reconnecting {
            tokio::task::yield_now().await;
        }
    })
    .await;
    // Release every barrier before assertions, including the RED path.
    let peer_audio = timeout(
        Duration::from_secs(1),
        h.audio.next_playback("physical-output"),
    )
    .await
    .unwrap()
    .unwrap();
    let _ = open.send(Ok(()));
    send.release();
    if recovering.is_err() {
        let _ = timeout(Duration::from_secs(2), run.stop()).await;
    }
    assert!(recovering.is_ok(), "capture loss did not enter replacement");
    eventually(|| {
        h.audio.total_opens() == 3
            && run.status().borrow().directions[0].phase == DirectionPhase::Running
    })
    .await;
    h.audio
        .send_capture("a-input", capture(2, 640, 9))
        .await
        .unwrap();
    h.audio
        .send_capture("a-input", capture(3, 640, 9))
        .await
        .unwrap();
    let fresh = timeout(Duration::from_secs(1), h.audio.next_playback("m1-render"))
        .await
        .unwrap()
        .unwrap();
    let result = run.stop().await;
    assert_eq!(fresh.samples(), &[9; 1920]);
    assert_eq!(peer_audio.samples(), &[4; 1920]);
    assert_eq!(
        h.provider.sent_samples(SessionDirection::SourceToTarget),
        vec![vec![9; 1280]]
    );
    assert_eq!(h.provider.connect_counts(), [1, 1]);
    assert_eq!(h.audio.total_opens(), 3);
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(result, RuntimeOutcome::Completed);
    let rows = h.store.diagnostic_page(h.id, None, 100).unwrap().rows;
    assert!(rows.iter().any(|r|matches!(&r.payload,DiagnosticPayload::PlaybackAccounting {statistics,..} if statistics.finalized && statistics.accepted_for_processing_samples==1920 && statistics.native.as_ref().is_some_and(|n|n.discarded_mono_samples==1920))));
    if local_backlog {
        assert!(rows.iter().any(|r|matches!(&r.payload,DiagnosticPayload::CaptureAccounting {statistics} if statistics.finalized && statistics.dropped_native_blocks==0 && statistics.dropped_normalized_frames==0)));
    } else {
        assert!(rows.iter().any(|r|matches!(&r.payload,DiagnosticPayload::CaptureAccounting {statistics} if statistics.finalized && statistics.dropped_native_blocks==3 && statistics.dropped_normalized_frames==2)));
        assert!(rows.iter().any(|r| matches!(
            r.payload,
            DiagnosticPayload::Gap {
                cause: GapCause::CaptureOverflow,
                dropped_frames: 2
            }
        )));
    }
}

#[tokio::test]
async fn repair_runtime_backlog_overflow_clears_stale_audio_and_keeps_peer() {
    check_capture_replacement_preserves_peer_and_accounting(true).await;
}
#[tokio::test]
async fn repair_native_loss_keeps_units_and_flushes_old_port_discard_accounting() {
    check_capture_replacement_preserves_peer_and_accounting(false).await;
}

#[tokio::test]
async fn repair_startup_stop_retains_failed_close_and_missing_terminal_ack() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let started = h.provider.delay_started(SessionDirection::SourceToTarget);
    let terminal = h.provider.delay_terminal(SessionDirection::SourceToTarget);
    let close = h.audio.delay_close_result("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    assert!(
        timeout(Duration::from_secs(1), started.wait_entered())
            .await
            .unwrap()
    );
    let stop = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    started.release();
    let _ = close.send(Err(RuntimeFailureCode::DeviceUnavailable));
    let result = timeout(Duration::from_secs(2), stop).await;
    terminal.release();
    let outcome = result.unwrap().unwrap();
    assert!(
        matches!(outcome,RuntimeOutcome::Failed {codes} if codes.contains(&RuntimeFailureCode::DeviceUnavailable) && codes.contains(&RuntimeFailureCode::CleanupTimeout))
    );
    assert_eq!(h.audio.open_ports(), 1);
    let statistics = h.audio.playback_statistics("m1-render").unwrap();
    assert!(!statistics.finalized && !statistics.tail_disposition_known);
}

#[tokio::test]
async fn repair_sink_construction_failure_is_a_storage_outcome() {
    use crate::{
        config::SecretStore, providers::factory::ProviderFactory, session::TranslationRuntime,
        storage::LocalStore,
    };
    use std::sync::Arc;
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    let factory = Arc::new(
        ProviderFactory::new(Arc::new(SecretStore::for_tests(dir.path()).unwrap()))
            .with_test_provider(h.provider.clone()),
    );
    let runtime = TranslationRuntime::new(factory, h.audio.clone(), store, h.bus.clone());
    let prepared = runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap();
    rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite"))
        .unwrap()
        .execute_batch("DROP TABLE translation_segments")
        .unwrap();
    let run = runtime.start(prepared);
    assert_eq!(
        timeout(Duration::from_secs(1), run.stop()).await.unwrap(),
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Storage]
        }
    );
    assert_eq!(h.provider.connect_counts(), [0, 0]);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_output_overflow_coalesces_one_replacement_and_discards_stale_carry() {
    let output = |sample| ProviderEvent::Audio {
        metadata: metadata(),
        frame: AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![sample; 1920],
        )
        .unwrap(),
    };
    let forward = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(60)),
            FixtureStep::EmitRepeated {
                event: output(2),
                count: 24,
            },
            FixtureStep::Emit(ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "old output drained"),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(output(9)),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let peer = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(output(4)),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(forward, peer).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let open = h.audio.delay_open("a-input");
    h.audio
        .send_capture("a-input", capture(0, 640, 2))
        .await
        .unwrap();
    eventually(|| h.store.transcript_page(h.id, None, 50).unwrap().rows.len() == 1).await;
    assert_eq!(
        run.status().borrow().directions[0].phase,
        DirectionPhase::Reconnecting
    );
    assert_eq!(
        h.audio
            .next_playback("physical-output")
            .await
            .unwrap()
            .samples(),
        &[4; 1920]
    );
    let _ = open.send(Ok(()));
    eventually(|| {
        h.audio.total_opens() == 3
            && run.status().borrow().directions[0].phase == DirectionPhase::Running
    })
    .await;
    h.audio
        .send_capture("a-input", capture(1, 640, 9))
        .await
        .unwrap();
    h.audio
        .send_capture("a-input", capture(2, 640, 9))
        .await
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), h.audio.next_playback("m1-render"))
            .await
            .unwrap()
            .unwrap()
            .samples(),
        &[9; 1920]
    );
    assert_eq!(run.stop().await, RuntimeOutcome::Completed);
    assert_eq!(
        h.provider.sent_samples(SessionDirection::SourceToTarget),
        vec![vec![9; 1280]]
    );
    assert_eq!(h.provider.connect_counts(), [1, 1]);
    assert_eq!(h.audio.total_opens(), 3);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_terminal_during_delayed_old_close_prevents_late_reopen() {
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
            FixtureStep::Delay(Duration::from_millis(40)),
            FixtureStep::Disconnect,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let close = h.audio.delay_close("live-input");
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    eventually(|| run.status().borrow().directions[0].phase == DirectionPhase::Reconnecting).await;
    tokio::time::sleep(Duration::from_millis(80)).await;
    let _ = close.send(());
    wait_phase(&run, RuntimePhase::Paused).await;
    let _ = run.stop().await;
    // Let a wrongly detached replacement attempt finish so this assertion catches it.
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(
        h.audio.total_opens(),
        1,
        "provider terminal must prevent pending replacement reopening after old close"
    );
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test]
async fn repair_stop_during_resumed_handshake_without_inventory_change() {
    let script = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Delay(Duration::from_millis(100)),
            FixtureStep::Disconnect,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::live(script).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    wait_phase(&run, RuntimePhase::Paused).await;
    let started = h.provider.delay_started(SessionDirection::SourceToTarget);
    let resumed = tokio::spawn({
        let run = run.clone();
        async move { run.resume_direction(SessionDirection::SourceToTarget).await }
    });
    assert!(
        timeout(Duration::from_secs(1), started.wait_entered())
            .await
            .unwrap()
    );
    let stopped = tokio::spawn({
        let run = run.clone();
        async move { run.stop().await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    started.release();
    let outcome = timeout(Duration::from_secs(2), stopped).await;
    let _ = timeout(Duration::from_secs(2), resumed).await;
    assert!(
        outcome.is_ok(),
        "resume consumed the only stop notification"
    );
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(h.provider.connect_counts(), [2, 0]);
}

#[tokio::test]
async fn rereview_recovery_accounting_storage_failure_must_not_resume() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let mut events = h.bus.subscribe(h.id);
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    h.store.delete_session(h.id).unwrap();
    h.audio.inject_capture_loss("live-input", 1, 0);
    h.audio
        .send_capture("live-input", capture(0, 1280, 1))
        .await
        .unwrap();
    timeout(Duration::from_millis(300), async {
        loop {
            if matches!(events.recv().await.unwrap(), SessionEvent::Diagnostic { diagnostic }
                if matches!(diagnostic.payload, DiagnosticPayload::Failure { code: RuntimeFailureCode::Storage, .. })) { break; }
        }
    }).await.unwrap();
    let returned_to_running = timeout(Duration::from_millis(300), async {
        loop {
            let snapshot = run.status().borrow().clone();
            if h.audio.total_opens() == 2 && snapshot.directions[0].phase == DirectionPhase::Running
            {
                break true;
            }
            if snapshot.directions[0].phase == DirectionPhase::Paused {
                break false;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let outcome = timeout(Duration::from_secs(2), run.stop()).await.unwrap();
    assert_eq!(h.audio.open_ports(), 0);
    assert!(
        matches!(outcome, RuntimeOutcome::Failed { codes } if codes.contains(&RuntimeFailureCode::Storage))
    );
    assert!(
        !returned_to_running,
        "replacement activated and published Running after old-port accounting persistence failed"
    );
}

#[tokio::test]
async fn fix2_retired_accounting_failure_cleans_new_ports_and_preserves_call_peer() {
    let peer = FixtureScript::with_output_audio(
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
                    vec![9; 1920],
                )
                .unwrap(),
            }),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(ready_script(), peer).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let mut events = h.bus.subscribe(h.id);
    h.fail_forward_diagnostic_writes();
    h.audio.inject_capture_loss("a-input", 1, 0);
    h.audio
        .send_capture("a-input", capture(0, 1280, 7))
        .await
        .unwrap();
    timeout(Duration::from_millis(300), async {
        loop {
            if matches!(events.recv().await.unwrap(), SessionEvent::Diagnostic {diagnostic}
                if matches!(diagnostic.payload,DiagnosticPayload::Failure {code:RuntimeFailureCode::Storage,..})) { break; }
        }
    }).await.unwrap();
    let resumed = timeout(Duration::from_millis(300), async {
        loop {
            let phase = run.status().borrow().directions[0].phase.clone();
            if h.audio.total_opens() == 3 && phase == DirectionPhase::Running {
                break true;
            }
            if phase == DirectionPhase::Paused {
                break false;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    if resumed {
        let _ = timeout(Duration::from_secs(2), run.stop()).await;
    }
    assert!(
        !resumed,
        "failed retired accounting admitted a new Running transition"
    );
    assert_eq!(h.audio.total_opens(), 3);
    assert_eq!(
        h.audio.open_ports(),
        1,
        "failed accounting must settle fresh ports before Paused"
    );
    assert!(
        h.audio
            .send_capture("a-input", capture(1, 1280, 8))
            .await
            .is_err()
    );
    h.audio
        .send_capture(
            "m2-capture",
            AudioFrame::new(
                AudioSource::System,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![5; 1280],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let played = timeout(
        Duration::from_secs(1),
        h.audio.next_playback("physical-output"),
    )
    .await
    .unwrap()
    .unwrap();
    let peer_phase = run.status().borrow().directions[1].phase.clone();
    let outcome = timeout(Duration::from_secs(2), run.stop()).await.unwrap();
    assert_eq!(played.samples(), &[9; 1920]);
    assert_eq!(peer_phase, DirectionPhase::Running);
    assert_eq!(h.provider.connect_counts(), [1, 1]);
    assert!(
        h.provider
            .sent_samples(SessionDirection::SourceToTarget)
            .is_empty()
    );
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(
        outcome,
        RuntimeOutcome::Failed {
            codes: vec![RuntimeFailureCode::Storage]
        }
    );
}

#[tokio::test]
async fn rereview_fix2_final_accounting_failure_preserves_pending_latency() {
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
    h.fail_final_capture_accounting_writes();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let captured = h
        .audio
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
        .await;
    let played = timeout(Duration::from_secs(1), h.audio.next_playback("m1-render")).await;
    // Settle the runtime before inspecting playback or durability assertions.
    let outcome = timeout(Duration::from_secs(2), run.stop()).await.unwrap();
    assert_eq!(h.audio.open_ports(), 0);
    captured.unwrap();
    assert_eq!(played.unwrap().unwrap().samples()[0], 6);
    assert!(
        matches!(outcome, RuntimeOutcome::Failed { codes } if codes.contains(&RuntimeFailureCode::Storage))
    );
    let diagnostics = h.store.diagnostic_page(h.id, None, 50).unwrap().rows;
    assert!(diagnostics.iter().any(|row| matches!(
        row.payload,
        DiagnosticPayload::Latency {
            capture_to_send_ms: Some(_),
            ..
        }
    )));
}

#[tokio::test]
async fn final_fix_invalid_transcript_and_sql_failure_keep_distinct_actor_categories() {
    for (language, expected) in [
        ("wrong-language", RuntimeFailureCode::InvalidEvent),
        ("zh", RuntimeFailureCode::Storage),
    ] {
        let script = FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::ReceiveAudio { count: 1 },
                FixtureStep::Emit(ProviderEvent::TranslationFinal {
                    metadata: metadata(),
                    text: final_text(language, "must not persist"),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap();
        let h = RuntimeHarness::live(script).unwrap();
        if expected == RuntimeFailureCode::Storage {
            h.fail_transcript_writes();
        }
        let run = h
            .runtime
            .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
        wait_phase(&run, RuntimePhase::Running).await;
        h.audio
            .send_capture("live-input", capture(0, 1280, 1))
            .await
            .unwrap();
        wait_phase(&run, RuntimePhase::Paused).await;
        assert_eq!(
            run.status().borrow().directions[0].failure,
            Some(expected.clone())
        );
        assert!(
            h.store
                .transcript_page(h.id, None, 50)
                .unwrap()
                .rows
                .is_empty()
        );
        assert_eq!(
            run.stop().await,
            RuntimeOutcome::Failed {
                codes: vec![expected]
            }
        );
    }
}

struct NativeRingBackend {
    inventory: std::sync::Arc<super::test_support::FixtureAudioBackend>,
    native: std::sync::Arc<crate::audio::playback::test_support::NativePlaybackFixture>,
}
#[async_trait::async_trait]
impl crate::audio::runtime::AudioBackend for NativeRingBackend {
    async fn inventory(
        &self,
    ) -> crate::error::AppResult<crate::audio::platform::inventory::AudioInventory> {
        self.inventory.inventory().await
    }
    async fn open(
        &self,
        request: crate::audio::runtime::AudioOpenRequest,
    ) -> crate::error::AppResult<crate::audio::runtime::AudioPorts> {
        crate::audio::platform::native_io::open_with_factory(request, self.native.clone()).await
    }
}

#[tokio::test]
async fn final_fix_native_ring_overflow_replaces_only_affected_route_and_clears_carry() {
    use crate::{
        audio::playback::test_support::NativePlaybackFixture, config::SecretStore,
        providers::factory::ProviderFactory,
    };
    use std::sync::Arc;
    let output = |sample| ProviderEvent::Audio {
        metadata: metadata(),
        frame: AudioFrame::new(
            AudioSource::Microphone,
            0,
            PcmSpec::pcm16_mono(24_000),
            vec![sample; 1920],
        )
        .unwrap(),
    };
    let forward = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Delay(Duration::from_millis(100)),
            // A single event splits into one router frame, far below native ingress capacity.
            FixtureStep::Emit(output(2)),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(output(9)),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let peer = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(ProviderEvent::SessionStarted {
                metadata: metadata(),
            }),
            FixtureStep::Emit(output(4)),
            FixtureStep::ReceiveAudio { count: 1 },
            FixtureStep::Emit(output(6)),
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let h = RuntimeHarness::call(forward, peer).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let factory = Arc::new(
        ProviderFactory::new(Arc::new(SecretStore::for_tests(dir.path()).unwrap()))
            .with_test_provider(h.provider.clone()),
    );
    let native = Arc::new(NativePlaybackFixture::default());
    let runtime = super::TranslationRuntime::new(
        factory,
        Arc::new(NativeRingBackend {
            inventory: h.audio.clone(),
            native: native.clone(),
        }),
        h.store.clone(),
        h.bus.clone(),
    );
    let run = runtime.start(runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let old = native.probes("a-input")[0].clone();
    let peer_port = native.probes("m2-capture")[0].clone();
    old.capture.push_sync(capture(0, 1280, 1)).unwrap();
    eventually(|| {
        h.provider
            .sent_sequences(SessionDirection::SourceToTarget)
            .len()
            == 1
    })
    .await;
    old.capture.push_sync(capture(1, 640, 2)).unwrap();
    let reopened = timeout(Duration::from_secs(1), async {
        while native.probes("a-input").len() != 2
            || run.status().borrow().directions[0].phase != DirectionPhase::Running
        {
            tokio::task::yield_now().await;
        }
    })
    .await;
    if reopened.is_err() {
        // Settle the real worker/native owner even on the expected RED path.
        let stopped = run.clone();
        let stop = tokio::spawn(async move { stopped.stop().await });
        while !stop.is_finished() {
            for input in ["a-input", "m2-capture"] {
                for p in native.probes(input) {
                    p.consume();
                }
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let _ = stop.await;
    }
    assert!(
        reopened.is_ok(),
        "native ring loss must reopen audio even when router ingress never overflows"
    );
    assert_eq!(old.pending_samples(), 0);
    assert!(old.capture.pop().await.is_none());
    assert!(old.statistics().native.unwrap().discarded_mono_samples > 0);
    assert!(peer_port.consume().iter().any(|&sample| sample == 4));
    assert_eq!(native.probes("m2-capture").len(), 1);
    assert_eq!(
        run.status().borrow().directions[1].phase,
        DirectionPhase::Running
    );
    assert!(
        run.status()
            .borrow()
            .directions
            .iter()
            .all(|direction| direction.stream_epoch == 0)
    );
    let fresh = native.probes("a-input")[1].clone();
    fresh.capture.push_sync(capture(2, 640, 9)).unwrap();
    fresh.capture.push_sync(capture(3, 640, 9)).unwrap();
    peer_port
        .capture
        .push_sync(
            AudioFrame::new(
                AudioSource::System,
                0,
                PcmSpec::pcm16_mono(16_000),
                vec![6; 1280],
            )
            .unwrap(),
        )
        .unwrap();
    eventually(|| {
        fresh.pending_samples() > 0
            && peer_port.statistics().accepted_for_processing_samples == 3840
    })
    .await;
    let fresh_audio = fresh.consume();
    assert!(fresh_audio.iter().any(|&sample| sample == 9));
    assert!(
        !fresh_audio.contains(&2),
        "retired native output must not leak into the fresh route"
    );
    // Admission accounting precedes conversion; observe real callback output.
    eventually(|| peer_port.consume().contains(&6)).await;
    assert_eq!(
        h.provider.sent_samples(SessionDirection::SourceToTarget),
        vec![vec![1; 1280], vec![9; 1280]]
    );
    assert_eq!(h.provider.connect_counts(), [1, 1]);
    assert_eq!(
        native.probes("a-input").len(),
        2,
        "new port loss baseline must not trigger another replacement"
    );
    let stopped = run.clone();
    let stop = tokio::spawn(async move { stopped.stop().await });
    timeout(Duration::from_secs(2), async {
        while !stop.is_finished() {
            fresh.consume();
            peer_port.consume();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(stop.await.unwrap(), RuntimeOutcome::Completed);
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
                    dropped_frames: 0
                }
            ))
    );
}

#[tokio::test]
async fn final_fix_periodic_playback_statistics_have_unknown_live_boundary() {
    let h = RuntimeHarness::live(ready_script()).unwrap();
    let run = h
        .runtime
        .start(h.runtime.prepare(h.id, &h.launch, &h.saved).await.unwrap());
    wait_phase(&run, RuntimePhase::Running).await;
    let live = timeout(Duration::from_millis(1500), async {
        loop {
            let rows = h.store.diagnostic_page(h.id, None, 100).unwrap().rows;
            if rows.iter().any(|row| matches!(&row.payload, DiagnosticPayload::PlaybackAccounting { boundary: None, statistics } if !statistics.finalized && !statistics.tail_disposition_known)) { break; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await;
    let result = run.stop().await;
    assert!(
        live.is_ok(),
        "live playback accounting must be published before cleanup"
    );
    assert_eq!(result, RuntimeOutcome::Completed);
}
