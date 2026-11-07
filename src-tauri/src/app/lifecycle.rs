//! App-owned subscription admission and cancellation-safe window close barriers.
use super::{AppState, FrontendEmitter, SubscriptionTicket};
use crate::{
    error::{AppError, AppResult},
    events::SessionEvent,
    session::SessionStatus,
};
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
}
pub(super) fn closing_error() -> AppError {
    AppError::InvalidRequest("window is closing; retry after close completes".into())
}

impl AppState {
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
            // Preserve the row when bounded cleanup admission fails, so this
            // command can retry while the mutation guard fences new subscribers.
            owner.close_registry(Some(session_id)).await?;
            owner.orchestrator.delete_session(session_id).await
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
    async fn close_intelligence_owner(&self) -> AppResult<()> {
        let _startup = self.intelligence_startup.lock().await;
        let (jobs, probes, subscriptions) = tokio::join!(
            self.jobs.shutdown(),
            self.probes.shutdown(),
            self.intelligence_subscriptions.close_all()
        );
        // Job cleanup may still need already-issued index ownership. The
        // independent index sender remains alive until the scheduler confirms
        // that workers and child operations have actually drained.
        jobs?;
        self.knowledge.shutdown_searches().await?;
        let _direct_operations = self
            .intelligence_operations
            .clone()
            .acquire_many_owned(16)
            .await
            .map_err(|_| closing_error())?;
        self.index_owner.shutdown().await?;
        probes?;
        subscriptions
    }
    /// Host-only admission for a new window after successful previous cleanup.
    pub fn open_window(&self) -> AppResult<()> {
        let mut state = self.close_state.lock().unwrap();
        if state.completion.is_some() || (state.closing && !state.close_succeeded) {
            return Err(closing_error());
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
                state.closing = true;
                state.close_succeeded = false;
                state.epoch = state.epoch.wrapping_add(1);
                self.close_epoch.send_replace(state.epoch);
                let (finished, completion) = watch::channel(None);
                state.completion = Some(completion.clone());
                // Signal intelligence/probes before waiting for any config or
                // driver admission; prompt audio stop starts in the same owner.
                self.jobs.signal_shutdown();
                self.probes.request_shutdown();
                let owner = self.clone();
                tokio::spawn(async move {
                    let (audio, frontend, intelligence) = tokio::join!(
                        async {
                            // Stop promptly even if unrelated native config work
                            // retains admission; a second pass fences a late start.
                            let early = owner.orchestrator.stop_active().await;
                            let _admission = owner.admission.lock().await;
                            let final_stop = owner.orchestrator.stop_active().await;
                            early.and(final_stop)
                        },
                        async {
                            let _mutation = owner.subscription_mutation.lock().await;
                            owner.close_registry(None).await
                        },
                        async { owner.close_intelligence_owner().await }
                    );
                    let result = audio
                        .and(frontend)
                        .and(intelligence)
                        .map_err(|error| error.to_string());
                    let mut state = owner.close_state.lock().unwrap();
                    state.closing = true;
                    state.close_succeeded = result.is_ok();
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
