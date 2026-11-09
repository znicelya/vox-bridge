//! Durable bounded execution. Workers own ports and operation leases, never the scheduler.
use super::{
    config::ServiceConfigStore,
    http::cancelled,
    store::{self, JobDraft},
    types::*,
};
use crate::{
    error::{AppError, AppResult},
    storage::LocalStore,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore, broadcast, mpsc, watch};
use uuid::Uuid;
mod child_operations;
pub(crate) mod reap;
use child_operations::ChildOperations;
pub(crate) use reap::{JobReapEvidence, JobReapPort};
pub struct JobContext {
    pub id: Uuid,
    pub cancel: watch::Receiver<bool>,
    fence: JobAdmissionFence,
    children: ChildOperations,
}
#[derive(Clone)]
pub(crate) struct JobPublicationFence {
    id: Uuid,
    cancel: watch::Receiver<bool>,
    fence: JobAdmissionFence,
}
impl JobPublicationFence {
    pub(crate) fn commit<T>(
        &self,
        tx: &rusqlite::Transaction<'_>,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        self.fence.run(|| {
            if *self.cancel.borrow()
                || store::job_in_transaction(tx, self.id)?
                    .is_none_or(|job| job.status != JobStatus::Running)
            {
                return Err(error(IntelligenceErrorCode::Cancelled));
            }
            operation()
        })
    }
}
impl JobContext {
    pub(crate) fn publication_fence(&self) -> JobPublicationFence {
        JobPublicationFence {
            id: self.id,
            cancel: self.cancel.clone(),
            fence: self.fence.clone(),
        }
    }
    pub(crate) fn operation_owner(&self) -> AppResult<OperationLease> {
        // Only control is held here. The returned token crosses config/index
        // boundaries after this guard is released; never consult SQLite here.
        let control = self.fence.control.lock().unwrap();
        if control.stopping || self.fence.closed.load(Ordering::Acquire) {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        if *self.cancel.borrow() || !control.active.contains_key(&self.id) {
            return Err(error(IntelligenceErrorCode::Cancelled));
        }
        self.children.try_acquire()
    }

    /// Caller already owns SQLite; generation control is acquired only inside it.
    pub(crate) fn commit<T>(
        &self,
        tx: &rusqlite::Transaction<'_>,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        self.fence.run(|| {
            if *self.cancel.borrow()
                || store::job_in_transaction(tx, self.id)?
                    .is_none_or(|job| job.status != JobStatus::Running)
            {
                return Err(error(IntelligenceErrorCode::Cancelled));
            }
            operation()
        })
    }
}
#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    async fn run(
        &self,
        context: JobContext,
        payload: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode>;
    /// Domain metadata only. No filesystem/config access while SQLite is held.
    fn terminal_tx(
        &self,
        _tx: &rusqlite::Transaction<'_>,
        _id: Uuid,
        _status: JobStatus,
        _code: Option<IntelligenceErrorCode>,
        _awaiting_reap: bool,
    ) -> AppResult<()> {
        Ok(())
    }
    async fn cleanup(&self, _reaped: Option<Uuid>) -> AppResult<()> {
        Ok(())
    }
    async fn cleanup_after_stop(&self) -> AppResult<()> {
        self.cleanup(None).await
    }
    fn reconcile_startup(&self) -> AppResult<()> {
        Ok(())
    }
}
pub(crate) struct AfterTerminalNotice {
    pub(crate) parent_id: Uuid,
    pub(crate) document_id: Uuid,
    pub(crate) version: u32,
    pub(crate) sha256: String,
    pub(crate) parser: String,
}
#[async_trait::async_trait]
pub(crate) trait AfterTerminal: Send + Sync {
    fn notice(
        &self,
        tx: &rusqlite::Transaction<'_>,
        job: &JobView,
    ) -> AppResult<Option<AfterTerminalNotice>>;
    fn deferred(
        &self,
        tx: &rusqlite::Transaction<'_>,
        notice: &AfterTerminalNotice,
    ) -> AppResult<()>;
    async fn succeeded(
        &self,
        notice: AfterTerminalNotice,
        owner: OperationLease,
        fence: JobAdmissionFence,
    );
}
#[derive(Clone)]
pub(crate) struct JobEvents(broadcast::Sender<IntelligenceEvent>);
impl JobEvents {
    pub(crate) fn publish(&self, job: JobView) {
        let _ = self.0.send(IntelligenceEvent::JobChanged { job });
    }
}
/// Generation fence for Task 6's config→SQLite atomic job/result transaction.
/// Call `run` only inside the SQLite transaction, never acquire it before SQLite.
#[derive(Clone)]
pub(crate) struct JobAdmissionFence {
    control: Arc<Mutex<Control>>,
    closed: Arc<AtomicBool>,
    cancel: watch::Receiver<bool>,
}
impl JobAdmissionFence {
    pub(crate) fn cancellation(&self) -> watch::Receiver<bool> {
        self.cancel.clone()
    }
    pub(crate) fn run<T>(&self, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        let control = self.control.lock().unwrap();
        if control.stopping || self.closed.load(Ordering::Acquire) {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        operation()
    }
}
type Handlers = HashMap<JobKind, Arc<dyn JobHandler>>;
pub(crate) type OperationLease = Arc<dyn Send + Sync>;
#[derive(Default)]
struct Control {
    stopping: bool,
    active: HashMap<Uuid, watch::Sender<bool>>,
}
struct Generation {
    id: Uuid,
    cancel: watch::Sender<bool>,
    completion: watch::Receiver<bool>,
    control: Arc<Mutex<Control>>,
}
#[derive(Default)]
struct State {
    frozen: bool,
    handlers: Handlers,
    generation: Option<Generation>,
    after_terminal: Option<Arc<dyn AfterTerminal>>,
}
pub struct JobScheduler {
    store: Arc<LocalStore>,
    state: Mutex<State>,
    wake: Arc<Notify>,
    events: broadcast::Sender<IntelligenceEvent>,
    parser: Arc<Semaphore>,
    shutdown_timeout: Duration,
    closed: Arc<AtomicBool>,
    signal: watch::Sender<bool>,
    reap: Arc<reap::Registry>,
}
impl JobScheduler {
    pub fn new(store: Arc<LocalStore>) -> Self {
        Self {
            store,
            state: Mutex::default(),
            wake: Arc::default(),
            events: broadcast::channel(64).0,
            parser: Arc::new(Semaphore::new(1)),
            shutdown_timeout: Duration::from_secs(5),
            closed: Arc::new(AtomicBool::new(true)),
            signal: watch::channel(false).0,
            reap: reap::Registry::new(),
        }
    }
    pub fn register(&self, kind: JobKind, handler: Arc<dyn JobHandler>) -> AppResult<()> {
        let mut s = self.state.lock().unwrap();
        if s.frozen || s.handlers.contains_key(&kind) {
            return Err(error(IntelligenceErrorCode::Configuration));
        }
        s.handlers.insert(kind, handler);
        Ok(())
    }
    pub(crate) fn register_after_terminal(&self, hook: Arc<dyn AfterTerminal>) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        if state.frozen
            || state.after_terminal.is_some()
            || !state.handlers.contains_key(&JobKind::Import)
            || !state.handlers.contains_key(&JobKind::Reindex)
        {
            return Err(error(IntelligenceErrorCode::Configuration));
        }
        state.after_terminal = Some(hook);
        Ok(())
    }
    pub(crate) fn wake_port(&self) -> Arc<Notify> {
        self.wake.clone()
    }
    pub(crate) fn reap_port(&self) -> JobReapPort {
        JobReapPort::new(self.reap.clone())
    }
    pub fn start(&self) -> AppResult<()> {
        self.start_owned(Arc::new(()))
    }
    pub(crate) fn start_owned(&self, lease: OperationLease) -> AppResult<()> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| error(IntelligenceErrorCode::Configuration))?;
        let mut s = self.state.lock().unwrap();
        if let Some(g) = &s.generation {
            return if self.closed.load(Ordering::Acquire) || g.control.lock().unwrap().stopping {
                Err(error(IntelligenceErrorCode::Busy))
            } else {
                Ok(())
            };
        }
        s.frozen = true;
        let handlers = Arc::new(s.handlers.clone());
        let after_terminal = s.after_terminal.clone();
        let control = Arc::new(Mutex::new(Control::default()));
        let generation_id = Uuid::new_v4();
        self.reap.begin(generation_id)?;
        let reap = self.reap.clone();
        self.closed.store(false, Ordering::Release);
        self.signal.send_replace(false);
        let cancel = self.signal.clone();
        let stop = cancel.subscribe();
        let (done, completion) = watch::channel(false);
        let wake = self.wake.clone();
        let store = self.store.clone();
        let events = self.events.clone();
        let worker_control = control.clone();
        let closed = self.closed.clone();
        let children = ChildOperations::new(&lease);
        runtime.spawn(async move {
            let _lease = lease;
            let (tx, rx) = mpsc::channel(2);
            let rx = Arc::new(tokio::sync::Mutex::new(rx));
            let mut workers = tokio::task::JoinSet::new();
            for _ in 0..2 {
                workers.spawn(worker(
                    store.clone(),
                    handlers.clone(),
                    worker_control.clone(),
                    rx.clone(),
                    wake.clone(),
                    events.clone(),
                    stop.clone(),
                    closed.clone(),
                    children.clone(),
                    after_terminal.clone(),
                    reap.clone(),
                ));
            }
            let mut stop = stop;
            loop {
                tokio::select! {
                                                  biased;
                                                  _ = cancelled(&mut stop) => break,
                                                  _ = wake.notified() => {
                                                      for _ in 0..2 {
                let _ = tx.try_send(());
                               }
                                                  }
                                              }
            }
            {
                let mut control = worker_control.lock().unwrap();
                control.stopping = true;
                for cancel in control.active.values() {
                    cancel.send_replace(true);
                }
            }
            drop(tx);
            while workers.join_next().await.is_some() {}
            for handler in handlers.values() {
                while handler.cleanup_after_stop().await.is_err() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
            children.drain().await;
            if reap.quiesce(generation_id).await.is_err() {
                return;
            }
            drop(_lease);
            if reap.joined(generation_id).is_err() {
                return;
            }
            done.send_replace(true);
        });
        s.generation = Some(Generation {
            id: generation_id,
            cancel,
            completion,
            control,
        });
        self.wake();
        Ok(())
    }
    pub(crate) fn reconcile(&self) -> AppResult<()> {
        let handlers = self.state.lock().unwrap().handlers.clone();
        interrupt(&self.store, &handlers)?;
        for handler in handlers.values() {
            handler.reconcile_startup()?;
        }
        Ok(())
    }
    pub async fn enqueue(
        &self,
        kind: JobKind,
        session_id: Option<Uuid>,
        object_id: Option<Uuid>,
        admission: JobAdmission,
        payload: serde_json::Value,
    ) -> AppResult<JobView> {
        // Non-profile jobs use the same admission helper. Profile consumers use the
        // atomic config boundary below, never admit a caller-supplied stale snapshot.
        let decoded = store::decode_payload(
            &serde_json::to_string(&payload)
                .map_err(|_| error(IntelligenceErrorCode::Configuration))?,
        )?;
        if !decoded.profiles.is_empty() {
            return Err(error(IntelligenceErrorCode::Configuration));
        }
        let s = self.state.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        check_admission(&s, kind)?;
        let result = self.store.transaction(|tx| {
            store::admit_job(
                tx,
                JobDraft {
                    id: Uuid::new_v4(),
                    kind,
                    session_id,
                    object_id,
                    admission,
                    payload,
                    initial_failure: None,
                },
            )
        })?;
        let view = result.view().clone();
        drop(s);
        self.publish(view.clone());
        self.wake();
        Ok(view)
    }
    /// Task 4/6/7 producers capture IDs and validate internal input, then use this
    /// config→SQLite boundary. Task 6 calls with_profile_snapshot + admit_job itself
    /// to insert the result version in the exact same transaction.
    pub(crate) fn admission_fence(&self, kind: JobKind) -> AppResult<JobAdmissionFence> {
        let state = self.state.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        check_admission(&state, kind)?;
        Ok(JobAdmissionFence {
            control: state
                .generation
                .as_ref()
                .ok_or_else(|| error(IntelligenceErrorCode::Busy))?
                .control
                .clone(),
            closed: self.closed.clone(),
            cancel: self.signal.subscribe(),
        })
    }
    pub async fn enqueue_with_profiles(
        &self,
        config: &ServiceConfigStore,
        kind: JobKind,
        session_id: Option<Uuid>,
        object_id: Option<Uuid>,
        admission: JobAdmission,
        ids: Vec<Uuid>,
        input: serde_json::Value,
    ) -> AppResult<JobView> {
        let fence = self.admission_fence(kind)?;
        // Shutdown can begin while config entry waits. A shared generation control
        // guard fences commit against shutdown without capturing Arc<JobScheduler>.
        let result = config
            .with_profile_snapshot(ids, move |tx, profiles| {
                fence.run(|| {
                    store::admit_job(
                        tx,
                        JobDraft {
                            id: Uuid::new_v4(),
                            kind,
                            session_id,
                            object_id,
                            admission,
                            payload: serde_json::json!({
                             "version": 1,
                             "profiles": profiles,
                             "input": input }
                            ),
                            initial_failure: None,
                        },
                    )
                })
            })
            .await?;
        let view = result.view().clone();
        self.publish(view.clone());
        self.wake();
        Ok(view)
    }
    pub async fn cancel(&self, id: Uuid) -> AppResult<JobView> {
        let s = self.state.lock().unwrap();
        self.store
            .transaction(|tx| {
                use rusqlite::OptionalExtension;
                let status: Option<String> = tx
                    .query_row(
                        "SELECT status FROM intelligence_jobs WHERE id=?1",
                        [id.to_string()],
                        |r| r.get(0),
                    )
                    .optional()?;
                let status = status.ok_or_else(|| error(IntelligenceErrorCode::Configuration))?;
                let control = s.generation.as_ref().map(|g| g.control.lock().unwrap());
                if status == "queued" {
                    transition_tx(
                        tx,
                        id,
                        "queued",
                        JobStatus::Cancelled,
                        Some(IntelligenceErrorCode::Cancelled),
                        &s.handlers,
                        false,
                    )?;
                }
                if status == "running" {
                    if let Some(cancel) = control.as_ref().and_then(|c| c.active.get(&id)) {
                        cancel.send_replace(true);
                    }
                }
                Ok(())
            })
            .map_err(store::safe_store_error)?;
        let job = self
            .store
            .get_job(id)?
            .ok_or_else(|| error(IntelligenceErrorCode::Storage))?;
        self.publish(job.clone());
        Ok(job)
    }
    /// Immediate cancellation/admission fence: no SQLite or generation mutex wait.
    pub(crate) fn signal_shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.signal.send_replace(true);
    }
    pub(crate) fn request_shutdown(&self) -> AppResult<()> {
        self.signal_shutdown();
        let s = self.state.lock().unwrap();
        if let Some(g) = &s.generation {
            {
                let mut c = g.control.lock().unwrap();
                c.stopping = true;
                g.cancel.send_replace(true);
                for cancel in c.active.values() {
                    cancel.send_replace(true);
                }
            }
            // Never wait for SQLite while holding generation control: snapshot callbacks
            // already own SQLite before they acquire that control fence.
            let ids = self
                .store
                .transaction(|tx| {
                    let ids = tx
                        .prepare("SELECT id FROM intelligence_jobs WHERE status='queued' LIMIT 32")?
                        .query_map([], |row| row.get::<_, String>(0))?
                        .collect::<Result<Vec<_>, _>>()?;
                    for id in &ids {
                        transition_tx(
                            tx,
                            Uuid::parse_str(id)
                                .map_err(|_| error(IntelligenceErrorCode::Storage))?,
                            "queued",
                            JobStatus::Cancelled,
                            Some(IntelligenceErrorCode::Cancelled),
                            &s.handlers,
                            false,
                        )?;
                    }
                    Ok(ids)
                })
                .map_err(store::safe_store_error)?;
            for id in ids {
                if let Ok(id) = Uuid::parse_str(&id) {
                    if let Some(job) = self.store.get_job(id)? {
                        self.publish(job);
                    }
                }
            }
        }
        Ok(())
    }
    pub async fn shutdown(&self) -> AppResult<()> {
        self.request_shutdown()?;
        let completion = self
            .state
            .lock()
            .unwrap()
            .generation
            .as_ref()
            .map(|g| (g.id, g.completion.clone()));
        if let Some((id, mut completion)) = completion {
            let result = tokio::time::timeout(self.shutdown_timeout, async {
                loop {
                    if *completion.borrow_and_update() {
                        return Ok::<(), AppError>(());
                    }
                    completion
                        .changed()
                        .await
                        .map_err(|_| error(IntelligenceErrorCode::Interrupted))?;
                }
            })
            .await;
            let mut state = self.state.lock().unwrap();
            // A delayed second observer cannot retire or interrupt a newer generation.
            if state.generation.as_ref().is_none_or(|g| g.id != id) {
                return Ok(());
            }
            match result {
                Ok(Ok(())) => state.generation = None,
                _ => {
                    for job in interrupt(&self.store, &state.handlers)? {
                        self.publish(job);
                    }
                    return Err(error(IntelligenceErrorCode::Interrupted));
                }
            }
        }
        Ok(())
    }
    pub fn parser_gate(&self) -> Arc<Semaphore> {
        self.parser.clone()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<IntelligenceEvent> {
        self.events.subscribe()
    }
    pub(crate) fn event_port(&self) -> JobEvents {
        JobEvents(self.events.clone())
    }
    pub(crate) async fn cleanup_cancelled(&self) -> AppResult<()> {
        let handlers = self.state.lock().unwrap().handlers.clone();
        for handler in handlers.values() {
            handler.cleanup(None).await?;
        }
        Ok(())
    }
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }
    fn publish(&self, job: JobView) {
        let _ = self.events.send(IntelligenceEvent::JobChanged { job });
    }
}
impl Drop for JobScheduler {
    fn drop(&mut self) {
        let _ = self.request_shutdown();
    }
}
fn check_admission(s: &State, kind: JobKind) -> AppResult<()> {
    if !s.handlers.contains_key(&kind) {
        return Err(error(IntelligenceErrorCode::Configuration));
    }
    if s.generation
        .as_ref()
        .is_none_or(|g| g.control.lock().unwrap().stopping)
    {
        return Err(error(IntelligenceErrorCode::Busy));
    }
    Ok(())
}
async fn worker(
    store: Arc<LocalStore>,
    handlers: Arc<Handlers>,
    control: Arc<Mutex<Control>>,
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<()>>>,
    wake: Arc<Notify>,
    events: broadcast::Sender<IntelligenceEvent>,
    mut stop: watch::Receiver<bool>,
    closed: Arc<AtomicBool>,
    children: ChildOperations,
    after_terminal: Option<Arc<dyn AfterTerminal>>,
    reap: Arc<reap::Registry>,
) {
    use futures_util::FutureExt;
    loop {
        tokio::select! {
        biased;
                _=cancelled(&mut stop)=>return,
        token=async{
        rx.lock().await.recv().await}
        =>if token.is_none(){
        return;
                }
        }
        let claimed = match claim(&store, &control, &closed, &reap) {
            Ok(v) => v,
            Err(_) => {
                control.lock().unwrap().stopping = true;
                return;
            }
        };
        let Some((job, payload, cancel, frame)) = claimed else {
            continue;
        };
        let job_children = children.for_job(frame.clone());
        let _ = events.send(IntelligenceEvent::JobChanged { job: job.clone() });
        let result = match handlers.get(&job.kind) {
            Some(handler) => std::panic::AssertUnwindSafe(handler.run(
                JobContext {
                    id: job.id,
                    cancel: cancel.clone(),
                    fence: JobAdmissionFence {
                        control: control.clone(),
                        closed: closed.clone(),
                        cancel: stop.clone(),
                    },
                    children: job_children.clone(),
                },
                payload,
            ))
            .catch_unwind()
            .await
            .unwrap_or(Err(IntelligenceErrorCode::Interrupted)),
            None => Err(IntelligenceErrorCode::Configuration),
        };
        let notice = if result.is_ok() {
            after_terminal
                .as_ref()
                .and_then(|hook| store.transaction(|tx| hook.notice(tx, &job)).ok().flatten())
        } else {
            None
        };
        // Reservation happens while this context is still active and Running.
        // ConfigIo never acquires an owner from a terminal context.
        let continuation = notice.as_ref().and_then(|_| {
            JobContext {
                id: job.id,
                cancel: cancel.clone(),
                fence: JobAdmissionFence {
                    control: control.clone(),
                    closed: closed.clone(),
                    cancel: stop.clone(),
                },
                children: job_children,
            }
            .operation_owner()
            .ok()
        });
        frame.close_handler();
        let finished = store.transaction(|tx| {
            let mut c = control.lock().unwrap();
            let was_running = store::job_in_transaction(tx, job.id)?
                .is_some_and(|job| job.status == JobStatus::Running);
            let is_cancelled = *cancel.borrow();
            let (status, code) = if is_cancelled {
                (JobStatus::Cancelled, Some(IntelligenceErrorCode::Cancelled))
            } else {
                match result {
                    Ok(()) => (JobStatus::Succeeded, None),
                    Err(IntelligenceErrorCode::Cancelled) => {
                        (JobStatus::Cancelled, Some(IntelligenceErrorCode::Cancelled))
                    }
                    Err(IntelligenceErrorCode::Interrupted) => (
                        JobStatus::Interrupted,
                        Some(IntelligenceErrorCode::Interrupted),
                    ),
                    Err(e) => (JobStatus::Failed, Some(e)),
                }
            };
            transition_tx(tx, job.id, "running", status, code, &handlers, false)?;
            c.active.remove(&job.id);
            let succeeded = was_running && status == JobStatus::Succeeded;
            if succeeded && continuation.is_none() {
                if let (Some(hook), Some(notice)) = (&after_terminal, &notice) {
                    hook.deferred(tx, notice)?;
                }
            }
            Ok(succeeded)
        });
        if finished.is_ok() {
            if let Ok(Some(job)) = store.get_job(job.id) {
                let _ = events.send(IntelligenceEvent::JobChanged { job });
            }
        }
        if matches!(finished, Ok(true)) {
            if let (Some(hook), Some(notice), Some(owner)) = (&after_terminal, notice, continuation)
            {
                hook.succeeded(
                    notice,
                    owner,
                    JobAdmissionFence {
                        control: control.clone(),
                        closed: closed.clone(),
                        cancel: stop.clone(),
                    },
                )
                .await;
            }
        }
        if let Some(handler) = handlers.get(&job.kind) {
            let _ = handler.cleanup(Some(job.id)).await;
        }
        if reap.prune().is_err() {
            return;
        }
        wake.notify_one();
    }
}
fn claim(
    store: &LocalStore,
    control: &Mutex<Control>,
    closed: &AtomicBool,
    reap: &Arc<reap::Registry>,
) -> AppResult<
    Option<(
        JobView,
        serde_json::Value,
        watch::Receiver<bool>,
        Arc<reap::Frame>,
    )>,
> {
    use rusqlite::OptionalExtension;
    let mut attempted = None;
    let claimed = store
        .transaction(|tx| {
            let mut control = control.lock().unwrap();
            if control.stopping || closed.load(Ordering::Acquire) {
                return Ok(None);
            }
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT id,payload_json FROM \
                    intelligence_jobs WHERE status='queued' \
                    ORDER BY rowid LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((id, payload)) = row else {
                return Ok(None);
            };
            let id = Uuid::parse_str(&id).map_err(|_| error(IntelligenceErrorCode::Storage))?;
            let decoded = store::decode_payload(&payload)?;
            let (rowid, hash) = reap::identity(tx, id)?;
            let pending = reap.register(id, rowid, hash)?;
            attempted = Some(id);
            let payload =
                serde_json::to_value(decoded).map_err(|_| error(IntelligenceErrorCode::Storage))?;
            tx.execute(
                "UPDATE intelligence_jobs SET \
                    status='running',updated_at_ms=?1 WHERE \
                    id=?2 AND status='queued'",
                rusqlite::params![now_ms(), id.to_string()],
            )?;
            let (cancel, receiver) = watch::channel(false);
            let job = store::job_in_transaction(tx, id)?
                .ok_or_else(|| error(IntelligenceErrorCode::Storage))?;
            control.active.insert(id, cancel);
            Ok(Some((job, payload, receiver, pending)))
        })
        .map_err(store::safe_store_error);
    if claimed.is_err() {
        if let Some(id) = attempted {
            control.lock().unwrap().active.remove(&id);
        }
    }
    let claimed = claimed?;
    Ok(claimed.map(|(job, payload, cancel, pending)| (job, payload, cancel, pending.commit())))
}
fn transition_tx(
    tx: &rusqlite::Transaction<'_>,
    id: Uuid,
    from: &str,
    status: JobStatus,
    code: Option<IntelligenceErrorCode>,
    handlers: &Handlers,
    awaiting_reap: bool,
) -> AppResult<()> {
    let changed = tx.execute(
        "UPDATE intelligence_jobs SET status=?1,\
                    error_code=?2,updated_at_ms=?3 WHERE id=?4 \
                    AND status=?5",
        rusqlite::params![
            serde_json::to_value(status).unwrap().as_str(),
            code.map(IntelligenceErrorCode::as_str),
            now_ms(),
            id.to_string(),
            from
        ],
    )?;
    if changed != 0 {
        let job = store::job_in_transaction(tx, id)?
            .ok_or_else(|| error(IntelligenceErrorCode::Storage))?;
        if let Some(handler) = handlers.get(&job.kind) {
            handler.terminal_tx(tx, id, status, code, awaiting_reap)?;
        }
    }
    Ok(())
}
fn interrupt(store: &LocalStore, handlers: &Handlers) -> AppResult<Vec<JobView>> {
    store
        .transaction(|tx| {
            let pending = tx
                .prepare(
                    "SELECT id,status FROM intelligence_jobs \
                    WHERE status IN ('queued','running') LIMIT 33",
                )?
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            if pending.len() > store::MAX_UNFINISHED {
                return Err(error(IntelligenceErrorCode::ResourceLimit));
            }
            let mut changed = Vec::with_capacity(pending.len());
            for (id, status) in pending {
                let id = Uuid::parse_str(&id).map_err(|_| error(IntelligenceErrorCode::Storage))?;
                transition_tx(
                    tx,
                    id,
                    &status,
                    JobStatus::Interrupted,
                    Some(IntelligenceErrorCode::Interrupted),
                    handlers,
                    status == "running",
                )?;
                changed.push(
                    store::job_in_transaction(tx, id)?
                        .ok_or_else(|| error(IntelligenceErrorCode::Storage))?,
                );
            }
            Ok(changed)
        })
        .map_err(store::safe_store_error)
}
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
fn error(code: IntelligenceErrorCode) -> AppError {
    AppError::Intelligence(code)
}
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_support;
