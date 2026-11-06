//! One explicit probe owner; its lease follows native work beyond observer timeouts.
use super::{
    config::{ServiceConfigStore, endpoint},
    http::{HttpRetryPolicy, JsonHttp, JsonRequest, cancelled},
    jobs::{OperationLease, now_ms},
    types::*,
};
use crate::error::{AppError, AppResult};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use uuid::Uuid;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
#[derive(Default)]
struct State {
    closed: bool,
    cancel: Option<watch::Sender<bool>>,
}
pub(crate) struct ProbeOwner {
    gate: Arc<Semaphore>,
    state: Mutex<State>,
    timeout: Duration,
}
struct ProbeLease {
    _operation: OperationLease,
    _permit: OwnedSemaphorePermit,
}
struct Observer {
    cancel: watch::Sender<bool>,
}
impl Drop for Observer {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}
impl ProbeOwner {
    pub(crate) fn new() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(1)),
            state: Mutex::default(),
            timeout: PROBE_TIMEOUT,
        }
    }
    fn begin(
        &self,
        operation: OperationLease,
    ) -> AppResult<(OperationLease, Observer, watch::Receiver<bool>)> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        let permit = self
            .gate
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(IntelligenceErrorCode::Busy))?;
        let (cancel, receiver) = watch::channel(false);
        state.cancel = Some(cancel.clone());
        Ok((
            Arc::new(ProbeLease {
                _operation: operation,
                _permit: permit,
            }),
            Observer { cancel },
            receiver,
        ))
    }
    pub(crate) async fn model(
        &self,
        config: Arc<ServiceConfigStore>,
        http: Arc<dyn JsonHttp>,
        id: Uuid,
        operation: OperationLease,
    ) -> AppResult<ConnectionProbe> {
        let (lease, _observer, mut cancel) = self.begin(operation)?;
        let deadline = tokio::time::Instant::now() + self.timeout;
        let work = tokio::spawn(async move {
            let config = config.scoped(lease.clone());
            let _lease = lease;
            let started = tokio::time::Instant::now();
            let (snapshot, revision, bearer) = match tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => Err(error(IntelligenceErrorCode::Cancelled)),
                result = tokio::time::timeout_at(deadline, config.probe_snapshot(id)) => {
                    result.unwrap_or(Err(error(IntelligenceErrorCode::Timeout)))
                }
            } {
                Ok(v) => v,
                Err(e) => return Ok(outcome(started, Err(category(e)))),
            };
            let (body, limit, retry) = match snapshot.kind {
                ServiceKind::Llm => (
                    serde_json::json!({"model":snapshot.model,"messages":[{"role":"user","content":"Reply with OK."}],"max_tokens":8}),
                    2 * 1024 * 1024,
                    HttpRetryPolicy::Never,
                ),
                ServiceKind::Embedding => (
                    serde_json::json!({"model":snapshot.model,"input":"connection test"}),
                    8 * 1024 * 1024,
                    HttpRetryPolicy::OnceTransient,
                ),
                ServiceKind::Tavily => (
                    serde_json::json!({"query":"connection test","max_results":1,"include_answer":false,"include_raw_content":false}),
                    1024 * 1024,
                    HttpRetryPolicy::OnceTransient,
                ),
            };
            let request = JsonRequest {
                endpoint: endpoint(&snapshot)?,
                bearer,
                body,
                timeout: Duration::from_millis(snapshot.timeout_ms as u64),
                max_response_bytes: limit,
                retry_policy: retry,
            };
            let result = tokio::time::timeout_at(deadline, http.post(request, cancel.clone()))
                .await
                .unwrap_or(Err(IntelligenceErrorCode::Timeout))
                .and_then(|r| parse_result(&snapshot, r.status, &r.body));
            let dimensions = result.as_ref().ok().copied().flatten();
            let probe = outcome(started, result.map(|_| ()));
            if !*cancel.borrow() {
                tokio::time::timeout_at(
                    deadline,
                    config.persist_probe(snapshot, revision, probe.clone(), dimensions),
                )
                .await
                .map_err(|_| error(IntelligenceErrorCode::Timeout))??;
            }
            Ok(probe)
        });
        observe(work, deadline).await
    }
    pub(crate) async fn translation(
        &self,
        io: crate::config::ConfigIo,
        factory: Arc<crate::providers::factory::ProviderFactory>,
        store: Arc<crate::storage::LocalStore>,
        id: Uuid,
        operation: OperationLease,
    ) -> AppResult<ConnectionProbe> {
        use crate::domain::session::{SessionConfig, SessionDirection};
        use crate::providers::{
            ProviderError, ProviderEvent,
            factory::volcengine_secret_reference,
            volc::client::{SECRET_SERVICE, secret_key},
        };
        let (lease, _observer, mut cancel) = self.begin(operation)?;
        let deadline = tokio::time::Instant::now() + self.timeout;
        let work = tokio::spawn(async move {
            let started = tokio::time::Instant::now();
            let io = io.with_operation_owner(lease.clone());
            let snapshot = io.run(move |tx| {
                let saved = tx.load_provider_config(id)?;
                if saved.provider() != "volcengine"
                    || saved.secret_reference() != Some(volcengine_secret_reference(id).as_str())
                    || !tx.secret_contains(SECRET_SERVICE, &secret_key(id))?
                {
                    return Err(error(IntelligenceErrorCode::Configuration));
                }
                let encrypted = tx.snapshot_secret(SECRET_SERVICE, &secret_key(id))?;
                Ok((saved, encrypted))
            });
            let (saved, encrypted) = match tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => Err(error(IntelligenceErrorCode::Cancelled)),
                result = tokio::time::timeout_at(deadline, snapshot) => {
                    result.unwrap_or(Err(error(IntelligenceErrorCode::Timeout)))
                }
            } {
                Ok(v) => v,
                Err(e) => return Ok(outcome(started, Err(category(e)))),
            };
            let prepared = (|| {
                let caps = factory.capabilities(&saved)?;
                let pair = caps
                    .language_pairs
                    .first()
                    .cloned()
                    .ok_or(ProviderError::UnsupportedLanguagePair)?;
                let session = SessionConfig::live(pair, SessionDirection::SourceToTarget)
                    .map_err(|_| ProviderError::InvalidConfiguration)?;
                factory.prepare(
                    &saved,
                    Uuid::new_v4(),
                    &session,
                    caps.input_audio,
                    caps.output_audio,
                )
            })();
            let prepared = match prepared {
                Ok(v) => v,
                Err(e) => return Ok(outcome(started, Err(provider_category(e)))),
            };
            let stream = match tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => Err(ProviderError::Cancelled),
                result = tokio::time::timeout_at(deadline, prepared.connect_probe_owned(lease.clone())) => {
                    result.unwrap_or(Err(ProviderError::Timeout))
                }
            } {
                Ok(v) => v.stream,
                Err(e) => return Ok(outcome(started, Err(provider_category(e)))),
            };
            let handshake = async {
                match stream.next_event().await? {
                    Some(ProviderEvent::SessionStarted { .. }) => {}
                    Some(ProviderEvent::Failed { error, .. }) => return Err(error),
                    _ => return Err(ProviderError::Protocol),
                }
                stream.finish().await?;
                loop {
                    match stream.next_event().await? {
                        Some(ProviderEvent::Finished { .. }) => return Ok(()),
                        Some(ProviderEvent::Failed { error, .. }) => return Err(error),
                        None => return Err(ProviderError::Disconnected),
                        _ => {}
                    }
                }
            };
            let result = tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => Err(ProviderError::Cancelled),
                result = tokio::time::timeout_at(deadline, handshake) => {
                    result.unwrap_or(Err(ProviderError::Timeout))
                }
            };
            let _ = tokio::time::timeout(Duration::from_secs(1), stream.cancel()).await;
            // Observation has a deadline; cleanup owns the stream and lease until the
            // concrete adapter acknowledges joined cleanup. An error is uncertainty.
            loop {
                if stream.wait_closed().await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            drop(stream);
            let probe = outcome(started, result.map_err(provider_category));
            if !*cancel.borrow() && tokio::time::Instant::now() < deadline {
                let recorded = probe.clone();
                io.run(move |tx| {
                    let Ok(current) = tx.load_provider_config(id) else { return Ok(()); };
                    if current != saved
                        || current.secret_reference() != Some(volcengine_secret_reference(id).as_str())
                        || tx.snapshot_secret(SECRET_SERVICE, &secret_key(id))? != encrypted
                    { return Ok(()); }
                    store.transaction(|db| {
                        let result = serde_json::to_string(&recorded)
                            .map_err(|_| error(IntelligenceErrorCode::Storage))?;
                        db.execute(
                            "INSERT OR REPLACE INTO service_probes(profile_id,checked_at_ms,result_json) VALUES(?1,?2,?3)",
                            rusqlite::params![id.to_string(), recorded.checked_at_ms, result],
                        )?;
                        Ok(())
                    })
                }).await.map_err(|e| error(category(e)))?;
            }
            drop(lease);
            Ok(probe)
        });
        observe(work, deadline).await
    }
    pub(crate) fn request_shutdown(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        if let Some(cancel) = &state.cancel {
            cancel.send_replace(true);
        }
    }
    pub(crate) async fn shutdown(&self) -> AppResult<()> {
        self.request_shutdown();
        let permit =
            tokio::time::timeout(Duration::from_secs(5), self.gate.clone().acquire_owned())
                .await
                .map_err(|_| error(IntelligenceErrorCode::Interrupted))?
                .map_err(|_| error(IntelligenceErrorCode::Interrupted))?;
        drop(permit);
        Ok(())
    }
    pub(crate) fn reopen(&self) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        if self.gate.available_permits() != 1 {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        state.closed = false;
        Ok(())
    }
}
impl Drop for ProbeOwner {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}
async fn observe(
    work: tokio::task::JoinHandle<AppResult<ConnectionProbe>>,
    deadline: tokio::time::Instant,
) -> AppResult<ConnectionProbe> {
    match tokio::time::timeout_at(deadline, work).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(error(IntelligenceErrorCode::Interrupted)),
        Err(_) => Ok(ConnectionProbe {
            checked_at_ms: now_ms(),
            ok: false,
            latency_ms: None,
            error: Some(IntelligenceErrorCode::Timeout),
        }),
    }
}
fn outcome(
    started: tokio::time::Instant,
    result: Result<(), IntelligenceErrorCode>,
) -> ConnectionProbe {
    ConnectionProbe {
        checked_at_ms: now_ms(),
        ok: result.is_ok(),
        latency_ms: result
            .is_ok()
            .then(|| started.elapsed().as_millis().min(u64::MAX as u128) as u64),
        error: result.err(),
    }
}
fn parse_result(
    profile: &ModelProfile,
    status: u16,
    bytes: &[u8],
) -> Result<Option<u32>, IntelligenceErrorCode> {
    use IntelligenceErrorCode as E;
    if !(200..300).contains(&status) {
        return Err(match status {
            401 | 403 => E::Authentication,
            429 => E::RateLimited,
            _ => E::Network,
        });
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| E::InvalidResponse)?;
    match profile.kind {
        ServiceKind::Llm => {
            if !value["choices"][0]["message"]["content"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty())
            {
                return Err(E::InvalidResponse);
            }
            Ok(None)
        }
        ServiceKind::Tavily => {
            if !value["results"].is_array() {
                return Err(E::InvalidResponse);
            }
            Ok(None)
        }
        ServiceKind::Embedding => {
            let values = value["data"][0]["embedding"]
                .as_array()
                .ok_or(E::InvalidResponse)?;
            let n = values.len() as u32;
            if !(128..=4096).contains(&n)
                || profile.dimensions.is_some_and(|d| d != n)
                || !values
                    .iter()
                    .all(|v| v.as_f64().is_some_and(f64::is_finite))
            {
                return Err(E::InvalidResponse);
            }
            Ok(Some(n))
        }
    }
}
fn provider_category(e: crate::providers::ProviderError) -> IntelligenceErrorCode {
    use crate::providers::ProviderError as P;
    use IntelligenceErrorCode as E;
    match e {
        P::Authentication => E::Authentication,
        P::Timeout => E::Timeout,
        P::Cancelled => E::Cancelled,
        P::Disconnected => E::Network,
        P::Protocol | P::RemoteFailure { .. } => E::InvalidResponse,
        _ => E::Configuration,
    }
}
fn category(e: AppError) -> IntelligenceErrorCode {
    match e {
        AppError::Intelligence(c) => c,
        _ => IntelligenceErrorCode::Configuration,
    }
}
fn error(c: IntelligenceErrorCode) -> AppError {
    AppError::Intelligence(c)
}
#[cfg(test)]
mod tests;
