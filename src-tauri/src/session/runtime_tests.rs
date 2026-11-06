use std::time::Duration;

use tokio::time::timeout;

use crate::{
    audio::{AudioFrame, PcmSpec},
    domain::{
        audio::AudioSource,
        launch::{DirectionPhase, RuntimeOutcome, RuntimePhase},
        session::SessionDirection,
    },
    providers::{
        ProviderEvent,
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
}
