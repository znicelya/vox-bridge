//! Owns acquisition and acknowledged teardown, including operations that finish
//! after their caller has stopped admitting work. No join handle is detached.
use super::{direction::provider_failure, runtime::audio_failure};
use crate::{
    audio::runtime::{AudioBackend, AudioControl, AudioOpenRequest, AudioPorts},
    domain::{
        session::SessionDirection,
        transcript::{AudioStatistics, RuntimeFailureCode},
    },
    providers::{
        ProviderConnection, ProviderError, ProviderEvent,
        factory::{DirectedProviderStream, PreparedProviderSession},
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{sync::watch, task::JoinHandle, time::Instant};

pub(super) const AUDIO_OPEN_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const PROVIDER_READY_TIMEOUT: Duration = Duration::from_secs(10);
const ROLLBACK_ALLOWANCE: Duration = Duration::from_secs(2);
#[cfg(not(test))]
pub(super) const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
pub(super) const CLOSE_TIMEOUT: Duration = Duration::from_millis(400);

pub(super) struct Resources {
    pub stream: Arc<dyn ProviderConnection>,
    pub ports: AudioPorts,
    pub output_sequence: u64,
}
#[derive(Default)]
pub(super) struct CleanupReport {
    pub failures: Vec<RuntimeFailureCode>,
    pub statistics: Vec<AudioStatistics>,
}
impl CleanupReport {
    pub fn fail(&mut self, code: RuntimeFailureCode) {
        if !self.failures.contains(&code) {
            self.failures.push(code);
        }
    }
    pub fn merge(&mut self, other: Self) {
        for code in other.failures {
            self.fail(code);
        }
        self.statistics.extend(other.statistics);
    }
}
pub(super) async fn close_audio(control: Arc<dyn AudioControl>, discard: bool) -> CleanupReport {
    if discard {
        control.suppress_now();
    }
    let mut report = CleanupReport::default();
    // A failed discard must not skip the close request.
    let deadline = Instant::now() + CLOSE_TIMEOUT;
    if discard {
        match tokio::time::timeout_at(deadline, control.discard_pending()).await {
            Ok(Err(error)) => report.fail(audio_failure(&error)),
            Err(_) => report.fail(RuntimeFailureCode::CleanupTimeout),
            _ => {}
        }
    }
    match tokio::time::timeout_at(deadline, control.close()).await {
        Ok(Err(error)) => report.fail(audio_failure(&error)),
        Err(_) => report.fail(RuntimeFailureCode::CleanupTimeout),
        _ => {}
    }
    if !report.failures.is_empty() {
        control.suppress_now();
    }
    report.statistics.push(control.statistics());
    report
}

pub(super) struct AudioAcquired {
    pub ports: Option<AudioPorts>,
    pub report: CleanupReport,
}
pub(super) struct AudioOperation {
    task: Option<JoinHandle<AudioAcquired>>,
    cancelled: Arc<AtomicBool>,
    pub admission_deadline: Instant,
    cleanup_deadline: Instant,
    retired_control: Option<Arc<dyn AudioControl>>,
}
impl AudioOperation {
    pub fn open(audio: Arc<dyn AudioBackend>, request: AudioOpenRequest) -> Self {
        Self::begin(audio, request, None)
    }
    pub fn replace(
        audio: Arc<dyn AudioBackend>,
        request: AudioOpenRequest,
        old: AudioPorts,
    ) -> Self {
        old.control.suppress_now();
        Self::begin(audio, request, Some(old))
    }
    fn begin(
        audio: Arc<dyn AudioBackend>,
        request: AudioOpenRequest,
        old: Option<AudioPorts>,
    ) -> Self {
        let retired_control = old.as_ref().map(|ports| ports.control.clone());
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let close_allowance = if old.is_some() {
            CLOSE_TIMEOUT
        } else {
            Duration::ZERO
        };
        let admission_deadline = Instant::now() + close_allowance + AUDIO_OPEN_TIMEOUT;
        let task = tokio::spawn(async move {
            let mut report = CleanupReport::default();
            if let Some(old) = old {
                report.merge(close_audio(old.control.clone(), true).await);
            }
            if !report.failures.is_empty() || signal.load(Ordering::Acquire) {
                return AudioAcquired {
                    ports: None,
                    report,
                };
            }
            let ports = match audio.open(request).await {
                Ok(ports) => Some(ports),
                Err(error) => {
                    report.fail(audio_failure(&error));
                    None
                }
            };
            AudioAcquired { ports, report }
        });
        Self {
            task: Some(task),
            cancelled,
            admission_deadline,
            cleanup_deadline: admission_deadline + ROLLBACK_ALLOWANCE,
            retired_control,
        }
    }
    pub async fn join(&mut self) -> AudioAcquired {
        let result = self.task.as_mut().expect("owned audio operation").await;
        self.task.take();
        result.unwrap_or_else(|_| AudioAcquired {
            ports: None,
            report: CleanupReport {
                failures: vec![RuntimeFailureCode::CleanupTimeout],
                statistics: vec![],
            },
        })
    }
    pub async fn cancel_and_settle(mut self) -> CleanupReport {
        self.cancelled.store(true, Ordering::Release);
        let mut report = CleanupReport::default();
        let acquired = match tokio::time::timeout_at(self.cleanup_deadline, self.join()).await {
            Ok(acquired) => Some(acquired),
            Err(_) => {
                report.fail(RuntimeFailureCode::CleanupTimeout);
                match self.task.take() {
                    Some(task) => {
                        task.abort();
                        // Abort is a request: the current poll can still finish
                        // with acquired ports or a typed failure. Own that value.
                        task.await.ok()
                    }
                    None => None,
                }
            }
        };
        if let Some(acquired) = acquired {
            report.merge(acquired.report);
            if let Some(ports) = acquired.ports {
                report.merge(close_audio(ports.control.clone(), true).await);
            }
        } else if let Some(control) = &self.retired_control {
            report.statistics.push(control.statistics());
        }
        report
    }
}
impl Drop for AudioOperation {
    fn drop(&mut self) {
        // Panic safety only; every ordinary exit calls join or cancel_and_settle.
        self.cancelled.store(true, Ordering::Release);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

pub(super) async fn cancel_provider(
    stream: Arc<dyn ProviderConnection>,
    mut event: impl FnMut(&ProviderEvent) -> Result<(), RuntimeFailureCode>,
) -> CleanupReport {
    let mut report = CleanupReport::default();
    let cancel = stream.cancel();
    tokio::pin!(cancel);
    let mut cancel_done = false;
    let mut terminal_done = false;
    let deadline = tokio::time::sleep(CLOSE_TIMEOUT);
    tokio::pin!(deadline);
    loop {
        if cancel_done && terminal_done {
            break;
        }
        tokio::select! {
            _ = &mut deadline => { report.fail(RuntimeFailureCode::CleanupTimeout); break; }
            result = &mut cancel, if !cancel_done => {
                cancel_done = true;
                if let Err(error) = result { if error != ProviderError::Cancelled { report.fail(provider_failure(&error)); } }
            }
            value = stream.next_event(), if !terminal_done => match value {
                Ok(Some(value @ ProviderEvent::Failed { .. })) => {
                    if let ProviderEvent::Failed { error, .. } = &value {
                        if *error != ProviderError::Cancelled {
                            report.fail(provider_failure(error));
                            if let Err(code) = event(&value) { report.fail(code); }
                        }
                    }
                    terminal_done = true;
                }
                Ok(Some(ProviderEvent::Finished { .. })) | Ok(None) => terminal_done = true,
                Err(error) => { report.fail(provider_failure(&error)); terminal_done = true; }
                Ok(Some(value @ (ProviderEvent::SourceInterim { .. } | ProviderEvent::SourceFinal { .. } | ProviderEvent::TranslationInterim { .. } | ProviderEvent::TranslationFinal { .. } | ProviderEvent::Muted { .. }))) => {
                    if let Err(code) = event(&value) { report.fail(code); }
                }
                Ok(Some(_)) => {}
            }
        }
    }
    report
}

async fn settle_provider_open(
    mut task: JoinHandle<crate::providers::ProviderResult<DirectedProviderStream>>,
    deadline: Instant,
    mut event: impl FnMut(&ProviderEvent) -> Result<(), RuntimeFailureCode>,
) -> CleanupReport {
    let mut report = CleanupReport::default();
    let joined = match tokio::time::timeout_at(deadline, &mut task).await {
        Ok(joined) => joined,
        Err(_) => {
            report.fail(RuntimeFailureCode::CleanupTimeout);
            task.abort();
            task.await
        }
    };
    match joined {
        Ok(Ok(DirectedProviderStream { stream, .. })) => {
            report.merge(cancel_provider(Arc::from(stream), &mut event).await);
        }
        Ok(Err(error)) => report.fail(provider_failure(&error)),
        Err(_) => report.fail(RuntimeFailureCode::CleanupTimeout),
    }
    report
}

pub(super) async fn open_resources(
    audio: Arc<dyn AudioBackend>,
    request: AudioOpenRequest,
    provider: PreparedProviderSession,
    direction: SessionDirection,
    mut stop: watch::Receiver<bool>,
    mut event: impl FnMut(&ProviderEvent) -> Result<(), RuntimeFailureCode>,
) -> Result<Resources, CleanupReport> {
    let mut operation = Some(AudioOperation::open(audio, request));
    let mut provider_task = Some(tokio::spawn(async move {
        provider.connect_direction(direction).await
    }));
    let provider_deadline = Instant::now() + PROVIDER_READY_TIMEOUT;
    let audio_deadline = operation.as_ref().unwrap().admission_deadline;
    let mut ports = None;
    let mut stream: Option<Arc<dyn ProviderConnection>> = None;
    let mut report = CleanupReport::default();
    loop {
        if *stop.borrow() {
            report.fail(RuntimeFailureCode::Interrupted);
            break;
        }
        if ports.is_some() && stream.is_some() {
            return Ok(Resources {
                ports: ports.unwrap(),
                stream: stream.unwrap(),
                output_sequence: 0,
            });
        }
        tokio::select! {
            _ = stop.changed() => {},
            _ = tokio::time::sleep_until(audio_deadline), if operation.is_some() => { report.fail(RuntimeFailureCode::StartupTimeout); break; }
            _ = tokio::time::sleep_until(provider_deadline), if provider_task.is_some() => { report.fail(RuntimeFailureCode::StartupTimeout); break; }
            acquired = async { operation.as_mut().unwrap().join().await }, if operation.is_some() => {
                operation.take();
                report.merge(acquired.report);
                ports = acquired.ports;
                if ports.is_none() { break; }
            }
            connected = async { provider_task.as_mut().unwrap().await }, if provider_task.is_some() => {
                provider_task.take();
                match connected {
                    Ok(Ok(directed)) => stream = Some(Arc::from(directed.stream)),
                    Ok(Err(error)) => { report.fail(provider_failure(&error)); break; }
                    Err(_) => { report.fail(RuntimeFailureCode::CleanupTimeout); break; }
                }
            }
        }
    }
    let audio_cleanup = async {
        if let Some(operation) = operation {
            operation.cancel_and_settle().await
        } else if let Some(ports) = ports {
            close_audio(ports.control.clone(), true).await
        } else {
            CleanupReport::default()
        }
    };
    let provider_cleanup = async {
        let mut report = CleanupReport::default();
        if let Some(task) = provider_task {
            report.merge(settle_provider_open(task, provider_deadline, &mut event).await);
        }
        if let Some(stream) = stream {
            report.merge(cancel_provider(stream, &mut event).await);
        }
        report
    };
    let (audio, provider) = tokio::join!(audio_cleanup, provider_cleanup);
    report.merge(audio);
    report.merge(provider);
    Err(report)
}

#[cfg(test)]
mod rereview_regressions {
    use super::*;
    use crate::{
        audio::{PcmSpec, runtime::AudioBackend},
        domain::audio::AudioSource,
        providers::fixture::{FixtureScript, FixtureStep},
        session::test_support::{RuntimeHarness, metadata},
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rereview_abort_racing_ready_acquisition_must_inspect_joined_value() {
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
        let ports = h
            .audio
            .open(AudioOpenRequest {
                inventory_generation: inventory.generation,
                input_id: "live-input".into(),
                output_id: "m1-render".into(),
                source: AudioSource::Microphone,
            })
            .await
            .unwrap();
        let control = ports.control.clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            entered_tx.send(()).unwrap();
            // Hold one finite Ready-producing poll across the timeout/abort boundary.
            // Tokio can complete this poll normally even after abort was requested.
            std::thread::sleep(Duration::from_millis(100));
            AudioAcquired {
                ports: Some(ports),
                report: CleanupReport::default(),
            }
        });
        entered_rx.await.unwrap();
        let operation = AudioOperation {
            task: Some(task),
            cancelled: Arc::new(AtomicBool::new(false)),
            admission_deadline: Instant::now(),
            cleanup_deadline: Instant::now() + Duration::from_millis(5),
            retired_control: None,
        };
        let report = operation.cancel_and_settle().await;
        let owned_after_settlement = h.audio.open_ports();
        control.close().await.unwrap();
        assert!(
            report
                .failures
                .contains(&RuntimeFailureCode::CleanupTimeout),
            "test must exercise the timeout arm"
        );
        assert_eq!(
            owned_after_settlement, 0,
            "timeout arm discarded live late ports without acknowledged close"
        );
    }
}

#[cfg(test)]
mod rereview_failure_regression {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rereview_abort_racing_typed_open_failure_must_inspect_joined_value() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            AudioAcquired {
                ports: None,
                report: CleanupReport {
                    failures: vec![RuntimeFailureCode::DeviceUnavailable],
                    statistics: vec![],
                },
            }
        });
        entered_rx.await.unwrap();
        let operation = AudioOperation {
            task: Some(task),
            cancelled: Arc::new(AtomicBool::new(false)),
            admission_deadline: Instant::now(),
            cleanup_deadline: Instant::now() + Duration::from_millis(5),
            retired_control: None,
        };
        let report = operation.cancel_and_settle().await;
        assert!(
            report
                .failures
                .contains(&RuntimeFailureCode::CleanupTimeout),
            "test must exercise timeout arm"
        );
        assert!(
            report
                .failures
                .contains(&RuntimeFailureCode::DeviceUnavailable),
            "timeout arm discarded the successfully joined open failure's typed category"
        );
    }
}

#[cfg(test)]
mod provider_abort_regressions {
    use super::*;
    use crate::{
        audio::PcmSpec,
        domain::session::SessionMode,
        providers::{
            ProviderSessionConfig, TranslationProvider,
            fixture::{FixtureScript, FixtureStep},
        },
        session::test_support::{RuntimeHarness, metadata},
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fix2_abort_racing_provider_acquisition_cancels_and_observes_terminal() {
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
        let config = ProviderSessionConfig {
            session_id: h.id,
            direction: SessionDirection::SourceToTarget,
            mode: SessionMode::Live,
            languages: h.launch.config.languages().clone(),
            target_audio: PcmSpec::pcm16_mono(24_000),
            provider_config_id: h.saved.id(),
        };
        let stream = h.provider.connect(config.clone()).await.unwrap();
        let terminal = h.provider.delay_terminal(SessionDirection::SourceToTarget);
        let (entered, entered_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = entered.send(());
            std::thread::sleep(Duration::from_millis(100));
            Ok(DirectedProviderStream { config, stream })
        });
        entered_rx.await.unwrap();
        let cleanup = tokio::spawn(settle_provider_open(
            task,
            Instant::now() + Duration::from_millis(5),
            |_| Ok(()),
        ));
        let terminal_observed =
            tokio::time::timeout(Duration::from_millis(250), terminal.wait_entered()).await;
        let waiting_for_terminal = !cleanup.is_finished();
        terminal.release();
        let report = tokio::time::timeout(Duration::from_secs(1), cleanup)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            h.provider.cancel_counts(),
            [1, 0],
            "joined live stream was dropped without cancellation"
        );
        assert!(
            matches!(terminal_observed, Ok(true)) && waiting_for_terminal,
            "cancellation was mistaken for terminal acknowledgement"
        );
        assert_eq!(report.failures, [RuntimeFailureCode::CleanupTimeout]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fix2_abort_racing_provider_failure_preserves_typed_failure() {
        let (entered, entered_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = entered.send(());
            std::thread::sleep(Duration::from_millis(100));
            Err(ProviderError::RemoteFailure { code: 41 })
        });
        entered_rx.await.unwrap();
        let report =
            settle_provider_open(task, Instant::now() + Duration::from_millis(5), |_| Ok(())).await;
        assert!(
            report
                .failures
                .contains(&RuntimeFailureCode::CleanupTimeout)
        );
        assert!(
            report.failures.contains(&RuntimeFailureCode::Provider),
            "joined provider failure was discarded after abort"
        );
    }
}
