//! App-owned subscription admission and cancellation-safe window close barriers.
#[cfg(test)]
#[path = "lifecycle/actual_join_tests.rs"]
mod actual_join_tests;
#[cfg(test)]
#[path = "lifecycle/errors_tests.rs"]
mod errors_tests;
#[cfg(test)]
#[path = "lifecycle/executor_tests.rs"]
mod executor_tests;
#[path = "lifecycle/final_drop.rs"]
mod final_drop;
#[cfg(test)]
#[path = "lifecycle/final_drop_tests.rs"]
mod final_drop_tests;
use super::{AppState, FrontendEmitter, SubscriptionTicket};
use crate::{
    error::{AppError, AppResult},
    events::SessionEvent,
    session::SessionStatus,
};
pub(super) use final_drop::FinalAppFacade;
use std::{sync::Arc, time::Duration};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

// Covers normal capture stop/drain (2+4s), provider finish (10s), playback
// drain (5s), final cleanup (2s), and scheduling. This bounds observation only:
// entered native work keeps its transaction guards and the one cleanup owner.
const CLOSE_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(30);
type CloseResult = Option<Result<(), String>>;
#[derive(Default)]
pub(super) struct CloseState {
    #[cfg(test)]
    pub(super) observation_timeout: Option<Duration>,
    epoch: u64,
    closing: bool,
    close_succeeded: bool,
    completion: Option<watch::Receiver<CloseResult>>,
    audio: Option<crate::audio::join::AudioCloseJoin>,
    audio_key: Option<crate::audio::join::AudioCloseKey>,
    audio_ack: Option<crate::audio::join::AudioCloseAck>,
    terminal_effects_repaired: bool,
    failures: [Option<String>; 9],
    first_failure: Option<[Option<String>; 9]>,
    facade_task: Option<tokio::task::JoinHandle<()>>,
}
pub(super) fn closing_error() -> AppError {
    AppError::InvalidRequest("window is closing; retry after close completes".into())
}
// The retained startup publisher consumes only these narrow ports. It never
// owns an AppState/final facade and is dropped before the live worker loop.
pub(super) struct SummaryPublication {
    state: Arc<std::sync::Mutex<CloseState>>,
    readiness: Arc<std::sync::Mutex<Option<crate::summary::SummaryDeliveryReady>>>,
    orchestrator: crate::session::SessionOrchestrator,
    epoch: u64,
}
impl SummaryPublication {
    pub(super) fn publish(self, ready: &crate::summary::SummaryDeliveryReady) -> AppResult<()> {
        let state = self.state.lock().map_err(|_| closing_error())?;
        if state.closing || state.epoch != self.epoch {
            return Err(closing_error());
        }
        ready.require_epoch(self.epoch)?;
        self.orchestrator
            .install_terminal_port(ready.terminal_port());
        *self.readiness.lock().map_err(|_| closing_error())? = Some(ready.clone());
        Ok(())
    }
}

impl AppState {
    pub(super) fn summary_publication(&self, epoch: u64) -> SummaryPublication {
        SummaryPublication {
            state: self.close_state.clone(),
            readiness: self.summary_ready.clone(),
            orchestrator: self.orchestrator.clone(),
            epoch,
        }
    }
    pub(super) fn with_open_epoch<T>(
        &self,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        let state = self.close_state.lock().unwrap();
        if state.closing {
            return Err(closing_error());
        }
        operation()
    }

    pub(super) fn admitted_epoch(&self) -> AppResult<u64> {
        let state = self.close_state.lock().unwrap();
        if state.closing {
            return Err(closing_error());
        }
        Ok(state.epoch)
    }
    pub(super) fn check_epoch(&self, epoch: u64) -> AppResult<()> {
        if self.admitted_epoch()? != epoch {
            return Err(closing_error());
        }
        Ok(())
    }
    pub async fn subscribe_session(
        &self,
        session_id: Uuid,
        subscriber_id: String,
        emitter: Arc<dyn FrontendEmitter>,
    ) -> AppResult<SubscriptionTicket> {
        let epoch = self.admitted_epoch()?;
        let _permit = self
            .subscription_admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::InvalidRequest("subscription admission is full".into()))?;
        let _mutation = self.subscription_mutation.lock().await;
        self.check_epoch(epoch)?;
        let record = self
            .orchestrator
            .get(session_id)
            .await?
            .ok_or(AppError::SessionNotFound(session_id))?;
        let receiver = match record.status {
            SessionStatus::Completed | SessionStatus::Failed => {
                // Durable history is authoritative even after replay eviction or restart.
                let (sender, receiver) = broadcast::channel(1);
                let status = if record.status == SessionStatus::Completed {
                    "completed"
                } else {
                    "failed"
                };
                let _ = sender.send(SessionEvent::status(session_id, status));
                receiver
            }
            _ => self.orchestrator.subscribe(session_id).await?,
        };
        self.subscriptions
            .subscribe(session_id, subscriber_id, receiver, emitter)
            .await
    }
    pub async fn unsubscribe_session(&self, subscription_id: Uuid) -> AppResult<()> {
        self.subscriptions.unsubscribe(subscription_id).await
    }
    pub async fn delete_session(&self, session_id: Uuid) -> AppResult<()> {
        let epoch = self.admitted_epoch()?;
        self.orchestrator.ensure_session_deletable(session_id)?;
        self.summaries.require_epoch(epoch)?;
        let summary_owner = self.intelligence_lease()?;
        let permit = self
            .subscription_admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::InvalidRequest("subscription admission is full".into()))?;
        let owner = self.clone();
        // Once admitted, cancellation cannot detach a deletion's cleanup.
        tokio::spawn(async move {
            let _permit = permit;
            let _mutation = owner.subscription_mutation.lock().await;
            owner.check_epoch(epoch)?;
            owner.orchestrator.ensure_session_deletable(session_id)?;
            let summaries = owner
                .summaries
                .prepare_session_delete_owned(session_id, summary_owner)
                .await?;
            // Preserve the row when bounded cleanup admission fails, so this
            // command can retry while the mutation guard fences new subscribers.
            owner.close_registry(Some(session_id)).await?;
            owner
                .orchestrator
                .delete_session_guarded(session_id, summaries)
                .await
        })
        .await
        .map_err(|_| AppError::Internal("session deletion owner stopped".into()))?
    }
    async fn close_registry(&self, session_id: Option<Uuid>) -> AppResult<()> {
        // The registry owns a bounded try-admission queue. Retry only its explicit
        // full response; never report a requested but unadmitted barrier as done.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let result = match session_id {
                Some(id) => self.subscriptions.close_session(id).await,
                None => self.subscriptions.close_all().await,
            };
            match result {
                Err(AppError::InvalidRequest(message))
                    if message == "subscription mutation queue is full" =>
                {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(AppError::Internal(
                            "subscription close admission timed out; cleanup remains pending"
                                .into(),
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                result => return result,
            }
        }
    }
    fn record_close_failure(&self, branch: usize, result: &AppResult<()>) {
        self.close_state.lock().unwrap().failures[branch] =
            result.as_ref().err().map(ToString::to_string);
    }
    async fn close_intelligence_owner(
        &self,
        key: crate::audio::join::AudioCloseKey,
        audio: crate::audio::join::AudioCloseJoin,
        terminal_effects: tokio::sync::oneshot::Receiver<()>,
        repair_tail: bool,
    ) -> AppResult<()> {
        let _startup = self.intelligence_startup.lock().await;
        let (jobs, probes, subscriptions, summaries, delivery) = tokio::join!(
            self.jobs.shutdown(),
            self.probes.shutdown(),
            self.intelligence_subscriptions.close_all(),
            self.summaries.drain_actual(),
            async {
                // Recovery can create a new terminal journal after an earlier
                // successful delivery close. Its tail must follow that write.
                terminal_effects.await.map_err(|_| closing_error())?;
                if repair_tail {
                    self.summary_delivery
                        .close_after_terminal_repair_owned(self._process_lock.clone(), key, audio)
                        .await
                } else {
                    self.summary_delivery
                        .close_join_owned(self._process_lock.clone(), key, audio)
                        .await
                }
            },
        );
        self.record_close_failure(2, &jobs);
        self.record_close_failure(3, &probes);
        self.record_close_failure(4, &subscriptions);
        self.record_close_failure(5, &summaries);
        self.record_close_failure(6, &delivery);
        let knowledge = self.knowledge.shutdown_searches().await;
        self.record_close_failure(7, &knowledge);
        // Job cleanup may still need already-issued index ownership. The
        // independent index sender remains alive until the scheduler confirms
        // that workers and child operations have actually drained.
        jobs?;
        delivery?;
        summaries?;
        knowledge?;
        let _direct_operations = self
            .intelligence_operations
            .clone()
            .acquire_many_owned(16)
            .await
            .map_err(|_| closing_error())?;
        let index = self.index_owner.shutdown().await;
        self.record_close_failure(8, &index);
        index?;
        probes?;
        subscriptions
    }
    /// Host-only admission for a new window after successful previous cleanup.
    pub fn open_window(&self) -> AppResult<()> {
        let mut state = self.close_state.lock().unwrap();
        if state.completion.is_some() || (state.closing && !state.close_succeeded) {
            return Err(closing_error());
        }
        if state.closing {
            let ack = state.audio_ack.as_ref().ok_or_else(closing_error)?;
            self.summary_delivery.validate_reopen_after_close(ack)?;
            self.audio.validate_reopen_after_close(ack.backend())?;
            self.orchestrator.validate_reopen_after_close(ack.key())?;
            self.audio.reopen_after_close(ack.backend())?;
            self.orchestrator.reopen_after_close(ack.key())?;
            self.summary_delivery.reopen_after_close(ack)?;
            state.audio = None;
            state.audio_key = None;
            state.audio_ack = None;
            state.terminal_effects_repaired = false;
            self.summary_ready.lock().unwrap().take();
        }
        state.epoch = state.epoch.wrapping_add(1);
        state.closing = false;
        self.close_epoch.send_replace(state.epoch);
        Ok(())
    }

    /// An owned, coalesced shutdown operation. Stops the authoritative runtime,
    /// drains prior subscription work, and leaves admission closed until the host
    /// explicitly begins a new window lifecycle.
    /// Failure keeps admission closed; callers may retry and must observe success.
    /// Observation timeout/cancellation never cancels the retained cleanup owner.
    pub async fn close_all(&self) -> AppResult<()> {
        #[cfg(test)]
        let observation_timeout = self
            .close_state
            .lock()
            .unwrap()
            .observation_timeout
            .unwrap_or(CLOSE_OBSERVATION_TIMEOUT);
        #[cfg(not(test))]
        let observation_timeout = CLOSE_OBSERVATION_TIMEOUT;
        let deadline = tokio::time::Instant::now() + observation_timeout;
        let mut completion = {
            let mut state = self.close_state.lock().unwrap();
            if state.closing && state.close_succeeded {
                return Ok(());
            }
            if let Some(completion) = &state.completion {
                completion.clone()
            } else {
                self._final_facade.record_executor();
                let retry = state.audio_ack.is_some();
                if !state.closing {
                    state.epoch = state.epoch.checked_add(1).ok_or_else(closing_error)?;
                    self.close_epoch.send_replace(state.epoch);
                }
                state.closing = true;
                state.close_succeeded = false;
                self.summary_ready.lock().unwrap().take();
                let key = self
                    .summary_delivery
                    .elect_closing(state.epoch, self.audio.actual_epoch()?)?;
                self.summary_delivery.signal_closing(&key)?;
                self.summaries.signal_closing()?;
                let audio_join = if let Some(join) = &state.audio {
                    join.clone()
                } else {
                    let native = self.audio.begin_close(key.clone())?;
                    let session = self.orchestrator.capture_close_join(key.clone())?;
                    let audio_owner = self.clone();
                    let join = crate::audio::join::AudioCloseJoin::start_owned(
                        self._process_lock.clone(),
                        key.clone(),
                        session,
                        native,
                        async move {
                            let early = audio_owner.orchestrator.stop_active().await;
                            let _admission = audio_owner.admission.lock().await;
                            let final_stop = audio_owner.orchestrator.stop_active().await;
                            early.and(final_stop)
                        },
                    )?;
                    state.audio = Some(join.clone());
                    state.audio_key = Some(key.clone());
                    join
                };
                let (finished, completion) = watch::channel(None);
                state.completion = Some(completion.clone());
                // Signal intelligence/probes before waiting for any config or
                // driver admission; prompt audio stop starts in the same owner.
                self.jobs.signal_shutdown();
                self.probes.request_shutdown();
                let owner = self.clone();
                let repair_tail = retry && audio_join.outcome().is_err();
                let (terminal_effects, before_tail) = tokio::sync::oneshot::channel();
                tokio::spawn(async move {
                    let (audio, frontend, intelligence) = tokio::join!(
                        async {
                            let result = async {
                                let ack = audio_join.wait().await?;
                                ack.require_key(&key)?;
                                owner.close_state.lock().unwrap().audio_ack = Some(ack);
                                if repair_tail {
                                    if !owner.close_state.lock().unwrap().terminal_effects_repaired
                                    {
                                        owner.orchestrator.retry_terminal_close_effects(&key)?;
                                        // Cache only this producer's verified same-key result;
                                        // a later failed delivery tail still needs its own retry.
                                        owner
                                            .close_state
                                            .lock()
                                            .unwrap()
                                            .terminal_effects_repaired = true;
                                    }
                                    Ok(())
                                } else {
                                    audio_join.outcome()
                                }
                            }
                            .await;
                            let _ = terminal_effects.send(());
                            result
                        },
                        async {
                            let _mutation = owner.subscription_mutation.lock().await;
                            owner.close_registry(None).await
                        },
                        async {
                            owner
                                .close_intelligence_owner(
                                    key.clone(),
                                    audio_join.clone(),
                                    before_tail,
                                    repair_tail,
                                )
                                .await
                        }
                    );
                    owner.record_close_failure(0, &audio);
                    owner.record_close_failure(1, &frontend);
                    let result = audio
                        .and(frontend)
                        .and(intelligence)
                        .map_err(|error| error.to_string());
                    let mut state = owner.close_state.lock().unwrap();
                    state.closing = true;
                    state.close_succeeded = result.is_ok();
                    if result.is_err() && state.first_failure.is_none() {
                        state.first_failure = Some(state.failures.clone());
                    }
                    state.completion = None;
                    finished.send_replace(Some(result));
                });
                completion
            }
        };
        tokio::time::timeout_at(deadline, async {
            loop {
                if let Some(result) = completion.borrow_and_update().clone() {
                    return result.map_err(AppError::Internal);
                }
                completion
                    .changed()
                    .await
                    .map_err(|_| AppError::Internal("window close owner stopped".into()))?;
            }
        })
        .await
        .map_err(|_| {
            AppError::Internal("window close observation timed out; cleanup remains pending".into())
        })?
    }
}
