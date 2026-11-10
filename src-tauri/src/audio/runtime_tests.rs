use super::{platform::native_io, runtime::*};
use crate::domain::{audio::AudioSource, transcript::*};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::ThreadId,
    time::Duration,
};
use uuid::Uuid;

struct Probe {
    threads: Mutex<Vec<ThreadId>>,
    pending: AtomicBool,
    samples: AtomicUsize,
    closes: AtomicUsize,
}
struct Factory(Arc<Probe>);
struct Owner {
    probe: Arc<Probe>,
    shared: AudioShared,
    _not_send: Rc<Cell<u32>>,
}
impl native_io::NativeOwnerFactory for Factory {
    fn open_on_owner_thread(
        &self,
        _: &AudioOpenRequest,
        _: super::queue::BoundedAudioQueue,
        _: tokio::sync::mpsc::Receiver<super::AudioFrame>,
        shared: AudioShared,
    ) -> crate::error::AppResult<Box<dyn native_io::NativeOwner>> {
        self.0
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        Ok(Box::new(Owner {
            probe: self.0.clone(),
            shared,
            _not_send: Rc::new(Cell::new(0)),
        }))
    }
}
impl native_io::NativeOwner for Owner {
    fn stop_capture(&mut self) -> crate::error::AppResult<()> {
        Ok(())
    }
    fn discard_pending(&mut self) -> crate::error::AppResult<()> {
        self.shared.suppress();
        Ok(())
    }
    fn seal_playback(&mut self) -> crate::error::AppResult<()> {
        Ok(())
    }
    fn playback_drained(&mut self) -> crate::error::AppResult<Option<PlaybackReport>> {
        self.probe
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
        if super::playback::drain_complete(
            !self.probe.pending.load(Ordering::Acquire),
            self.probe.samples.load(Ordering::Acquire) == 0,
        ) {
            Ok(Some(PlaybackReport {
                boundary: PlaybackBoundary::CallbackConsumed,
                statistics: self.shared.statistics().playback,
            }))
        } else {
            Ok(None)
        }
    }
    fn close(&mut self) -> crate::error::AppResult<()> {
        self.probe.closes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.probe
            .threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
    }
}
fn request() -> AudioOpenRequest {
    AudioOpenRequest {
        inventory_generation: Uuid::nil(),
        input_id: "in".into(),
        output_id: "out".into(),
        source: AudioSource::Microphone,
    }
}
#[tokio::test]
async fn native_drain_waits_for_conversion_and_consumption_and_close_is_shared() {
    let probe = Arc::new(Probe {
        threads: Mutex::new(vec![]),
        pending: AtomicBool::new(true),
        samples: AtomicUsize::new(0),
        closes: AtomicUsize::new(0),
    });
    let ports = native_io::open_with_factory(request(), Arc::new(Factory(probe.clone())))
        .await
        .unwrap();
    let control = ports.control.clone();
    let mut drain = Box::pin(control.drain_playback());
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut drain)
            .await
            .is_err()
    );
    probe.samples.store(1920, Ordering::Release);
    probe.pending.store(false, Ordering::Release);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut drain)
            .await
            .is_err()
    );
    probe.samples.store(0, Ordering::Release);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), drain)
            .await
            .unwrap()
            .unwrap()
            .boundary,
        PlaybackBoundary::CallbackConsumed
    );
    let (a, b) = tokio::join!(ports.control.close(), ports.control.close());
    a.unwrap();
    b.unwrap();
    assert_eq!(probe.closes.load(Ordering::Relaxed), 1);
    let ids = probe.threads.lock().unwrap();
    assert!(ids.iter().all(|id| *id == ids[0]));
}
#[test]
fn accounting_keeps_bus_and_native_units_and_legacy_json() {
    let shared = AudioShared::new(super::queue::BoundedAudioQueue::default());
    shared.counters.accepted.store(1280, Ordering::Relaxed);
    shared.counters.native_rate.store(48000, Ordering::Relaxed);
    shared
        .counters
        .native_generated
        .store(2560, Ordering::Relaxed);
    shared
        .counters
        .native_discarded
        .store(17, Ordering::Relaxed);
    let stats = shared.statistics();
    assert_eq!(stats.playback.input_sample_rate_hz, 24000);
    assert_eq!(stats.playback.discarded_before_conversion_samples, 0);
    assert_eq!(stats.playback.native.unwrap().discarded_mono_samples, 17);
    let old = r#"{"playback":{"boundary":"discarded","accepted_samples":8,"discarded_samples":2}}"#;
    let value: DiagnosticPayload = serde_json::from_str(old).unwrap();
    assert_eq!(serde_json::to_string(&value).unwrap(), old);
    for value in [
        DiagnosticPayload::CaptureAccounting {
            statistics: shared.statistics().capture,
        },
        DiagnosticPayload::PlaybackAccounting {
            boundary: None,
            statistics: shared.statistics().playback,
        },
    ] {
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<DiagnosticPayload>(&json).unwrap(),
            value
        );
    }
    shared.suppress();
    assert!(shared.gates.capture_suppressed.load(Ordering::Acquire));
    assert!(shared.gates.playback_suppressed.load(Ordering::Acquire));
    assert!(!shared.statistics().playback.tail_disposition_known);
}

pub(crate) struct DropFactory {
    pub(crate) entered: Arc<AtomicBool>,
    pub(crate) release: Arc<AtomicBool>,
}
struct BlockingDrop {
    entered: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    _not_send: Rc<()>,
}
impl native_io::NativeOwnerFactory for DropFactory {
    fn open_on_owner_thread(
        &self,
        _: &AudioOpenRequest,
        _: super::queue::BoundedAudioQueue,
        _: tokio::sync::mpsc::Receiver<super::AudioFrame>,
        _: AudioShared,
    ) -> crate::error::AppResult<Box<dyn native_io::NativeOwner>> {
        Ok(Box::new(BlockingDrop {
            entered: self.entered.clone(),
            release: self.release.clone(),
            _not_send: Rc::new(()),
        }))
    }
}
impl native_io::NativeOwner for BlockingDrop {
    fn stop_capture(&mut self) -> crate::error::AppResult<()> {
        Ok(())
    }
    fn discard_pending(&mut self) -> crate::error::AppResult<()> {
        Ok(())
    }
    fn seal_playback(&mut self) -> crate::error::AppResult<()> {
        Ok(())
    }
    fn playback_drained(&mut self) -> crate::error::AppResult<Option<PlaybackReport>> {
        Ok(None)
    }
    fn close(&mut self) -> crate::error::AppResult<()> {
        Ok(())
    }
}
impl Drop for BlockingDrop {
    fn drop(&mut self) {
        self.entered.store(true, Ordering::Release);
        while !self.release.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
#[tokio::test]
async fn close_acknowledgement_waits_for_owner_destruction() {
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let ports = native_io::open_owned(
        request(),
        Arc::new(DropFactory {
            entered: entered.clone(),
            release: release.clone(),
        }),
        None,
        native_io::Deadlines {
            open: Duration::from_secs(1),
            close: Duration::from_millis(30),
            drain: Duration::from_millis(50),
        },
    )
    .await
    .unwrap();
    let result = ports.control.close().await;
    // Release even on a regression, so no injected native owner remains blocked.
    release.store(true, Ordering::Release);
    assert!(
        matches!(
            result,
            Err(crate::error::AppError::Audio {
                code: RuntimeFailureCode::CleanupTimeout,
                ..
            })
        ),
        "native destructor must acknowledge before close succeeds"
    );
    assert!(entered.load(Ordering::Acquire));
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
    ports.control.suppress_now();
    assert!(!ports.control.statistics().playback.finalized);
}

// Dropping/caching a timed-out command reply must not lose the independent
// actual outer-thread join producer or falsely acknowledge native teardown.
#[tokio::test]
async fn audio_actual_join_survives_native_close_observer_timeout() {
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let ports = native_io::open_owned(
        request(),
        Arc::new(DropFactory {
            entered: entered.clone(),
            release: release.clone(),
        }),
        None,
        native_io::Deadlines {
            open: Duration::from_secs(1),
            close: Duration::from_millis(30),
            drain: Duration::from_millis(50),
        },
    )
    .await
    .unwrap();
    let actual = ports.control.actual_join_port();
    let result = ports.control.close().await;
    let pending = match &actual {
        Ok(join) => tokio::time::timeout(Duration::from_millis(20), join.wait())
            .await
            .is_err(),
        Err(_) => false,
    };
    release.store(true, Ordering::Release);
    let joined = match actual {
        Ok(join) => tokio::time::timeout(Duration::from_secs(2), join.wait())
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false),
        Err(_) => false,
    };
    assert!(entered.load(Ordering::Acquire));
    assert!(result.is_err());
    assert!(
        pending,
        "a real native drop remains entered after the command observer times out"
    );
    assert!(
        joined,
        "the same actual join source must acknowledge outer-thread completion after release"
    );
    assert_eq!(*ports.health.borrow(), AudioHealth::NeedsRestart);
}
