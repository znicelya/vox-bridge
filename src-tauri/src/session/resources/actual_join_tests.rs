use super::*;
use crate::{
    audio::{PcmSpec, runtime::AudioBackend},
    domain::audio::AudioSource,
    providers::fixture::{FixtureScript, FixtureStep},
    session::test_support::{RuntimeHarness, metadata},
};

#[tokio::test]
async fn replacement_waits_for_the_original_actual_parent_after_a_successful_close_reply() {
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
    let inventory = h.audio.inventory().await.unwrap();
    let request = AudioOpenRequest {
        inventory_generation: inventory.generation,
        input_id: "live-input".into(),
        output_id: "m1-render".into(),
        source: AudioSource::Microphone,
    };
    let (joined_workers, release) = h.audio.delay_actual_join("live-input");
    let old = h.audio.open(request.clone()).await.unwrap();
    let original = old.control.actual_join_port().unwrap();
    let mut replacement = AudioOperation::replace(h.audio.clone(), request, old);
    tokio::time::timeout(Duration::from_secs(1), joined_workers)
        .await
        .unwrap()
        .unwrap();
    let early = tokio::time::timeout(Duration::from_millis(30), replacement.join()).await;
    let pending = early.is_err();
    let opens_before_release = h.audio.open_calls();
    let _ = release.send(());
    let acquired = match early {
        Ok(acquired) => acquired,
        Err(_) => replacement.join().await,
    };
    original.wait().await.unwrap();
    if let Some(ports) = &acquired.ports {
        let actual = ports.control.actual_join_port().unwrap();
        let cleanup = close_audio(ports.control.clone(), true).await;
        actual.wait().await.unwrap();
        assert!(cleanup.failures.is_empty());
    }
    assert!(
        pending,
        "a close reply does not join the original native parent"
    );
    assert_eq!(
        opens_before_release, 1,
        "replacement must not consume a second actual stream slot before its original parent joins"
    );
    assert!(acquired.ports.is_some());
    assert!(acquired.report.failures.is_empty());
}
