use super::*;
use crate::domain::audio::AudioSource;
use tokio::time::timeout;

const SHORT: Duration = Duration::from_millis(30);

fn frame(sequence: u64, samples: usize) -> AudioFrame {
    AudioFrame::new(
        AudioSource::System,
        sequence,
        PcmSpec::pcm16_mono(24_000),
        vec![sequence as i16; samples],
    )
    .unwrap()
}

async fn request(backend: &FixtureAudioBackend) -> AudioOpenRequest {
    AudioOpenRequest {
        inventory_generation: backend.inventory().await.unwrap().generation,
        input_id: "live-input".into(),
        output_id: "m1-render".into(),
        source: AudioSource::Microphone,
    }
}

async fn open(backend: &FixtureAudioBackend) -> AudioPorts {
    backend.open(request(backend).await).await.unwrap()
}

#[tokio::test]
async fn suppression_hides_queued_playback_before_async_cleanup() {
    let backend = FixtureAudioBackend::new();
    let ports = open(&backend).await;
    ports.playback.send(frame(1, 320)).await.unwrap();
    ports.control.suppress_now();
    assert_eq!(
        timeout(SHORT, backend.next_playback("m1-render"))
            .await
            .unwrap(),
        None
    );
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn discard_permanently_closes_playback_admission() {
    let backend = FixtureAudioBackend::new();
    let ports = open(&backend).await;
    ports.playback.send(frame(1, 320)).await.unwrap();
    ports.control.discard_pending().await.unwrap();
    assert!(ports.playback.send(frame(2, 160)).await.is_err());
    assert_eq!(
        timeout(SHORT, backend.next_playback("m1-render"))
            .await
            .unwrap(),
        None
    );
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn drain_seals_admission_and_counts_callback_consumed_samples() {
    let backend = FixtureAudioBackend::new();
    let ports = open(&backend).await;
    ports.playback.send(frame(1, 320)).await.unwrap();
    ports.playback.send(frame(2, 160)).await.unwrap();
    let control = ports.control.clone();
    let mut drain = tokio::spawn(async move { control.drain_playback().await });
    assert!(timeout(SHORT, &mut drain).await.is_err());
    assert!(ports.playback.try_send(frame(3, 80)).is_err());
    assert_eq!(
        backend.next_playback("m1-render").await.unwrap().samples(),
        &[1; 320]
    );
    assert!(timeout(SHORT, &mut drain).await.is_err());
    assert_eq!(
        backend.next_playback("m1-render").await.unwrap().samples(),
        &[2; 160]
    );
    let report = timeout(SHORT, drain).await.unwrap().unwrap().unwrap();
    assert_eq!(report.boundary, PlaybackBoundary::CallbackConsumed);
    assert_eq!(report.statistics.accepted_for_processing_samples, 480);
    assert_eq!(report.statistics.discarded_before_conversion_samples, 0);
    let native = report.statistics.native.unwrap();
    assert_eq!(native.sample_rate_hz, 24_000);
    assert_eq!(native.generated_mono_samples, 480);
    assert_eq!(native.callback_consumed_mono_samples, 480);
    assert_eq!(native.discarded_mono_samples, 0);
    assert!(report.statistics.finalized);
    assert!(report.statistics.tail_disposition_known);
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn empty_playback_wait_does_not_block_discard() {
    let backend = FixtureAudioBackend::new();
    let consumer = backend.clone();
    let ports = open(&backend).await;
    let mut next = tokio::spawn(async move { consumer.next_playback("m1-render").await });
    assert!(timeout(SHORT, &mut next).await.is_err());
    timeout(SHORT, ports.control.discard_pending())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(timeout(SHORT, next).await.unwrap().unwrap(), None);
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn close_is_owned_and_replayed_after_a_waiter_is_cancelled() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_close("live-input");
    let ports = open(&backend).await;
    assert!(timeout(SHORT, ports.control.close()).await.is_err());
    assert_eq!(backend.open_ports(), 1);
    assert!(!ports.control.statistics().playback.finalized);
    assert!(timeout(SHORT, ports.control.close()).await.is_err());
    release.send(()).unwrap();
    timeout(SHORT, ports.control.close())
        .await
        .unwrap()
        .unwrap();
    ports.control.close().await.unwrap();
    assert_eq!(backend.open_ports(), 0);
    assert!(ports.control.statistics().playback.finalized);
}

#[tokio::test]
async fn dropped_close_acknowledgement_keeps_unknown_tail_and_quarantines_endpoints() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_close("live-input");
    let ports = open(&backend).await;
    drop(release);
    assert!(matches!(
        ports.control.close().await,
        Err(AppError::Audio {
            code: RuntimeFailureCode::CleanupTimeout,
            ..
        })
    ));
    assert_eq!(backend.open_ports(), 1);
    assert!(!ports.control.statistics().playback.finalized);
    assert!(!ports.control.statistics().playback.tail_disposition_known);
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
    assert!(backend.open(request(&backend).await).await.is_err());
    drop(ports);
    assert_eq!(backend.open_ports(), 1);
}

#[tokio::test]
async fn dropping_control_does_not_acknowledge_a_delayed_close() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_close("live-input");
    let ports = open(&backend).await;
    drop(ports);
    tokio::task::yield_now().await;
    assert_eq!(backend.open_ports(), 1);
    assert!(backend.open(request(&backend).await).await.is_err());
    release.send(()).unwrap();
    timeout(SHORT, async {
        while backend.open_ports() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn close_suppresses_playback_before_waiting_for_acknowledgement() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_close("live-input");
    let ports = open(&backend).await;
    ports.playback.send(frame(1, 320)).await.unwrap();
    assert!(timeout(SHORT, ports.control.close()).await.is_err());
    assert_eq!(
        timeout(SHORT, backend.next_playback("m1-render"))
            .await
            .unwrap(),
        None
    );
    release.send(()).unwrap();
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn readiness_changes_inventory_generation_and_rejects_stale_open() {
    let backend = FixtureAudioBackend::new();
    let first = backend.inventory().await.unwrap();
    assert_eq!(
        first.generation,
        backend.inventory().await.unwrap().generation
    );
    backend.lose("live-input");
    let lost = backend.inventory().await.unwrap();
    assert_ne!(first.generation, lost.generation);
    backend.lose("live-input");
    assert_eq!(
        lost.generation,
        backend.inventory().await.unwrap().generation
    );
    backend.restore("live-input");
    let restored = backend.inventory().await.unwrap();
    assert_ne!(lost.generation, restored.generation);
    let mut stale = request(&backend).await;
    stale.inventory_generation = first.generation;
    assert!(backend.open(stale).await.is_err());
    let ports = open(&backend).await;
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn delayed_open_revalidates_inventory_after_release() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_open("live-input");
    let old = request(&backend).await;
    let opening = backend.clone();
    let mut pending = tokio::spawn(async move { opening.open(old).await });
    assert!(timeout(SHORT, &mut pending).await.is_err());
    backend.lose("live-input");
    backend.restore("live-input");
    release.send(Ok(())).unwrap();
    assert!(pending.await.unwrap().is_err());
    assert_eq!(backend.open_ports(), 0);
}

#[tokio::test]
async fn stale_open_is_rejected_before_consuming_a_delayed_open_gate() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_open("live-input");
    let mut stale = request(&backend).await;
    stale.inventory_generation = Uuid::new_v4();
    assert!(timeout(SHORT, backend.open(stale)).await.unwrap().is_err());
    let opening = backend.clone();
    let current = request(&backend).await;
    let mut pending = tokio::spawn(async move { opening.open(current).await });
    assert!(timeout(SHORT, &mut pending).await.is_err());
    release.send(Ok(())).unwrap();
    let ports = pending.await.unwrap().unwrap();
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn open_rejects_wrong_endpoint_direction_and_busy_routes() {
    let backend = FixtureAudioBackend::new();
    let mut wrong = request(&backend).await;
    std::mem::swap(&mut wrong.input_id, &mut wrong.output_id);
    assert!(backend.open(wrong).await.is_err());
    let ports = open(&backend).await;
    assert!(backend.open(request(&backend).await).await.is_err());
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn role_driver_and_readiness_controls_expose_current_inventory_constraints() {
    let backend = FixtureAudioBackend::new();
    let before = backend.inventory().await.unwrap();
    backend.set_role("m1-render", EndpointRole::Input);
    let changed = backend.inventory().await.unwrap();
    assert_ne!(before.generation, changed.generation);
    assert_eq!(
        changed
            .endpoints
            .iter()
            .find(|e| e.endpoint.id == "m1-render")
            .unwrap()
            .role,
        EndpointRole::Input
    );
    assert!(backend.open(request(&backend).await).await.is_err());
    backend.set_role("m1-render", EndpointRole::Input);
    assert_eq!(
        changed.generation,
        backend.inventory().await.unwrap().generation
    );
    backend.set_role("m1-render", EndpointRole::M1Render);
    backend.set_ready("m1-render", false);
    assert!(backend.open(request(&backend).await).await.is_err());
    backend.set_ready("m1-render", true);
    backend.set_driver_status(DriverStatus::Failed("fixture driver unavailable".into()));
    assert!(backend.open(request(&backend).await).await.is_err());
    backend.set_driver_status(DriverStatus::Ready);
    let ports = open(&backend).await;
    ports.control.close().await.unwrap();
}

#[tokio::test]
async fn typed_close_failure_is_replayed_without_finalizing_or_releasing_endpoints() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_close_result("live-input");
    let ports = open(&backend).await;
    release
        .send(Err(RuntimeFailureCode::DeviceUnavailable))
        .unwrap();
    for _ in 0..2 {
        assert!(matches!(
            ports.control.close().await,
            Err(AppError::Audio {
                code: RuntimeFailureCode::DeviceUnavailable,
                ..
            })
        ));
        assert!(!ports.control.statistics().playback.finalized);
        assert!(!ports.control.statistics().playback.tail_disposition_known);
    }
    assert_eq!(backend.open_ports(), 1);
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
    assert!(backend.open(request(&backend).await).await.is_err());
}

#[tokio::test]
async fn capture_loss_controls_keep_native_blocks_and_normalized_frames_distinct() {
    let backend = FixtureAudioBackend::new();
    let ports = open(&backend).await;
    backend.inject_capture_loss("live-input", 2, 7);
    for sequence in 0..10 {
        backend
            .send_capture(
                "live-input",
                AudioFrame::new(
                    AudioSource::Microphone,
                    sequence,
                    PcmSpec::pcm16_mono(16_000),
                    vec![1; 1280],
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    let capture = ports.control.statistics().capture;
    assert_eq!(capture.dropped_native_blocks, 2);
    assert_eq!(capture.dropped_normalized_frames, 9);
    ports.control.close().await.unwrap();
    assert_eq!(
        ports.control.statistics().capture.dropped_normalized_frames,
        9
    );
}

#[tokio::test]
async fn discard_reports_actual_consumption_and_both_discard_stages() {
    let backend = FixtureAudioBackend::new();
    let ports = open(&backend).await;
    ports.playback.send(frame(1, 320)).await.unwrap();
    assert_eq!(
        backend
            .next_playback("m1-render")
            .await
            .unwrap()
            .samples()
            .len(),
        320
    );
    ports.playback.send(frame(2, 160)).await.unwrap();
    timeout(SHORT, async {
        loop {
            if backend
                .playback_statistics("m1-render")
                .unwrap()
                .accepted_for_processing_samples
                == 480
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // This frame is queued at ingress when suppression wins, so it is never converted.
    ports.playback.try_send(frame(3, 80)).unwrap();
    ports.control.discard_pending().await.unwrap();
    ports.control.close().await.unwrap();
    let statistics = backend.playback_statistics("m1-render").unwrap();
    assert_eq!(statistics.accepted_for_processing_samples, 560);
    assert_eq!(statistics.discarded_before_conversion_samples, 80);
    let native = statistics.native.unwrap();
    assert_eq!(native.generated_mono_samples, 480);
    assert_eq!(native.callback_consumed_mono_samples, 320);
    assert_eq!(native.discarded_mono_samples, 160);
    assert!(statistics.finalized);
    assert!(statistics.tail_disposition_known);
    let reopened = open(&backend).await;
    assert_eq!(
        backend
            .playback_statistics("m1-render")
            .unwrap()
            .accepted_for_processing_samples,
        0
    );
    assert_eq!(backend.total_opens(), 2);
    reopened.control.close().await.unwrap();
}

#[tokio::test]
async fn empty_playback_wait_does_not_block_drain_or_close() {
    let backend = FixtureAudioBackend::new();
    let ports = open(&backend).await;
    let consumer = backend.clone();
    let mut next = tokio::spawn(async move { consumer.next_playback("m1-render").await });
    assert!(timeout(SHORT, &mut next).await.is_err());
    let report = timeout(SHORT, ports.control.drain_playback())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.boundary, PlaybackBoundary::CallbackConsumed);
    assert_eq!(timeout(SHORT, next).await.unwrap().unwrap(), None);
    timeout(SHORT, ports.control.close())
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn close_timeout_retains_unknown_tail_until_late_owner_acknowledgement() {
    let backend = FixtureAudioBackend::new();
    let release = backend.delay_close("live-input");
    let ports = open(&backend).await;
    assert!(matches!(
        ports.control.close().await,
        Err(AppError::Audio {
            code: RuntimeFailureCode::CleanupTimeout,
            ..
        })
    ));
    assert_eq!(backend.open_ports(), 1);
    assert!(!ports.control.statistics().playback.finalized);
    assert!(!ports.control.statistics().playback.tail_disposition_known);
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
    release.send(()).unwrap();
    timeout(SHORT, async {
        while backend.open_ports() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(ports.control.statistics().playback.finalized);
    assert!(ports.control.statistics().playback.tail_disposition_known);
    assert!(matches!(
        ports.control.close().await,
        Err(AppError::Audio {
            code: RuntimeFailureCode::CleanupTimeout,
            ..
        })
    ));
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
    assert!(backend.open(request(&backend).await).await.is_err());
}
