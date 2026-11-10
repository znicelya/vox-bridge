//! One bounded delivery owner. Native callbacks reserve only a UUID hint;
//! authority and exactly-once disposition live in the existing SQL journal.
use super::{
    SummaryReady, SummaryService,
    store::{error, storage, uuid},
};
use crate::{
    audio::join::{AudioCloseAck, AudioCloseJoin, AudioCloseKey},
    error::{AppError, AppResult},
    intelligence::{
        config::{ServiceConfigStore, SummarySelection},
        jobs::{JobAdmissionFence, JobEvents, JobScheduler, OperationLease},
        store::{JobAdmissionResult, permanent_job, safe_store_error},
        types::{IntelligenceErrorCode as Code, JobKind},
    },
    storage::{LocalStore, SqlSlot},
};
use futures_util::FutureExt;
use rusqlite::{OptionalExtension, Transaction, params};
use std::{
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, OwnedSemaphorePermit, Semaphore, mpsc, watch},
    task::JoinHandle,
};
use uuid::Uuid;

const CHANNEL: usize = 32;
const PAGE: usize = 32;
const RESCAN: Duration = Duration::from_millis(200);
const START_OBSERVATION: Duration = Duration::from_secs(10);
const CLOSE_OBSERVATION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
pub(crate) enum LocalRetryReason {
    Busy,
    Interrupted,
}
impl LocalRetryReason {
    fn code(self) -> Code {
        match self {
            Self::Busy => Code::Busy,
            Self::Interrupted => Code::Interrupted,
        }
    }
}
pub(crate) struct TerminalCommitIntent {
    delivery_id: Option<Uuid>,
    local_error: Option<LocalRetryReason>,
}
impl TerminalCommitIntent {
    pub(crate) fn journal_fields(&self) -> (Option<String>, Option<&'static str>) {
        (
            self.delivery_id.map(|id| id.to_string()),
            self.local_error.map(|reason| reason.code().as_str()),
        )
    }
}
pub(crate) struct TerminalReservation {
    intent: TerminalCommitIntent,
    permit: Option<mpsc::OwnedPermit<Uuid>>,
    rescan: Arc<Notify>,
}
#[derive(Clone)]
pub(crate) struct SessionTerminalPort {
    delivery_id: Option<Uuid>,
    live: Arc<AtomicBool>,
    sender: Option<mpsc::Sender<Uuid>>,
    rescan: Arc<Notify>,
}
impl SessionTerminalPort {
    pub(crate) fn unavailable() -> Self {
        Self {
            delivery_id: None,
            live: Arc::new(AtomicBool::new(false)),
            sender: None,
            rescan: Arc::new(Notify::new()),
        }
    }
    pub(crate) fn try_reserve(&self) -> TerminalReservation {
        let mut reason = Some(LocalRetryReason::Interrupted);
        let mut permit = None;
        if self.live.load(Ordering::Acquire) {
            if let Some(sender) = &self.sender {
                match sender.clone().try_reserve_owned() {
                    Ok(slot) => {
                        permit = Some(slot);
                        reason = None;
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        reason = Some(LocalRetryReason::Busy)
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {}
                }
            }
        }
        TerminalReservation {
            intent: TerminalCommitIntent {
                delivery_id: self.delivery_id,
                local_error: reason,
            },
            permit,
            rescan: self.rescan.clone(),
        }
    }
}
impl TerminalReservation {
    pub(crate) fn intent(&self) -> &TerminalCommitIntent {
        &self.intent
    }
    pub(crate) fn committed(self, session_id: Uuid) {
        if let Some(permit) = self.permit {
            permit.send(session_id);
        }
        self.rescan.notify_one();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Dormant,
    Bootstrapping,
    Live,
    Closing,
    Failed,
    Joined,
}
struct State {
    id: Uuid,
    epoch: Option<u64>,
    stage: Stage,
    live: Arc<AtomicBool>,
    sender: Option<mpsc::Sender<Uuid>>,
    rescan: Arc<Notify>,
    seal: Option<watch::Sender<bool>>,
    worker_done: Option<watch::Receiver<Option<Result<(), Code>>>>,
    close_key: Option<AudioCloseKey>,
    close_done: Option<watch::Receiver<Option<Result<(), Code>>>>,
}
pub(super) struct DeliveryControl {
    state: Mutex<State>,
    child: Arc<Semaphore>,
    sql: SqlSlot,
    worker: AsyncMutex<Option<JoinHandle<()>>>,
    closer: AsyncMutex<Option<JoinHandle<()>>>,
    actual_join_failed: AtomicBool,
    #[cfg(test)]
    publication: Arc<readiness_test_support::BoundaryControl>,
    #[cfg(test)]
    ready_issued: Arc<readiness_test_support::BoundaryControl>,
    #[cfg(test)]
    ready_published: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}
// Only external service/delivery facades hold this guard. The narrow worker
// retains control, never the guard whose disappearance ends live admission.
pub(super) struct DeliveryFacade {
    control: Arc<DeliveryControl>,
}
impl DeliveryFacade {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            control: DeliveryControl::new(),
        })
    }
}
impl Drop for DeliveryFacade {
    fn drop(&mut self) {
        if let Ok(mut state) = self.control.state.lock() {
            state.live.store(false, Ordering::Release);
            if state.stage != Stage::Joined {
                state.stage = Stage::Closing;
            }
            state.sender.take();
            state.seal.take();
            state.rescan.notify_one();
        }
    }
}
impl DeliveryControl {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                id: Uuid::new_v4(),
                epoch: None,
                stage: Stage::Dormant,
                live: Arc::new(AtomicBool::new(false)),
                sender: None,
                rescan: Arc::new(Notify::new()),
                seal: None,
                worker_done: None,
                close_key: None,
                close_done: None,
            }),
            child: Arc::new(Semaphore::new(1)),
            sql: SqlSlot::default(),
            worker: AsyncMutex::new(None),
            closer: AsyncMutex::new(None),
            actual_join_failed: AtomicBool::new(false),
            #[cfg(test)]
            publication: Arc::default(),
            #[cfg(test)]
            ready_issued: Arc::default(),
            #[cfg(test)]
            ready_published: Mutex::new(None),
        })
    }
    fn local(self: &Arc<Self>, id: Uuid) -> SummaryLocalFence {
        SummaryLocalFence {
            control: self.clone(),
            delivery_id: id,
        }
    }
    fn run_live<T>(
        &self,
        id: Uuid,
        epoch: u64,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        let state = self.state.lock().map_err(|_| storage())?;
        if state.id != id
            || state.epoch != Some(epoch)
            || state.stage != Stage::Live
            || !state.live.load(Ordering::Acquire)
        {
            return Err(error(Code::Interrupted));
        }
        operation()
    }
    fn fail(&self, id: Uuid) {
        if let Ok(mut state) = self.state.lock() {
            if state.id == id && !matches!(state.stage, Stage::Closing | Stage::Joined) {
                state.live.store(false, Ordering::Release);
                state.stage = Stage::Failed;
            }
        }
    }
    async fn child_barrier(&self) -> AppResult<()> {
        let sql = self.sql.drain().await;
        let _all = self
            .child
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| error(Code::Interrupted))?;
        if sql.is_err() {
            self.actual_join_failed.store(true, Ordering::Release);
        }
        sql
    }
    async fn worker_outcome(&self) -> AppResult<()> {
        let completion = self
            .state
            .lock()
            .map_err(|_| storage())?
            .worker_done
            .clone();
        if let Some(completion) = completion {
            observed(completion).await
        } else {
            Ok(())
        }
    }
    async fn join_worker(&self) -> AppResult<()> {
        let result = self.worker_outcome().await;
        self.join_worker_actual().await?;
        result
    }
    async fn join_worker_actual(&self) -> AppResult<()> {
        let mut worker = self.worker.lock().await;
        if let Some(task) = worker.as_mut() {
            let joined = task.await;
            worker.take();
            return self.record_actual_join(joined);
        }
        self.actual_join_result()
    }
    async fn join_closer_actual(&self) -> AppResult<()> {
        let mut closer = self.closer.lock().await;
        if let Some(task) = closer.as_mut() {
            let joined = task.await;
            closer.take();
            return self.record_actual_join(joined);
        }
        self.actual_join_result()
    }
    fn record_actual_join(&self, joined: Result<(), tokio::task::JoinError>) -> AppResult<()> {
        if joined.is_err() {
            self.actual_join_failed.store(true, Ordering::Release);
        }
        self.actual_join_result()
    }
    fn actual_join_result(&self) -> AppResult<()> {
        if self.actual_join_failed.load(Ordering::Acquire) || self.sql.failed() {
            return Err(error(Code::Interrupted));
        }
        Ok(())
    }
    async fn join_final_actual(&self, outcome: AppResult<()>) -> AppResult<FinalDeliveryJoin> {
        let closer = self.join_closer_actual().await;
        let worker = self.join_worker_actual().await;
        let child = self.child_barrier().await;
        closer?;
        worker?;
        child?;
        Ok(FinalDeliveryJoin { outcome })
    }
}
#[derive(Clone)]
pub(crate) struct SummaryLocalFence {
    control: Arc<DeliveryControl>,
    delivery_id: Uuid,
}
impl SummaryLocalFence {
    fn run<T>(
        &self,
        reason: LocalRetryReason,
        operation: impl FnOnce(LocalRetryReason) -> AppResult<T>,
    ) -> AppResult<T> {
        let state = self.control.state.lock().map_err(|_| storage())?;
        if state.id != self.delivery_id
            || !matches!(
                state.stage,
                Stage::Bootstrapping | Stage::Live | Stage::Closing | Stage::Failed
            )
        {
            return Err(error(Code::Interrupted));
        }
        let reason = if state.stage == Stage::Live {
            reason
        } else {
            LocalRetryReason::Interrupted
        };
        operation(reason)
    }
}
pub(crate) enum AutomaticAttempt {
    Live {
        delivery_id: Uuid,
        job_fence: JobAdmissionFence,
    },
    LocalRetry {
        delivery_id: Uuid,
        reason: LocalRetryReason,
        local_fence: SummaryLocalFence,
    },
}
#[derive(Clone)]
pub(crate) struct SummaryDeliveryReady {
    control: Arc<DeliveryControl>,
    delivery_id: Uuid,
    epoch: u64,
    local_ready: SummaryReady,
    terminal: SessionTerminalPort,
}
impl SummaryDeliveryReady {
    pub(crate) fn require_epoch(&self, epoch: u64) -> AppResult<()> {
        if epoch != self.epoch {
            return Err(error(Code::Interrupted));
        }
        self.run(|| Ok(()))
    }
    pub(crate) fn run<T>(&self, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        self.control.run_live(self.delivery_id, self.epoch, || {
            self.local_ready.run(operation)
        })
    }
    pub(super) fn require_owner(&self, delivery: &Arc<DeliveryFacade>) -> AppResult<()> {
        if !Arc::ptr_eq(&self.control, &delivery.control) {
            return Err(error(Code::Interrupted));
        }
        self.require_epoch(self.epoch)
    }
    pub(crate) fn terminal_port(&self) -> SessionTerminalPort {
        self.terminal.clone()
    }
}
#[derive(Clone)]
struct AutomaticPort {
    store: Arc<LocalStore>,
    config: Arc<ServiceConfigStore>,
    jobs: Weak<JobScheduler>,
    events: JobEvents,
    wake: Arc<Notify>,
    control: Arc<DeliveryControl>,
}
#[derive(Clone)]
pub(crate) struct SummaryDeliveryOwner {
    port: AutomaticPort,
    _facade: Arc<DeliveryFacade>,
}
// Only positive closer/worker/child joins construct this capability. A failed
// durable/local result remains separate from unknown actual task completion.
pub(crate) struct FinalDeliveryJoin {
    outcome: AppResult<()>,
}
impl FinalDeliveryJoin {
    pub(crate) fn outcome(self) -> AppResult<()> {
        self.outcome
    }
}
struct ChildOwner {
    _process: OperationLease,
    _slot: OwnedSemaphorePermit,
}
struct ChildFactory {
    process: Weak<dyn Send + Sync>,
    slot: Arc<Semaphore>,
    closed: AtomicBool,
}
struct Startup {
    epoch: u64,
    local_ready: SummaryReady,
    terminal: SessionTerminalPort,
    started: watch::Sender<Option<Result<SummaryDeliveryReady, Code>>>,
    publish: Box<dyn FnOnce(&SummaryDeliveryReady) -> AppResult<()> + Send>,
}
impl ChildFactory {
    async fn enter(&self) -> AppResult<OperationLease> {
        if self.closed.load(Ordering::Acquire) {
            return Err(error(Code::Interrupted));
        }
        let slot = self
            .slot
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| error(Code::Interrupted))?;
        if self.closed.load(Ordering::Acquire) {
            return Err(error(Code::Interrupted));
        }
        let process = self
            .process
            .upgrade()
            .ok_or_else(|| error(Code::Interrupted))?;
        Ok(Arc::new(ChildOwner {
            _process: process,
            _slot: slot,
        }))
    }
}
impl SummaryService {
    pub(crate) fn delivery_owner(&self) -> SummaryDeliveryOwner {
        SummaryDeliveryOwner {
            port: AutomaticPort {
                store: self.store.clone(),
                config: self.config.clone(),
                jobs: Arc::downgrade(&self.jobs),
                events: self.jobs.event_port(),
                wake: self.jobs.wake_port(),
                control: self.delivery.control.clone(),
            },
            _facade: self.delivery.clone(),
        }
    }
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Required owned facade companion; retained delivery uses the same narrow admission port without holding the strong service/scheduler facade"
        )
    )]
    pub(crate) async fn after_terminal_owned(
        &self,
        session_id: Uuid,
        attempt: AutomaticAttempt,
        owner: OperationLease,
    ) -> AppResult<()> {
        match self
            .delivery_owner()
            .port
            .admit(session_id, attempt, owner, false)
            .await?
        {
            Decision::ReadOnlyExisting => Ok(()),
            decision => decision.result(),
        }
    }
}

#[cfg(test)]
pub(crate) mod readiness_test_support;
#[cfg(test)]
mod tests;
enum Decision {
    Job(JobAdmissionResult),
    Cleared,
    Skipped,
    ReadOnlyExisting,
    ResourceLimit,
}
impl Decision {
    fn result(self) -> AppResult<()> {
        match self {
            Self::ResourceLimit => Err(error(Code::ResourceLimit)),
            Self::ReadOnlyExisting => Err(error(Code::Configuration)),
            _ => Ok(()),
        }
    }
}
impl AutomaticPort {
    async fn admit(
        &self,
        session_id: Uuid,
        attempt: AutomaticAttempt,
        owner: OperationLease,
        bootstrap: bool,
    ) -> AppResult<Decision> {
        let control = self.control.clone();
        let events = self.events.clone();
        let wake = self.wake.clone();
        let result = self.config.scoped(owner.clone()).with_summary_selection_completion(
            SummarySelection::Automatic,
            move |tx, snapshot| {
                let Some(journal) = LocalStore::summary_terminal_in_tx(tx, session_id)? else { return Ok(Decision::Skipped); };
                if journal.deleting || !journal.terminal || (!bootstrap && !journal.pending) { return Ok(Decision::Skipped); }
                let run = |initial_failure| {
                    // The permanent marker is authoritative before settings,
                    // counters or version allocation, including deleted output.
                    if let Some(existing) = permanent_job(tx, &format!("auto-summary:{session_id}"))? {
                        LocalStore::consume_summary_terminal_in_tx(tx, session_id)?;
                        return Ok(Decision::Job(JobAdmissionResult::Existing(existing)));
                    }
                    if !snapshot.automatic_summary {
                        LocalStore::consume_summary_terminal_in_tx(tx, session_id)?;
                        return Ok(Decision::Cleared);
                    }
                    let admitted = LocalStore::create_summary_job_in_tx(tx, super::SummaryJobRequest {
                        session_id, profile: if snapshot.selection_error.is_some() { None } else { snapshot.profile }, automatic: true, initial_failure,
                    });
                    match admitted {
                        Ok(result) => { LocalStore::consume_summary_terminal_in_tx(tx, session_id)?; Ok(Decision::Job(result)) }
                        Err(AppError::Intelligence(Code::ResourceLimit)) => {
                            tx.execute("UPDATE session_lifecycle SET summary_terminal_pending=1,summary_terminal_error='resource_limit' WHERE session_id=?1", [session_id.to_string()])?;
                            Ok(Decision::ResourceLimit)
                        }
                        Err(error) => Err(error),
                    }
                };
                match &attempt {
                    AutomaticAttempt::Live { delivery_id, job_fence } => {
                        let state = control.state.lock().map_err(|_| storage())?;
                        if state.id != *delivery_id || !matches!(state.stage, Stage::Live | Stage::Closing | Stage::Failed) { return Err(error(Code::Interrupted)); }
                        if state.stage == Stage::Live && state.live.load(Ordering::Acquire) {
                            if journal.generation != Some(*delivery_id) || journal.local_error.is_some() { return Err(error(Code::Interrupted)); }
                            job_fence.run(|| run(None))
                        } else {
                            // The config observer may enter while Live and reach
                            // this SQL fence after Closing. Its original child
                            // can only commit a local terminal disposition.
                            run(Some(Code::Interrupted))
                        }
                    },
                    AutomaticAttempt::LocalRetry { delivery_id, reason, local_fence } => {
                        if *delivery_id != local_fence.delivery_id { return Err(error(Code::Interrupted)); }
                        local_fence.run(*reason, |reason| run(Some(reason.code())))
                    }
                }
            },
            move |decision| {
                if let Decision::Job(JobAdmissionResult::Created(job)) = decision { events.publish(job.clone()); wake.notify_one(); }
            },
        ).await;
        match result {
            Err(AppError::Intelligence(Code::Configuration)) | Err(AppError::Configuration(_)) => {
                let storage = self.store.clone();
                self.control
                    .sql
                    .run(owner, move || {
                        storage
                            .transaction(|tx| {
                                let Some(journal) =
                                    LocalStore::summary_terminal_in_tx(tx, session_id)?
                                else {
                                    return Ok(Decision::Skipped);
                                };
                                if journal.deleting
                                    || !journal.terminal
                                    || (!bootstrap && !journal.pending)
                                {
                                    return Ok(Decision::Skipped);
                                }
                                if permanent_job(tx, &format!("auto-summary:{session_id}"))?
                                    .is_some()
                                {
                                    Ok(Decision::ReadOnlyExisting)
                                } else {
                                    Err(error(Code::Configuration))
                                }
                            })
                            .map_err(safe_store_error)
                    })
                    .await
            }
            result => result,
        }
    }
    async fn attempt(
        &self,
        session_id: Uuid,
        bootstrap: bool,
        owner: OperationLease,
    ) -> AppResult<AutomaticAttempt> {
        let attempt_store = self.store.clone();
        let journal = self
            .control
            .sql
            .run(owner, move || {
                attempt_store
                    .transaction(|tx| LocalStore::summary_terminal_in_tx(tx, session_id))
                    .map_err(safe_store_error)
            })
            .await?;
        let state = self.control.state.lock().map_err(|_| storage())?;
        let id = state.id;
        let live = !bootstrap
            && state.stage == Stage::Live
            && state.live.load(Ordering::Acquire)
            && journal
                .as_ref()
                .is_some_and(|j| j.generation == Some(id) && j.local_error.is_none());
        let reason = if !bootstrap
            && state.stage == Stage::Live
            && journal.as_ref().is_some_and(|j| {
                j.generation == Some(id) && j.local_error.as_deref() == Some("busy")
            }) {
            LocalRetryReason::Busy
        } else {
            LocalRetryReason::Interrupted
        };
        drop(state);
        if live {
            let fence = self
                .jobs
                .upgrade()
                .ok_or_else(|| error(Code::Interrupted))?
                .admission_fence(JobKind::Summary);
            if let Ok(job_fence) = fence {
                return Ok(AutomaticAttempt::Live {
                    delivery_id: id,
                    job_fence,
                });
            }
        }
        Ok(AutomaticAttempt::LocalRetry {
            delivery_id: id,
            reason,
            local_fence: self.control.local(id),
        })
    }
    async fn one(&self, id: Uuid, factory: &ChildFactory, bootstrap: bool) -> AppResult<()> {
        let child = factory.enter().await?;
        let attempt = self.attempt(id, bootstrap, child.clone()).await?;
        let result = self.admit(id, attempt, child, bootstrap).await;
        // Observer timeout does not release the entered config/SQL child. No
        // next admission is dispatched before its real release and late read.
        self.control.child_barrier().await?;
        if result.is_err() {
            let storage = self.store.clone();
            self.control
                .sql
                .run(factory.enter().await?, move || {
                    storage
                        .transaction(|tx| {
                            if let Some(journal) = LocalStore::summary_terminal_in_tx(tx, id)? {
                                if journal.terminal {
                                    let _ = permanent_job(tx, &format!("auto-summary:{id}"))?;
                                }
                            }
                            Ok(())
                        })
                        .map_err(safe_store_error)
                })
                .await?;
        }
        result?.result()
    }
    async fn scan(&self, factory: &ChildFactory, bootstrap: bool) -> AppResult<()> {
        for status in if bootstrap {
            vec![Some("completed"), Some("failed")]
        } else {
            vec![None]
        } {
            let mut after = None;
            loop {
                if bootstrap
                    && self.control.state.lock().map_err(|_| storage())?.stage
                        != Stage::Bootstrapping
                {
                    return Err(error(Code::Interrupted));
                }
                let storage = self.store.clone();
                let page_after = after.clone();
                let page = self
                    .control
                    .sql
                    .run(factory.enter().await?, move || {
                        storage.summary_terminal_page(status, page_after.as_deref())
                    })
                    .await?;
                if page.is_empty() {
                    break;
                }
                after = page.last().map(Uuid::to_string);
                let more = page.len() == PAGE;
                for id in page {
                    self.one(id, factory, bootstrap).await?;
                }
                if !more {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }
        Ok(())
    }
}
impl SummaryDeliveryOwner {
    #[cfg(test)]
    pub(crate) fn observed_terminal_for_tests(
        &self,
    ) -> AppResult<(SessionTerminalPort, mpsc::Receiver<Uuid>, Arc<Notify>)> {
        let state = self.port.control.state.lock().map_err(|_| storage())?;
        let (sender, receiver) = mpsc::channel(CHANNEL);
        let rescan = Arc::new(Notify::new());
        Ok((
            SessionTerminalPort {
                delivery_id: Some(state.id),
                live: state.live.clone(),
                sender: Some(sender),
                rescan: rescan.clone(),
            },
            receiver,
            rescan,
        ))
    }
    pub(crate) fn delivery_id(&self) -> AppResult<Uuid> {
        Ok(self.port.control.state.lock().map_err(|_| storage())?.id)
    }
    #[cfg(test)]
    pub(crate) fn require_epoch(&self, epoch: u64) -> AppResult<()> {
        let state = self.port.control.state.lock().map_err(|_| storage())?;
        if state.stage != Stage::Live
            || state.epoch != Some(epoch)
            || !state.live.load(Ordering::Acquire)
        {
            return Err(error(Code::Interrupted));
        }
        Ok(())
    }
    pub(crate) fn ensure_startable(&self) -> AppResult<()> {
        self.port.control.actual_join_result()?;
        let state = self.port.control.state.lock().map_err(|_| storage())?;
        if matches!(
            state.stage,
            Stage::Live | Stage::Closing | Stage::Bootstrapping
        ) || state
            .worker_done
            .as_ref()
            .is_some_and(|done| done.borrow().is_none())
        {
            return Err(error(Code::Busy));
        }
        Ok(())
    }
    fn with_joined_close<T>(
        &self,
        ack: &AudioCloseAck,
        operation: impl FnOnce(&mut State) -> T,
    ) -> AppResult<T> {
        let worker = self
            .port
            .control
            .worker
            .try_lock()
            .map_err(|_| error(Code::Busy))?;
        let closer = self
            .port
            .control
            .closer
            .try_lock()
            .map_err(|_| error(Code::Busy))?;
        let _child = self
            .port
            .control
            .child
            .clone()
            .try_acquire_owned()
            .map_err(|_| error(Code::Busy))?;
        let mut state = self.port.control.state.lock().map_err(|_| storage())?;
        if self.port.control.actual_join_failed.load(Ordering::Acquire)
            || worker.is_some()
            || closer.is_some()
            || state.id != ack.key().delivery_id()
            || state.close_key.as_ref() != Some(ack.key())
            || state.live.load(Ordering::Acquire)
            || !state
                .worker_done
                .as_ref()
                .is_some_and(|done| done.borrow().as_ref().is_some_and(Result::is_ok))
            || !state
                .close_done
                .as_ref()
                .is_some_and(|done| done.borrow().as_ref().is_some_and(Result::is_ok))
        {
            return Err(error(Code::Interrupted));
        }
        ack.require_key(
            state
                .close_key
                .as_ref()
                .ok_or_else(|| error(Code::Interrupted))?,
        )?;
        Ok(operation(&mut state))
    }
    pub(crate) fn validate_reopen_after_close(&self, ack: &AudioCloseAck) -> AppResult<()> {
        self.with_joined_close(ack, |_| ())
    }
    pub(crate) fn reopen_after_close(&self, ack: &AudioCloseAck) -> AppResult<()> {
        self.with_joined_close(ack, |state| {
            state.id = Uuid::new_v4();
            state.epoch = None;
            state.stage = Stage::Dormant;
            state.live = Arc::new(AtomicBool::new(false));
            state.sender = None;
            state.rescan = Arc::new(Notify::new());
            state.seal = None;
            state.worker_done = None;
            state.close_key = None;
            state.close_done = None;
        })
    }
    pub(crate) fn elect_closing(
        &self,
        application_epoch: u64,
        backend_epoch: u64,
    ) -> AppResult<AudioCloseKey> {
        let mut state = self.port.control.state.lock().map_err(|_| storage())?;
        let key = state
            .close_key
            .clone()
            .unwrap_or_else(|| AudioCloseKey::new(application_epoch, state.id, backend_epoch));
        if key.application_epoch() != application_epoch || key.backend_epoch() != backend_epoch {
            return Err(error(Code::Interrupted));
        }
        state.live.store(false, Ordering::Release);
        state.stage = Stage::Closing;
        state.close_key = Some(key.clone());
        state.rescan.notify_one();
        Ok(key)
    }
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Owned standalone delivery companion; App supplies its narrow startup publication"
        )
    )]
    pub(crate) async fn start_owned(
        &self,
        owner: OperationLease,
        epoch: u64,
        local_ready: SummaryReady,
    ) -> AppResult<SummaryDeliveryReady> {
        self.start_published_owned(owner, epoch, local_ready, |_| Ok(()))
            .await
    }
    pub(crate) async fn start_published_owned(
        &self,
        owner: OperationLease,
        epoch: u64,
        local_ready: SummaryReady,
        publish: impl FnOnce(&SummaryDeliveryReady) -> AppResult<()> + Send + 'static,
    ) -> AppResult<SummaryDeliveryReady> {
        local_ready.require_epoch(epoch)?;
        self.ensure_startable()?;
        let _ = self.port.control.join_worker().await;
        self.port.control.child_barrier().await?;
        let (started, mut ready) = watch::channel(None);
        let (receiver, seal, id, terminal) = {
            let mut state = self.port.control.state.lock().map_err(|_| storage())?;
            if matches!(
                state.stage,
                Stage::Live | Stage::Closing | Stage::Bootstrapping
            ) {
                return Err(error(Code::Busy));
            }
            let id = Uuid::new_v4();
            let (sender, receiver) = mpsc::channel(CHANNEL);
            let (seal, sealed) = watch::channel(false);
            state.id = id;
            state.epoch = Some(epoch);
            state.stage = Stage::Bootstrapping;
            state.live = Arc::new(AtomicBool::new(false));
            state.rescan = Arc::new(Notify::new());
            state.sender = Some(sender.clone());
            state.seal = Some(seal);
            state.close_key = None;
            state.close_done = None;
            let terminal = SessionTerminalPort {
                delivery_id: Some(id),
                live: state.live.clone(),
                sender: Some(sender),
                rescan: state.rescan.clone(),
            };
            (receiver, sealed, id, terminal)
        };
        self.spawn_worker(
            owner,
            receiver,
            seal,
            id,
            Some(Startup {
                epoch,
                local_ready,
                terminal,
                started,
                publish: Box::new(publish),
            }),
        )?;
        let observed = tokio::time::timeout(START_OBSERVATION, async {
            loop {
                if let Some(result) = ready.borrow_and_update().clone() {
                    return result.map_err(error);
                }
                ready
                    .changed()
                    .await
                    .map_err(|_| error(Code::Interrupted))?;
            }
        })
        .await;
        match observed {
            Ok(result) => result,
            Err(_) => {
                self.port.control.fail(id);
                Err(error(Code::Timeout))
            }
        }
    }
    fn spawn_worker(
        &self,
        owner: OperationLease,
        mut receiver: mpsc::Receiver<Uuid>,
        mut sealed: watch::Receiver<bool>,
        id: Uuid,
        startup: Option<Startup>,
    ) -> AppResult<()> {
        let mut worker_slot = self
            .port
            .control
            .worker
            .try_lock()
            .map_err(|_| error(Code::Busy))?;
        let factory = ChildFactory {
            process: Arc::downgrade(&owner),
            slot: self.port.control.child.clone(),
            closed: AtomicBool::new(false),
        };
        let (done, completion) = watch::channel(None);
        self.port
            .control
            .state
            .lock()
            .map_err(|_| storage())?
            .worker_done = Some(completion);
        let port = self.port.clone();
        let task = tokio::spawn(async move {
            let mut failure = None;
            if let Some(Startup {
                epoch,
                local_ready,
                terminal,
                started,
                publish,
            }) = startup
            {
                let bootstrap = port.scan(&factory, true).await.and_then(|()| {
                    local_ready.require_epoch(epoch)?;
                    let mut state = port.control.state.lock().map_err(|_| storage())?;
                    if state.id != id || state.stage != Stage::Bootstrapping {
                        return Err(error(Code::Interrupted));
                    }
                    let jobs = port
                        .jobs
                        .upgrade()
                        .ok_or_else(|| error(Code::Interrupted))?;
                    jobs.start_owned(owner.clone())?;
                    jobs.admission_fence(JobKind::Summary)?.run(|| Ok(()))?;
                    #[cfg(test)]
                    port.control.publication.enter();
                    state.stage = Stage::Live;
                    state.live.store(true, Ordering::Release);
                    let ready = SummaryDeliveryReady {
                        control: port.control.clone(),
                        delivery_id: id,
                        epoch,
                        local_ready,
                        terminal,
                    };
                    // App publication takes CloseState before delivery/local
                    // readiness. Never hold the inverse delivery→App order.
                    drop(state);
                    #[cfg(test)]
                    port.control.ready_issued.enter();
                    publish(&ready)?;
                    Ok(ready)
                });
                match bootstrap {
                    Ok(ready) => {
                        started.send_replace(Some(Ok(ready)));
                        #[cfg(test)]
                        readiness_test_support::published(&port.control);
                    }
                    Err(error) => {
                        let code = safe_code(error);
                        port.control.fail(id);
                        failure = Some(code);
                        started.send_replace(Some(Err(code)));
                    }
                }
            }
            if failure.is_none() {
                let rescan = match port.control.state.lock() {
                    Ok(state) => state.rescan.clone(),
                    Err(_) => Arc::new(Notify::new()),
                };
                let mut tick = tokio::time::interval(RESCAN);
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let audio_sealed = loop {
                    if *sealed.borrow_and_update() {
                        break true;
                    }
                    let notice = tokio::select! {
                        changed = sealed.changed() => {
                            if changed.is_err() { break false; }
                            continue;
                        },
                        notice = receiver.recv() => notice,
                        _ = rescan.notified() => None,
                        _ = tick.tick() => None,
                    };
                    let result = if let Some(session) = notice {
                        port.one(session, &factory, false).await
                    } else {
                        port.scan(&factory, false).await
                    };
                    if let Err(error) = result {
                        let code = safe_code(error);
                        port.control.fail(id);
                        failure.get_or_insert(code);
                    }
                };
                // Receiver.close leaves pre-issued OwnedPermits valid. Only
                // recv-to-EOF proves that every terminal producer finished send.
                // Facade loss is only a notification/permit proof: without an
                // audio seal, retain old senders and wait for their natural EOF.
                if audio_sealed {
                    receiver.close();
                }
                while let Some(session) = receiver.recv().await {
                    if let Err(error) = port.one(session, &factory, false).await {
                        failure.get_or_insert(safe_code(error));
                    }
                }
                if let Err(error) = port.scan(&factory, false).await {
                    failure.get_or_insert(safe_code(error));
                }
            }
            factory.closed.store(true, Ordering::Release);
            if let Err(error) = port.control.child_barrier().await {
                failure.get_or_insert(safe_code(error));
            }
            if let Ok(mut state) = port.control.state.lock() {
                if state.id == id {
                    state.live.store(false, Ordering::Release);
                    state.stage = if failure.is_some() {
                        Stage::Failed
                    } else {
                        Stage::Joined
                    };
                }
            }
            drop(factory);
            drop(owner);
            done.send_replace(Some(failure.map_or(Ok(()), Err)));
        });
        *worker_slot = Some(task);
        Ok(())
    }
    pub(crate) fn signal_closing(&self, key: &AudioCloseKey) -> AppResult<()> {
        let mut state = self.port.control.state.lock().map_err(|_| storage())?;
        if state.id != key.delivery_id() || state.close_key.as_ref().is_some_and(|old| old != key) {
            return Err(error(Code::Interrupted));
        }
        state.live.store(false, Ordering::Release);
        state.stage = Stage::Closing;
        state.close_key = Some(key.clone());
        state.rescan.notify_one();
        Ok(())
    }
    pub(crate) async fn close_join_owned(
        &self,
        owner: OperationLease,
        key: AudioCloseKey,
        audio: AudioCloseJoin,
    ) -> AppResult<()> {
        self.close_join(owner, key, audio, false).await
    }
    pub(crate) async fn close_after_terminal_repair_owned(
        &self,
        owner: OperationLease,
        key: AudioCloseKey,
        audio: AudioCloseJoin,
    ) -> AppResult<()> {
        self.close_join(owner, key, audio, true).await
    }
    async fn close_join(
        &self,
        owner: OperationLease,
        key: AudioCloseKey,
        audio: AudioCloseJoin,
        repair_tail: bool,
    ) -> AppResult<()> {
        self.signal_closing(&key)?;
        let completion = {
            let mut state = self.port.control.state.lock().map_err(|_| storage())?;
            if repair_tail {
                // The prior closer is joined below before another bounded
                // worker is started, even if that earlier tail succeeded.
                state.close_done = None;
                None
            } else if let Some(completion) = &state.close_done {
                if completion.borrow().is_none_or(|result| result.is_ok()) {
                    Some(completion.clone())
                } else {
                    state.close_done = None;
                    None
                }
            } else {
                None
            }
        };
        let completion = if let Some(completion) = completion {
            completion
        } else {
            let mut prior = self.port.control.closer.lock().await;
            if let Some(task) = prior.as_mut() {
                let joined = task.await;
                prior.take();
                self.port.control.record_actual_join(joined)?;
            }
            let (done, completion) = watch::channel(None);
            self.port
                .control
                .state
                .lock()
                .map_err(|_| storage())?
                .close_done = Some(completion.clone());
            let delivery = self.clone();
            let task = tokio::spawn(async move {
                let result = async {
                    let ack = audio.wait().await?;
                    ack.require_key(&key)?;
                    let worker_done = delivery
                        .port
                        .control
                        .state
                        .lock()
                        .map_err(|_| storage())?
                        .worker_done
                        .clone();
                    let inactive = worker_done
                        .as_ref()
                        .is_none_or(|done| done.borrow().is_some());
                    let finished = worker_done
                        .as_ref()
                        .is_some_and(|done| done.borrow().as_ref().is_some_and(Result::is_ok));
                    if finished && !repair_tail {
                        delivery.port.control.join_worker().await
                    } else if inactive {
                        let _ = delivery.port.control.join_worker().await;
                        let (sender, receiver) = mpsc::channel(CHANNEL);
                        let (seal, sealed) = watch::channel(true);
                        let id = key.delivery_id();
                        {
                            let mut state =
                                delivery.port.control.state.lock().map_err(|_| storage())?;
                            state.stage = Stage::Closing;
                            state.sender = Some(sender);
                            state.seal = Some(seal);
                        }
                        delivery.spawn_worker(owner.clone(), receiver, sealed, id, None)?;
                        delivery.port.control.join_worker().await
                    } else {
                        if let Some(seal) = &delivery
                            .port
                            .control
                            .state
                            .lock()
                            .map_err(|_| storage())?
                            .seal
                        {
                            seal.send_replace(true);
                        }
                        delivery.port.control.join_worker().await
                    }
                }
                .await;
                drop(owner);
                done.send_replace(Some(result.map_err(safe_code)));
            });
            *prior = Some(task);
            completion
        };
        tokio::time::timeout(CLOSE_OBSERVATION, observed(completion))
            .await
            .map_err(|_| error(Code::Timeout))??;
        let mut closer = self.port.control.closer.lock().await;
        if let Some(task) = closer.as_mut() {
            let joined = task.await;
            closer.take();
            self.port.control.record_actual_join(joined)?;
        }
        Ok(())
    }
    // Final App disappearance has no observer to retry a 5s timeout. It retains
    // the same close task, worker and child until their actual joins, preserving
    // the original observation/result failure independently of those joins.
    pub(crate) async fn close_final_facade_owned(
        &self,
        owner: OperationLease,
        key: AudioCloseKey,
        audio: AudioCloseJoin,
    ) -> AppResult<FinalDeliveryJoin> {
        let result = self.close_join_owned(owner, key, audio).await;
        self.port.control.join_final_actual(result).await
    }
    // Poll real sources to completion for inert construction, including when
    // no runtime exists. Pending sources remain owned by the final App guard.
    pub(crate) fn try_drain_after_audio(
        &self,
        key: &AudioCloseKey,
        audio: &AudioCloseJoin,
    ) -> Option<AppResult<FinalDeliveryJoin>> {
        let ack = match audio.wait().now_or_never()? {
            Ok(ack) => ack,
            Err(_) => return None,
        };
        if ack.require_key(key).is_err() || self.signal_closing(key).is_err() {
            return None;
        }
        if let Some(seal) = &self.port.control.state.lock().ok()?.seal {
            seal.send_replace(true);
        }
        async {
            let result = self.port.control.worker_outcome().await;
            self.port.control.join_final_actual(result).await
        }
        .now_or_never()
    }
    pub(crate) async fn drain_failed_startup(&self) -> AppResult<()> {
        let id = self.delivery_id()?;
        self.port.control.fail(id);
        self.port.control.child_barrier().await?;
        self.port.control.join_worker_actual().await
    }
}
async fn observed(mut completion: watch::Receiver<Option<Result<(), Code>>>) -> AppResult<()> {
    loop {
        if let Some(result) = *completion.borrow_and_update() {
            return result.map_err(error);
        }
        completion
            .changed()
            .await
            .map_err(|_| error(Code::Interrupted))?;
    }
}
fn safe_code(error: AppError) -> Code {
    match error {
        AppError::Intelligence(code) => code,
        AppError::Configuration(_) => Code::Configuration,
        _ => Code::Storage,
    }
}

pub(super) struct TerminalJournal {
    pub(super) terminal: bool,
    pub(super) pending: bool,
    pub(super) generation: Option<Uuid>,
    pub(super) local_error: Option<String>,
    pub(super) deleting: bool,
}
impl LocalStore {
    pub(super) fn summary_terminal_in_tx(
        tx: &Transaction<'_>,
        session: Uuid,
    ) -> AppResult<Option<TerminalJournal>> {
        let row = tx.query_row("SELECT status,summary_terminal_pending,summary_terminal_generation,summary_terminal_error,summary_session_deleting FROM session_lifecycle WHERE session_id=?1", [session.to_string()], |row| {
            Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,i64>(4)?))
        }).optional()?;
        row.map(|(status, pending, generation, local_error, deleting)| {
            Ok(TerminalJournal {
                terminal: matches!(status.as_str(), "completed" | "failed"),
                pending: pending == 1,
                generation: generation.map(|id| uuid(&id)).transpose()?,
                local_error,
                deleting: deleting == 1,
            })
        })
        .transpose()
    }
    pub(super) fn consume_summary_terminal_in_tx(
        tx: &Transaction<'_>,
        session: Uuid,
    ) -> AppResult<()> {
        tx.execute("UPDATE session_lifecycle SET summary_terminal_pending=0,summary_terminal_generation=NULL,summary_terminal_error=NULL WHERE session_id=?1", [session.to_string()])?;
        Ok(())
    }
    fn summary_terminal_page(
        &self,
        status: Option<&str>,
        after: Option<&str>,
    ) -> AppResult<Vec<Uuid>> {
        self.transaction(|tx| {
            let sql = match (status, after) {
                (Some(_),Some(_)) => "SELECT session_id FROM session_lifecycle INDEXED BY summary_terminal_eligible WHERE status=?1 AND session_id>?2 ORDER BY session_id LIMIT 33",
                (Some(_),None) => "SELECT session_id FROM session_lifecycle INDEXED BY summary_terminal_eligible WHERE status=?1 ORDER BY session_id LIMIT 33",
                (None,Some(_)) => "SELECT session_id FROM session_lifecycle INDEXED BY summary_terminal_pending WHERE summary_terminal_pending=1 AND session_id>?1 ORDER BY session_id LIMIT 33",
                (None,None) => "SELECT session_id FROM session_lifecycle INDEXED BY summary_terminal_pending WHERE summary_terminal_pending=1 ORDER BY session_id LIMIT 33",
            };
            let mut query = tx.prepare(sql)?;
            let mut rows = match (status, after) {
                (Some(status),Some(after)) => query.query(params![status,after])?,
                (Some(status),None) => query.query([status])?,
                (None,Some(after)) => query.query([after])?,
                (None,None) => query.query([])?,
            };
            let mut page = Vec::with_capacity(PAGE);
            while let Some(row) = rows.next()? {
                if page.len() == PAGE { break; }
                page.push(uuid(super::store::text(row,0,36)?)?);
            }
            Ok(page)
        }).map_err(safe_store_error)
    }
}
