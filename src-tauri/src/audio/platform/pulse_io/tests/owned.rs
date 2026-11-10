use super::*;
use crate::audio::join::AudioCloseKey;

#[tokio::test]
async fn pulse_actual_close_waits_late_nonzero_exit_after_worker_observer_error() {
    let fixture = Fixture::new();
    let (backend, _, result) = ports(fixture.clone(), "mic", "VoxBridge_M1").await;
    let ports = result.unwrap();
    fixture
        .cleanup_mode
        .store(CLEANUP_ERROR, AtomicOrdering::Release);
    let failed = ports.control.close().await.is_err();
    let key = AudioCloseKey::new(1, Uuid::new_v4(), backend.actual_epoch().unwrap());
    let close = backend.begin_close(key).unwrap();
    let held = tokio::time::timeout(Duration::from_millis(30), close.wait())
        .await
        .is_err();
    fixture.stream_child("parec").exit(7);
    fixture.stream_child("pacat").exit(8);
    close.wait().await.unwrap();
    assert!(failed);
    assert!(
        held,
        "worker errors and EOF cannot acknowledge still-entered Pulse children"
    );
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
}

#[tokio::test]
async fn pulse_failed_open_without_pipes_keeps_actual_children_owned() {
    let fixture = Fixture::new();
    fixture
        .cleanup_mode
        .store(CLEANUP_ERROR, AtomicOrdering::Release);
    let (backend, _, result) =
        failed_open_with_cleanup(fixture.clone(), Duration::from_millis(30)).await;
    let key = AudioCloseKey::new(1, Uuid::new_v4(), backend.actual_epoch().unwrap());
    let close = backend.begin_close(key).unwrap();
    let held = tokio::time::timeout(Duration::from_millis(30), close.wait())
        .await
        .is_err();
    fixture.stream_child("parec").exit(0);
    fixture.stream_child("pacat").exit(-9);
    let ack = close.wait().await.unwrap();
    assert!(result.is_err());
    assert!(
        held,
        "failed acquisition returning no AudioPorts must retain its actual parent"
    );
    assert!(
        backend.reopen_after_close(&ack).is_err(),
        "positive late reap cannot clear native quarantine"
    );
}

#[tokio::test]
async fn pulse_inventory_held_helper_blocks_another_request_and_actual_close() {
    let fixture = Fixture::new();
    fixture
        .cleanup_mode
        .store(CLEANUP_ERROR, AtomicOrdering::Release);
    *fixture.held_helper.lock().unwrap() = Some("modules".into());
    let backend =
        super::super::super::inventory::PlatformAudioBackend::with_spawner(fixture.clone());
    assert!(backend.inventory().await.is_err());
    let _ = tokio::time::timeout(Duration::from_millis(30), backend.inventory()).await;
    let starts = fixture.specs.lock().unwrap().len();
    let key = AudioCloseKey::new(1, Uuid::new_v4(), backend.actual_epoch().unwrap());
    let close = backend.begin_close(key).unwrap();
    let held = tokio::time::timeout(Duration::from_millis(30), close.wait())
        .await
        .is_err();
    for child in fixture.held_children.lock().unwrap().iter() {
        child.exit(0);
    }
    close.wait().await.unwrap();
    assert_eq!(
        starts, 3,
        "an observer timeout must retain the one inventory parent and its three-helper group"
    );
    assert!(
        held,
        "unattached inventory helpers belong to the actual close barrier"
    );
}

#[tokio::test]
async fn pulse_cancelled_monitor_keeps_its_original_helper_after_multiple_ticks() {
    let fixture = Fixture::new();
    let (backend, _, result) = ports(fixture.clone(), "mic", "VoxBridge_M1").await;
    let ports = result.unwrap();
    fixture
        .cleanup_mode
        .store(CLEANUP_ERROR, AtomicOrdering::Release);
    *fixture.held_helper.lock().unwrap() = Some("source-outputs".into());
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.held_children.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(2200)).await;
    let starts = fixture.specs.lock().unwrap().len();
    let failed = ports.control.close().await.is_err();
    let key = AudioCloseKey::new(1, Uuid::new_v4(), backend.actual_epoch().unwrap());
    let close = backend.begin_close(key).unwrap();
    fixture.stream_child("parec").exit(7);
    fixture.stream_child("pacat").exit(8);
    let pending = tokio::time::timeout(Duration::from_millis(30), close.wait())
        .await
        .is_err();
    for child in fixture.held_children.lock().unwrap().iter() {
        child.exit(0);
    }
    close.wait().await.unwrap();
    assert_eq!(
        starts, 9,
        "open and every monitor tick share the same two-helper group"
    );
    assert!(
        failed && pending,
        "joining the monitor and both stream children must not omit its cancelled helper"
    );
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
}

async fn zero_entry_closing_preserves_rearm(boundary: entry_test_support::Boundary) {
    use super::super::super::inventory::{PlatformAudioBackend, actual_test_support};
    use crate::audio::join::AudioCloseJoin;
    let fixture = Fixture::new();
    let backend = Arc::new(PlatformAudioBackend::with_spawner(fixture.clone()));
    let inventory = backend.inventory().await.unwrap();
    let request = AudioOpenRequest {
        inventory_generation: inventory.generation,
        input_id: "pulse:source:mic".into(),
        output_id: "pulse:sink:VoxBridge_M1".into(),
        source: AudioSource::Microphone,
    };
    let before_calls = fixture.spawn_audio_calls.load(AtomicOrdering::SeqCst);
    let before_specs = fixture.specs.lock().unwrap().len();
    let before_children = fixture.children.lock().unwrap().len();
    let (entered, release) = actual_test_support::hold_pulse_open(&backend, boundary);
    let acquired = backend.clone();
    let original_request = request.clone();
    let open = tokio::spawn(async move { acquired.open(original_request).await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(actual_test_support::pair_health(&backend), (2, 0));
    let key = AudioCloseKey::new(1, Uuid::new_v4(), backend.actual_epoch().unwrap());
    let native = backend.begin_close(key.clone()).unwrap();
    let runtime = crate::session::test_support::RuntimeHarness::live(
        crate::providers::fixture::FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [crate::providers::fixture::FixtureStep::WaitForFinish],
        )
        .unwrap(),
    )
    .unwrap();
    let orchestrator = runtime.orchestrator();
    let session = orchestrator.capture_close_join(key.clone()).unwrap();
    let stop = orchestrator.clone();
    let audio =
        AudioCloseJoin::start_owned(Arc::new(()), key.clone(), session, native, async move {
            stop.stop_active().await
        })
        .unwrap();
    let pending = tokio::time::timeout(Duration::from_millis(30), audio.wait())
        .await
        .is_err();
    release.send(()).unwrap();
    let rejected = open.await.unwrap().is_err();
    let ack = tokio::time::timeout(Duration::from_secs(2), audio.wait())
        .await
        .unwrap()
        .unwrap();
    ack.require_key(&key).unwrap();
    assert!(
        pending,
        "the held original acquisition parent must block actual close"
    );
    assert!(rejected);
    assert_eq!(
        fixture.spawn_audio_calls.load(AtomicOrdering::SeqCst),
        before_calls
    );
    assert_eq!(fixture.specs.lock().unwrap().len(), before_specs);
    assert_eq!(fixture.children.lock().unwrap().len(), before_children);
    assert_eq!(
        actual_test_support::pair_health(&backend),
        (0, 0),
        "a proven zero-entry rejection must release its pair without quarantine"
    );
    backend.validate_reopen_after_close(ack.backend()).unwrap();
    orchestrator.validate_reopen_after_close(&key).unwrap();
    backend.reopen_after_close(ack.backend()).unwrap();
    orchestrator.reopen_after_close(&key).unwrap();
    assert_eq!(backend.actual_epoch().unwrap(), key.backend_epoch() + 1);
    let lease =
        super::super::super::inventory::driver_test_support::claim(&backend, &request).unwrap();
    lease.release();
}

#[tokio::test]
async fn pulse_zero_entry_owner_closing_rejection_preserves_pair_health_and_actual_rearm() {
    zero_entry_closing_preserves_rearm(entry_test_support::Boundary::OwnerReady).await;
}

#[tokio::test]
async fn pulse_zero_entry_stream_group_closing_rejection_preserves_pair_health_and_actual_rearm() {
    zero_entry_closing_preserves_rearm(entry_test_support::Boundary::StreamGroup).await;
}
