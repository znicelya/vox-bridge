use super::*;
use crate::domain::{audio::AudioSource, session::SessionMode};
use tokio::time::timeout;

const SHORT: Duration = Duration::from_millis(30);

fn script(events: impl IntoIterator<Item = ProviderEvent>) -> FixtureScript {
    FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        events
            .into_iter()
            .map(FixtureStep::Emit)
            .chain([FixtureStep::WaitForFinish]),
    )
    .unwrap()
}

fn started() -> ProviderEvent {
    ProviderEvent::SessionStarted {
        metadata: metadata(),
    }
}

fn capture(sequence: u64, value: i16) -> AudioFrame {
    AudioFrame::new(
        AudioSource::Microphone,
        sequence,
        PcmSpec::pcm16_mono(16_000),
        vec![value; 1280],
    )
    .unwrap()
}

async fn connect(
    provider: &ObservedFixtureProvider,
    direction: SessionDirection,
) -> ProviderStream {
    provider
        .connect(ProviderSessionConfig {
            session_id: Uuid::new_v4(),
            direction,
            mode: SessionMode::Call,
            languages: if direction == SessionDirection::SourceToTarget {
                LanguagePair::new("en", "zh").unwrap()
            } else {
                LanguagePair::new("zh", "en").unwrap()
            },
            target_audio: PcmSpec::pcm16_mono(24_000),
            provider_config_id: Uuid::new_v4(),
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn dropping_send_release_unblocks_a_pending_send() {
    let provider = ObservedFixtureProvider::new(script([started()]), script([started()]));
    let stream = connect(&provider, SessionDirection::SourceToTarget).await;
    stream.next_event().await.unwrap().unwrap();
    let release = provider.delay_send(SessionDirection::SourceToTarget);
    let mut pending = Box::pin(stream.send_audio(capture(1, 7)));
    assert!(timeout(SHORT, &mut pending).await.is_err());
    drop(release);
    timeout(SHORT, pending).await.unwrap().unwrap();
    assert_eq!(
        provider.sent_sequences(SessionDirection::SourceToTarget),
        vec![1]
    );
    stream.cancel().await.unwrap();
}

#[tokio::test]
async fn dropping_gap_release_delivers_the_retained_gap() {
    let provider = ObservedFixtureProvider::new(
        script([
            started(),
            ProviderEvent::Gap {
                metadata: metadata(),
            },
        ]),
        script([started()]),
    );
    let stream = connect(&provider, SessionDirection::SourceToTarget).await;
    stream.next_event().await.unwrap().unwrap();
    let release = provider.delay_gap(SessionDirection::SourceToTarget);
    let mut pending = Box::pin(stream.next_event());
    assert!(timeout(SHORT, &mut pending).await.is_err());
    drop(release);
    assert!(matches!(
        timeout(SHORT, pending).await.unwrap().unwrap(),
        Some(ProviderEvent::Gap { .. })
    ));
    stream.cancel().await.unwrap();
}

#[tokio::test]
async fn dropping_terminal_release_delivers_the_retained_terminal() {
    let provider = ObservedFixtureProvider::new(
        script([
            started(),
            ProviderEvent::Finished {
                metadata: metadata(),
            },
        ]),
        script([started()]),
    );
    let stream = connect(&provider, SessionDirection::SourceToTarget).await;
    stream.next_event().await.unwrap().unwrap();
    let release = provider.delay_terminal(SessionDirection::SourceToTarget);
    let mut pending = Box::pin(stream.next_event());
    assert!(timeout(SHORT, &mut pending).await.is_err());
    drop(release);
    assert!(matches!(
        timeout(SHORT, pending).await.unwrap().unwrap(),
        Some(ProviderEvent::Finished { .. })
    ));
}

#[tokio::test]
async fn an_older_release_does_not_unblock_a_newer_hold() {
    let provider = ObservedFixtureProvider::new(script([started()]), script([started()]));
    let stream = connect(&provider, SessionDirection::SourceToTarget).await;
    stream.next_event().await.unwrap().unwrap();
    let older = provider.delay_send(SessionDirection::SourceToTarget);
    let current = provider.delay_send(SessionDirection::SourceToTarget);
    let mut pending = Box::pin(stream.send_audio(capture(1, 7)));
    assert!(timeout(SHORT, &mut pending).await.is_err());
    assert!(!timeout(SHORT, older.wait_entered()).await.unwrap());
    older.release();
    assert!(timeout(SHORT, &mut pending).await.is_err());
    current.release();
    timeout(SHORT, pending).await.unwrap().unwrap();
    stream.cancel().await.unwrap();
}

#[tokio::test]
async fn finish_and_cancel_holds_are_independent_for_each_direction() {
    let provider = ObservedFixtureProvider::new(script([started()]), script([started()]));
    let forward: Arc<dyn ProviderConnection> =
        Arc::from(connect(&provider, SessionDirection::SourceToTarget).await);
    let reverse: Arc<dyn ProviderConnection> =
        Arc::from(connect(&provider, SessionDirection::TargetToSource).await);
    forward.next_event().await.unwrap().unwrap();
    reverse.next_event().await.unwrap().unwrap();
    let finish_forward = provider.delay_finish(SessionDirection::SourceToTarget);
    let finish_reverse = provider.delay_finish(SessionDirection::TargetToSource);
    let cancel_forward = provider.delay_cancel(SessionDirection::SourceToTarget);
    let cancel_reverse = provider.delay_cancel(SessionDirection::TargetToSource);
    let forward_finish = forward.clone();
    let reverse_finish = reverse.clone();
    let forward_cancel = forward.clone();
    let reverse_cancel = reverse.clone();
    let mut finishing_forward = tokio::spawn(async move { forward_finish.finish().await });
    let mut finishing_reverse = tokio::spawn(async move { reverse_finish.finish().await });
    let mut cancelling_forward = tokio::spawn(async move { forward_cancel.cancel().await });
    let mut cancelling_reverse = tokio::spawn(async move { reverse_cancel.cancel().await });
    assert!(timeout(SHORT, &mut finishing_forward).await.is_err());
    for gate in [
        &finish_forward,
        &finish_reverse,
        &cancel_forward,
        &cancel_reverse,
    ] {
        assert!(timeout(SHORT, gate.wait_entered()).await.unwrap());
    }
    assert_eq!(provider.finish_counts(), [1, 1]);
    assert_eq!(provider.cancel_counts(), [1, 1]);
    finish_forward.release();
    timeout(SHORT, finishing_forward)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(timeout(SHORT, &mut finishing_reverse).await.is_err());
    assert!(timeout(SHORT, &mut cancelling_forward).await.is_err());
    assert!(timeout(SHORT, &mut cancelling_reverse).await.is_err());
    drop(cancel_reverse);
    timeout(SHORT, cancelling_reverse)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(finish_reverse);
    drop(cancel_forward);
    timeout(SHORT, finishing_reverse)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    timeout(SHORT, cancelling_forward)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn final_and_started_delivery_holds_preserve_order_and_call_peer_delivery() {
    let provider = ObservedFixtureProvider::new(
        script([
            started(),
            ProviderEvent::SourceFinal {
                metadata: metadata(),
                text: final_text("en", "source"),
            },
            ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("zh", "translation"),
            },
            ProviderEvent::Gap {
                metadata: metadata(),
            },
            started(),
        ]),
        script([
            started(),
            ProviderEvent::TranslationFinal {
                metadata: metadata(),
                text: final_text("en", "peer"),
            },
        ]),
    );
    let hold_final = provider.delay_final(SessionDirection::SourceToTarget);
    let hold_peer_started = provider.delay_started(SessionDirection::TargetToSource);
    let forward = connect(&provider, SessionDirection::SourceToTarget).await;
    let reverse = connect(&provider, SessionDirection::TargetToSource).await;
    assert!(matches!(
        forward.next_event().await.unwrap(),
        Some(ProviderEvent::SessionStarted { .. })
    ));
    let hold_forward_started = provider.delay_started(SessionDirection::SourceToTarget);
    let mut forward_event = Box::pin(forward.next_event());
    let mut peer_event = Box::pin(reverse.next_event());
    assert!(timeout(SHORT, &mut forward_event).await.is_err());
    assert!(timeout(SHORT, &mut peer_event).await.is_err());
    assert!(timeout(SHORT, hold_final.wait_entered()).await.unwrap());
    assert!(
        timeout(SHORT, hold_peer_started.wait_entered())
            .await
            .unwrap()
    );
    drop(hold_peer_started);
    assert!(matches!(
        timeout(SHORT, peer_event).await.unwrap().unwrap(),
        Some(ProviderEvent::SessionStarted { .. })
    ));
    assert!(
        matches!(reverse.next_event().await.unwrap(), Some(ProviderEvent::TranslationFinal { text, .. }) if text.translation.text == "peer")
    );
    assert!(timeout(SHORT, &mut forward_event).await.is_err());
    drop(hold_final);
    assert!(
        matches!(timeout(SHORT, forward_event).await.unwrap().unwrap(), Some(ProviderEvent::SourceFinal { text, .. }) if text.translation.text == "source")
    );
    assert!(
        matches!(forward.next_event().await.unwrap(), Some(ProviderEvent::TranslationFinal { text, .. }) if text.translation.text == "translation")
    );
    assert!(matches!(
        forward.next_event().await.unwrap(),
        Some(ProviderEvent::Gap { .. })
    ));
    let mut forward_started = Box::pin(forward.next_event());
    assert!(timeout(SHORT, &mut forward_started).await.is_err());
    assert!(
        timeout(SHORT, hold_forward_started.wait_entered())
            .await
            .unwrap()
    );
    hold_forward_started.release();
    assert!(matches!(
        timeout(SHORT, forward_started).await.unwrap().unwrap(),
        Some(ProviderEvent::SessionStarted { .. })
    ));
    forward.cancel().await.unwrap();
    reverse.cancel().await.unwrap();
}

#[tokio::test]
async fn send_entry_counts_include_held_and_rejected_sends_without_recording_rejected_samples() {
    let provider = ObservedFixtureProvider::new(script([started()]), script([started()]));
    let stream = connect(&provider, SessionDirection::SourceToTarget).await;
    stream.next_event().await.unwrap().unwrap();
    provider.queue_full_once(SessionDirection::SourceToTarget);
    let release = provider.delay_send(SessionDirection::SourceToTarget);
    let mut pending = Box::pin(stream.send_audio(capture(1, 7)));
    assert!(timeout(SHORT, &mut pending).await.is_err());
    assert!(timeout(SHORT, release.wait_entered()).await.unwrap());
    assert_eq!(provider.send_calls(), [1, 0]);
    drop(release);
    assert_eq!(
        timeout(SHORT, pending).await.unwrap(),
        Err(crate::providers::ProviderError::QueueFull)
    );
    assert!(
        provider
            .sent_samples(SessionDirection::SourceToTarget)
            .is_empty()
    );
    stream.send_audio(capture(2, 8)).await.unwrap();
    assert_eq!(provider.send_calls(), [2, 0]);
    assert_eq!(
        provider.sent_samples(SessionDirection::SourceToTarget),
        vec![vec![8; 1280]]
    );
    stream.cancel().await.unwrap();
}

#[tokio::test]
async fn successful_sample_observations_keep_only_the_latest_64_frames_per_direction() {
    let receiving = FixtureScript::with_output_audio(
        PcmSpec::pcm16_mono(24_000),
        [
            FixtureStep::Emit(started()),
            FixtureStep::ReceiveAudio { count: 70 },
            FixtureStep::WaitForFinish,
        ],
    )
    .unwrap();
    let provider = ObservedFixtureProvider::new(receiving, script([started()]));
    let forward = connect(&provider, SessionDirection::SourceToTarget).await;
    let reverse = connect(&provider, SessionDirection::TargetToSource).await;
    forward.next_event().await.unwrap().unwrap();
    reverse.next_event().await.unwrap().unwrap();
    for sequence in 0..70 {
        forward
            .send_audio(capture(sequence, sequence as i16 + 101))
            .await
            .unwrap();
        tokio::task::yield_now().await;
    }
    reverse.send_audio(capture(1, -9)).await.unwrap();
    let samples = provider.sent_samples(SessionDirection::SourceToTarget);
    assert_eq!(samples.len(), 64);
    assert_eq!(samples.first().unwrap(), &vec![107; 1280]);
    assert_eq!(samples.last().unwrap(), &vec![170; 1280]);
    assert_eq!(
        provider.sent_samples(SessionDirection::TargetToSource),
        vec![vec![-9; 1280]]
    );
    assert_eq!(provider.send_calls(), [70, 1]);
    forward.cancel().await.unwrap();
    reverse.cancel().await.unwrap();
}
