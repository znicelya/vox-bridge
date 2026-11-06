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
        match tokio::time::timeout_at(self.cleanup_deadline, self.join()).await {
            Ok(acquired) => {
                report.merge(acquired.report);
                if let Some(ports) = acquired.ports {
                    report.merge(close_audio(ports.control.clone(), true).await);
                }
            }
            Err(_) => {
                report.fail(RuntimeFailureCode::CleanupTimeout);
                if let Some(task) = self.task.take() {
                    task.abort();
                    let _ = task.await;
                }
                if let Some(control) = &self.retired_control {
                    report.statistics.push(control.statistics());
                }
            }
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
        if let Some(mut task) = provider_task {
            match tokio::time::timeout_at(provider_deadline, &mut task).await {
                Ok(Ok(Ok(DirectedProviderStream {
                    stream: acquired, ..
                }))) => stream = Some(Arc::from(acquired)),
                Ok(Ok(Err(error))) => report.fail(provider_failure(&error)),
                Ok(Err(_)) => report.fail(RuntimeFailureCode::CleanupTimeout),
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    report.fail(RuntimeFailureCode::CleanupTimeout);
                }
            }
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
