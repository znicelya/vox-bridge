//! One execution thread, one retained join responsibility, bounded observers.
use super::{
    codec::ChunkMetadata,
    owner::{PersistedReceipt, Runtime},
    repair,
    repository::{DurableIntent, IndexRepository, PublicationAck},
    seal,
};
use crate::{
    error::{AppError, AppResult},
    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode as Code},
    storage::LocalStore,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::watch;
use uuid::Uuid;

type Outcome = Result<(), Code>;
type Completion = Option<Outcome>;
type PublicationKey = (String, Uuid);
#[derive(Clone, Copy)]
struct PendingPublication {
    operation: Uuid,
    epoch: Uuid,
}
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct IndexOwner {
    inner: Arc<Inner>,
}
#[derive(Clone)]
pub(crate) struct WorkTicket {
    intent: DurableIntent,
    lease: OperationLease,
    cancelled: Arc<AtomicBool>,
}
pub(crate) struct Pending<T> {
    result: tokio::sync::oneshot::Receiver<AppResult<Reply>>,
    project: fn(Reply) -> AppResult<T>,
}
pub(crate) struct MaintenanceTicket {
    owner: IndexOwner,
    work: WorkTicket,
    token: Uuid,
    active: AtomicBool,
    release_requested: Arc<AtomicBool>,
}
pub(crate) struct RecoveryTicket {
    owner: IndexOwner,
    work: WorkTicket,
    token: Uuid,
    active: AtomicBool,
    release_requested: Arc<AtomicBool>,
}
impl RecoveryTicket {
    pub(crate) fn token(&self) -> Uuid {
        self.token
    }
}
impl Drop for RecoveryTicket {
    fn drop(&mut self) {
        if self.active.load(Ordering::Acquire) {
            self.release_requested.store(true, Ordering::Release);
            let _ =
                self.owner
                    .enqueue_admitted(&self.work, Command::EndRecovery(self.token), |r| match r {
                        Reply::MaintenanceEnded => Ok(()),
                        _ => Err(error(Code::Interrupted)),
                    });
        }
    }
}
impl WorkTicket {
    pub(crate) fn operation_id(&self) -> Uuid {
        self.intent.body().operation_id
    }
    pub(crate) fn generation(&self) -> &seal::Generation {
        &self.intent.body().descriptor
    }
    pub(crate) fn cleanup(&self) -> Self {
        Self {
            intent: self.intent.clone(),
            lease: self.lease.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl<T> Pending<T> {
    pub(crate) async fn observe(self) -> AppResult<T> {
        let reply = tokio::time::timeout(STOP_TIMEOUT, self.result)
            .await
            .map_err(|_| error(Code::Timeout))?
            .map_err(|_| error(Code::Interrupted))??;
        (self.project)(reply)
    }
}
impl MaintenanceTicket {
    fn enqueue_end(&self) -> AppResult<Pending<()>> {
        self.owner
            .enqueue_admitted(
                &self.work,
                Command::EndMaintenance(self.token),
                |reply| match reply {
                    Reply::MaintenanceEnded => Ok(()),
                    _ => Err(error(Code::Interrupted)),
                },
            )
    }
}
impl Drop for MaintenanceTicket {
    fn drop(&mut self) {
        if self.active.load(Ordering::Acquire) {
            self.release_requested.store(true, Ordering::Release);
            // The accepted envelope retains the operation token even when no
            // caller remains to observe its release response.
            let _ = self.enqueue_end();
        }
    }
}

enum Command {
    BeginRecovery(super::recovery::RecoveryEvidence, Arc<AtomicBool>),
    CopyRecovery {
        token: Uuid,
        start: u64,
        limit: u32,
    },
    VerifyRecovery(Uuid),
    EndRecovery(Uuid),
    RetireRecovery(super::recovery::RecoveryFence),
    RemoveGeneration(seal::Generation),
    Open,
    PublishPlan(PersistedReceipt, super::publication::PublicationPlan),
    Retire,
    Candidates {
        vector: Vec<f32>,
        limit: u32,
    },
    #[cfg(test)]
    CorruptMetadata {
        id: u64,
        vector: Vec<f32>,
        metadata: String,
    },
    Add(Vec<(Vec<f32>, ChunkMetadata)>),
    ScanRecords {
        start: u64,
        limit: u32,
    },
    Scan {
        start: u64,
        limit: u32,
    },
    Delete(Vec<u64>),
    Replace {
        id: u64,
        vector: Vec<f32>,
        metadata: ChunkMetadata,
    },
    Search {
        vector: Vec<f32>,
        limit: u32,
    },
    BeginMaintenance(Arc<AtomicBool>),
    CopyVectors {
        token: Uuid,
        start: u64,
        limit: u32,
    },
    EndMaintenance(Uuid),
    Persist,
    Publish(PersistedReceipt),
    Acknowledge(PublicationAck),
}
enum Reply {
    Candidates(Vec<super::owner::Candidate>),
    Opened(u64),
    Added(Vec<u64>),
    Scanned(Vec<(u64, Option<ChunkMetadata>)>),
    Records(Vec<super::owner::RecordCandidate>),
    Deleted(Vec<bool>),
    Replaced(u64),
    Searched(Vec<(u64, f32, ChunkMetadata)>),
    MaintenanceStarted(Uuid),
    VectorsCopied(Vec<(u64, Vec<f32>)>),
    MaintenanceEnded,
    Persisted(PersistedReceipt),
    Published(PublicationAck),
    Acknowledged,
}
struct Envelope {
    intent: DurableIntent,
    _lease: OperationLease,
    cancelled: Arc<AtomicBool>,
    command: Command,
    reply: tokio::sync::oneshot::Sender<AppResult<Reply>>,
}
#[cfg(test)]
pub(super) struct Gate {
    pub entered: std::sync::mpsc::SyncSender<()>,
    pub release: Receiver<()>,
}
#[cfg(test)]
#[derive(Default)]
pub(super) struct Hooks {
    pub command: Mutex<Option<Gate>>,
    pub ready: Mutex<Option<Gate>>,
    pub stop_return: Mutex<Option<Gate>>,
    pub fail_join_spawn: AtomicBool,
    pub fail_ready: AtomicBool,
}
struct Inner {
    root: PathBuf,
    repository: Arc<IndexRepository>,
    generation: Mutex<Option<Generation>>,
    publications: Mutex<HashMap<PublicationKey, PendingPublication>>,
    #[cfg(test)]
    hooks: Arc<Hooks>,
}
struct Generation {
    epoch: Uuid,
    running: bool,
    stopping: bool,
    sender: Option<SyncSender<Envelope>>,
    ready: watch::Receiver<Completion>,
    done: watch::Receiver<Completion>,
    stop: Arc<StopControl>,
    join: Option<JoinPayload>,
}
impl IndexOwner {
    pub(crate) fn new(root: PathBuf, store: Arc<LocalStore>) -> Self {
        Self {
            inner: Arc::new(Inner {
                root,
                repository: Arc::new(IndexRepository::new(store)),
                generation: Mutex::new(None),
                publications: Mutex::new(HashMap::new()),
                #[cfg(test)]
                hooks: Arc::new(Hooks::default()),
            }),
        }
    }
    #[cfg(test)]
    pub(super) fn hooks(&self) -> Arc<Hooks> {
        self.inner.hooks.clone()
    }

    fn active_epoch(&self) -> AppResult<Uuid> {
        let state = self
            .inner
            .generation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .as_ref()
            .filter(|generation| generation.running && !generation.stopping)
            .map(|generation| generation.epoch)
            .ok_or_else(|| error(Code::Interrupted))
    }

    #[cfg(test)]
    pub(crate) fn running_epoch_for_test(&self) -> AppResult<Uuid> {
        self.active_epoch()
    }

    #[cfg(test)]
    pub(crate) fn block_startup_for_test(&self) -> (Receiver<()>, SyncSender<()>) {
        let (entered, wait) = mpsc::sync_channel(1);
        let (release, receive) = mpsc::sync_channel(1);
        *self
            .inner
            .hooks
            .ready
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Gate {
            entered,
            release: receive,
        });
        (wait, release)
    }

    #[cfg(test)]
    pub(crate) fn block_stop_return_for_test(&self) -> (Receiver<()>, SyncSender<()>) {
        let (entered, wait) = mpsc::sync_channel(1);
        let (release, receive) = mpsc::sync_channel(1);
        *self
            .inner
            .hooks
            .stop_return
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Gate {
            entered,
            release: receive,
        });
        (wait, release)
    }

    #[cfg(test)]
    pub(crate) fn set_transition_for_test(&self, operation: Uuid, epoch: Uuid) {
        self.inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                (format!("test-transition-{operation}"), Uuid::new_v4()),
                PendingPublication { operation, epoch },
            );
    }

    #[cfg(test)]
    pub(crate) fn transition_epochs_for_test(&self) -> Vec<Uuid> {
        self.inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .map(|pending| pending.epoch)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn fail_startup_for_test(&self) {
        self.inner.hooks.fail_ready.store(true, Ordering::Release);
    }

    pub(crate) fn create(
        &self,
        generation: seal::Generation,
        lease: OperationLease,
    ) -> AppResult<WorkTicket> {
        let epoch = self.active_epoch()?;
        // Config/SQLite/generation-control ordering: no facade lock over SQL.
        let intent = self.inner.repository.create(generation, epoch)?;
        Ok(WorkTicket {
            intent,
            lease,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn open_existing(
        &self,
        generation: seal::Generation,
        lease: OperationLease,
    ) -> AppResult<WorkTicket> {
        let epoch = self.active_epoch()?;
        let key = (generation.key()?, generation.generation);
        // Keep the transition fence through SQLite intent creation. Publishing
        // acquires this same lock before committing a clean seal.
        let publications = self
            .inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if publications.contains_key(&key) {
            return Err(error(Code::Busy));
        }
        let intent = self.inner.repository.open(generation, epoch)?;
        drop(publications);
        Ok(WorkTicket {
            intent,
            lease,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    fn enqueue<T>(
        &self,
        ticket: &WorkTicket,
        command: Command,
        project: fn(Reply) -> AppResult<T>,
    ) -> AppResult<Pending<T>> {
        self.inner.repository.validate(&ticket.intent)?;
        self.enqueue_admitted(ticket, command, project)
    }

    fn enqueue_admitted<T>(
        &self,
        ticket: &WorkTicket,
        command: Command,
        project: fn(Reply) -> AppResult<T>,
    ) -> AppResult<Pending<T>> {
        let (reply, result) = tokio::sync::oneshot::channel();
        let envelope = Envelope {
            intent: ticket.intent.clone(),
            _lease: ticket.lease.clone(),
            cancelled: ticket.cancelled.clone(),
            command,
            reply,
        };
        let state = self
            .inner
            .generation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let generation = state
            .as_ref()
            .filter(|generation| {
                generation.running
                    && !generation.stopping
                    && generation.epoch == ticket.intent.body().owner_epoch
            })
            .ok_or_else(|| error(Code::Interrupted))?;
        generation
            .sender
            .as_ref()
            .ok_or_else(|| error(Code::Interrupted))?
            .try_send(envelope)
            .map_err(|failure| match failure {
                mpsc::TrySendError::Full(_) => error(Code::Busy),
                mpsc::TrySendError::Disconnected(_) => error(Code::Interrupted),
            })?;
        Ok(Pending { result, project })
    }

    pub(crate) fn enqueue_open(&self, ticket: &WorkTicket) -> AppResult<Pending<u64>> {
        self.enqueue(ticket, Command::Open, |reply| match reply {
            Reply::Opened(count) => Ok(count),
            _ => Err(error(Code::Interrupted)),
        })
    }

    pub(crate) async fn add(
        &self,
        ticket: &WorkTicket,
        rows: Vec<(Vec<f32>, ChunkMetadata)>,
    ) -> AppResult<Vec<u64>> {
        self.enqueue(ticket, Command::Add(rows), |reply| match reply {
            Reply::Added(ids) => Ok(ids),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await
    }

    pub(crate) async fn scan_records(
        &self,
        ticket: &WorkTicket,
        start: u64,
        limit: u32,
    ) -> AppResult<Vec<super::owner::RecordCandidate>> {
        self.enqueue(ticket, Command::ScanRecords { start, limit }, |r| match r {
            Reply::Records(rows) => Ok(rows),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await
    }
    // Retained strict compatibility port; 5C consumes scan_records.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn scan(
        &self,
        ticket: &WorkTicket,
        start: u64,
        limit: u32,
    ) -> AppResult<Vec<(u64, Option<ChunkMetadata>)>> {
        self.enqueue(
            ticket,
            Command::Scan { start, limit },
            |reply| match reply {
                Reply::Scanned(rows) => Ok(rows),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }

    pub(crate) async fn delete(&self, ticket: &WorkTicket, ids: Vec<u64>) -> AppResult<Vec<bool>> {
        self.enqueue(ticket, Command::Delete(ids), |reply| match reply {
            Reply::Deleted(rows) => Ok(rows),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await
    }

    pub(crate) async fn replace(
        &self,
        ticket: &WorkTicket,
        id: u64,
        vector: Vec<f32>,
        metadata: ChunkMetadata,
    ) -> AppResult<u64> {
        self.enqueue(
            ticket,
            Command::Replace {
                id,
                vector,
                metadata,
            },
            |reply| match reply {
                Reply::Replaced(id) => Ok(id),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }

    pub(crate) async fn candidates(
        &self,
        ticket: &WorkTicket,
        vector: Vec<f32>,
        limit: u32,
    ) -> AppResult<Vec<super::owner::Candidate>> {
        self.enqueue(
            ticket,
            Command::Candidates { vector, limit },
            |reply| match reply {
                Reply::Candidates(c) => Ok(c),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }
    // Retained strict compatibility port; 5C consumes candidates.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn search(
        &self,
        ticket: &WorkTicket,
        vector: Vec<f32>,
        limit: u32,
    ) -> AppResult<Vec<(u64, f32, ChunkMetadata)>> {
        self.enqueue(
            ticket,
            Command::Search { vector, limit },
            |reply| match reply {
                Reply::Searched(hits) => Ok(hits),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }

    pub(crate) fn recovery_evidence(
        &self,
        identity: &str,
        generation: Uuid,
    ) -> AppResult<super::recovery::RecoveryEvidence> {
        self.inner
            .repository
            .recovery_evidence(identity, generation)
    }
    pub(crate) async fn begin_recovery(
        &self,
        ticket: &WorkTicket,
        evidence: super::recovery::RecoveryEvidence,
    ) -> AppResult<RecoveryTicket> {
        // Create the drop guard before awaiting entry. A disappeared observer
        // requests release even if the reader is still being opened in the pool.
        let mut guard = RecoveryTicket {
            owner: self.clone(),
            work: ticket.clone(),
            token: Uuid::nil(),
            active: AtomicBool::new(true),
            release_requested: Arc::new(AtomicBool::new(false)),
        };
        guard.token = self
            .enqueue(
                ticket,
                Command::BeginRecovery(evidence, guard.release_requested.clone()),
                |r| match r {
                    Reply::MaintenanceStarted(t) => Ok(t),
                    _ => Err(error(Code::Interrupted)),
                },
            )?
            .observe()
            .await?;
        Ok(guard)
    }
    pub(crate) async fn copy_recovery(
        &self,
        reader: &RecoveryTicket,
        start: u64,
        limit: u32,
    ) -> AppResult<Vec<(u64, Vec<f32>)>> {
        if !Arc::ptr_eq(&self.inner, &reader.owner.inner) {
            return Err(repair());
        }
        self.enqueue(
            &reader.work,
            Command::CopyRecovery {
                token: reader.token,
                start,
                limit,
            },
            |r| match r {
                Reply::VectorsCopied(v) => Ok(v),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }
    pub(crate) async fn verify_recovery(&self, reader: &RecoveryTicket) -> AppResult<()> {
        self.enqueue(
            &reader.work,
            Command::VerifyRecovery(reader.token),
            |r| match r {
                Reply::MaintenanceEnded => Ok(()),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }
    pub(crate) async fn end_recovery(&self, reader: RecoveryTicket) -> AppResult<()> {
        let pending = self.enqueue_admitted(
            &reader.work,
            Command::EndRecovery(reader.token),
            |r| match r {
                Reply::MaintenanceEnded => Ok(()),
                _ => Err(error(Code::Interrupted)),
            },
        )?;
        reader.active.store(false, Ordering::Release);
        pending.observe().await
    }
    pub(crate) async fn remove_generation(
        &self,
        ticket: &WorkTicket,
        source: seal::Generation,
    ) -> AppResult<()> {
        self.enqueue_admitted(ticket, Command::RemoveGeneration(source), |r| match r {
            Reply::MaintenanceEnded => Ok(()),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await
    }
    pub(crate) async fn retire_recovery(
        &self,
        ticket: &WorkTicket,
        fence: super::recovery::RecoveryFence,
    ) -> AppResult<()> {
        self.enqueue_admitted(ticket, Command::RetireRecovery(fence), |r| match r {
            Reply::MaintenanceEnded => Ok(()),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await
    }
    pub(crate) async fn begin_maintenance(
        &self,
        ticket: &WorkTicket,
    ) -> AppResult<MaintenanceTicket> {
        let mut guard = MaintenanceTicket {
            owner: self.clone(),
            work: ticket.clone(),
            token: Uuid::nil(),
            active: AtomicBool::new(true),
            release_requested: Arc::new(AtomicBool::new(false)),
        };
        guard.token = self
            .enqueue(
                ticket,
                Command::BeginMaintenance(guard.release_requested.clone()),
                |reply| match reply {
                    Reply::MaintenanceStarted(token) => Ok(token),
                    _ => Err(error(Code::Interrupted)),
                },
            )?
            .observe()
            .await?;
        Ok(guard)
    }

    pub(crate) async fn copy_vectors(
        &self,
        maintenance: &MaintenanceTicket,
        start: u64,
        limit: u32,
    ) -> AppResult<Vec<(u64, Vec<f32>)>> {
        if !Arc::ptr_eq(&self.inner, &maintenance.owner.inner) {
            return Err(repair());
        }
        self.enqueue(
            &maintenance.work,
            Command::CopyVectors {
                token: maintenance.token,
                start,
                limit,
            },
            |reply| match reply {
                Reply::VectorsCopied(vectors) => Ok(vectors),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }

    pub(crate) async fn end_maintenance(&self, maintenance: MaintenanceTicket) -> AppResult<()> {
        if !Arc::ptr_eq(&self.inner, &maintenance.owner.inner) {
            return Err(repair());
        }
        let pending = maintenance.enqueue_end()?;
        maintenance.active.store(false, Ordering::Release);
        pending.observe().await
    }

    pub(crate) async fn persist(&self, ticket: &WorkTicket) -> AppResult<PersistedReceipt> {
        let body = ticket.intent.body();
        let key = (body.identity.clone(), body.descriptor.generation);
        let operation = body.operation_id;
        let epoch = body.owner_epoch;
        self.begin_transition(key.clone(), operation, epoch)?;
        let pending = match self.enqueue(ticket, Command::Persist, |reply| match reply {
            Reply::Persisted(receipt) => Ok(receipt),
            _ => Err(error(Code::Interrupted)),
        }) {
            Ok(pending) => pending,
            Err(failure) => {
                self.clear_publication(&key, operation);
                return Err(failure);
            }
        };
        pending.observe().await
    }

    pub(crate) async fn publish(
        &self,
        ticket: &WorkTicket,
        receipt: &PersistedReceipt,
    ) -> AppResult<PublicationAck> {
        if ticket.intent.body() != receipt.intent().body() {
            return Err(repair());
        }
        let body = receipt.intent().body();
        let key = (body.identity.clone(), body.descriptor.generation);
        let operation = body.operation_id;
        self.ensure_transition(&key, operation, body.owner_epoch)?;
        self.enqueue(
            ticket,
            Command::Publish(receipt.clone()),
            |reply| match reply {
                Reply::Published(ack) => Ok(ack),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }
    pub(crate) async fn publish_plan(
        &self,
        ticket: &WorkTicket,
        receipt: &PersistedReceipt,
        plan: super::publication::PublicationPlan,
    ) -> AppResult<PublicationAck> {
        if ticket.intent.body() != receipt.intent().body() {
            return Err(repair());
        }
        let body = receipt.intent().body();
        self.ensure_transition(
            &(body.identity.clone(), body.descriptor.generation),
            body.operation_id,
            body.owner_epoch,
        )?;
        self.enqueue(
            ticket,
            Command::PublishPlan(receipt.clone(), plan),
            |reply| match reply {
                Reply::Published(ack) => Ok(ack),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }
    pub(crate) async fn retire(&self, ticket: &WorkTicket) -> AppResult<()> {
        self.enqueue_admitted(ticket, Command::Retire, |reply| match reply {
            Reply::Acknowledged => Ok(()),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await
    }
    #[cfg(test)]
    pub(crate) async fn corrupt_metadata(
        &self,
        ticket: &WorkTicket,
        id: u64,
        vector: Vec<f32>,
        metadata: String,
    ) -> AppResult<u64> {
        self.enqueue(
            ticket,
            Command::CorruptMetadata {
                id,
                vector,
                metadata,
            },
            |reply| match reply {
                Reply::Replaced(id) => Ok(id),
                _ => Err(error(Code::Interrupted)),
            },
        )?
        .observe()
        .await
    }

    pub(crate) async fn acknowledge(
        &self,
        ticket: &WorkTicket,
        ack: PublicationAck,
    ) -> AppResult<()> {
        if ticket.intent.body() != ack.intent().body() {
            return Err(repair());
        }
        self.inner.repository.validate_ack(&ack)?;
        self.enqueue_admitted(ticket, Command::Acknowledge(ack), |reply| match reply {
            Reply::Acknowledged => Ok(()),
            _ => Err(error(Code::Interrupted)),
        })?
        .observe()
        .await?;
        let body = ticket.intent.body();
        self.clear_publication(
            &(body.identity.clone(), body.descriptor.generation),
            body.operation_id,
        );
        Ok(())
    }

    fn clear_publication(&self, key: &PublicationKey, operation: Uuid) {
        let mut publications = self
            .inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if publications
            .get(key)
            .is_some_and(|pending| pending.operation == operation)
        {
            publications.remove(key);
        }
    }

    fn ensure_transition(
        &self,
        key: &PublicationKey,
        operation: Uuid,
        epoch: Uuid,
    ) -> AppResult<()> {
        let publications = self
            .inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if publications
            .get(key)
            .is_some_and(|pending| pending.operation == operation && pending.epoch == epoch)
        {
            Ok(())
        } else {
            Err(error(Code::Interrupted))
        }
    }

    fn begin_transition(&self, key: PublicationKey, operation: Uuid, epoch: Uuid) -> AppResult<()> {
        let mut publications = self
            .inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pending = PendingPublication { operation, epoch };
        match publications.get(&key) {
            Some(existing) if existing.operation == operation && existing.epoch == epoch => Ok(()),
            Some(_) => Err(error(Code::Busy)),
            None => {
                publications.insert(key, pending);
                Ok(())
            }
        }
    }

    fn clear_transitions_for_epoch(&self, epoch: Uuid) {
        self.inner
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, pending| pending.epoch != epoch);
    }

    pub(crate) async fn start_owned(&self, lease: OperationLease) -> AppResult<Uuid> {
        let (epoch, ready) = {
            let mut state = self.inner.generation.lock().unwrap();
            if state.as_ref().is_some_and(|g| g.done.borrow().is_some()) {
                state.take();
            }
            if let Some(generation) = state.as_ref() {
                if generation.stopping {
                    return Err(error(Code::Busy));
                }
                (generation.epoch, generation.ready.clone())
            } else {
                let epoch = Uuid::new_v4();
                let (sender, receiver) = mpsc::sync_channel(16);
                let (ready_tx, ready) = watch::channel(None);
                let (done_tx, done) = watch::channel(None);
                let stop = Arc::new(StopControl::new());
                let root = self.inner.root.clone();
                let repository = self.inner.repository.clone();
                let worker_lease = lease.clone();
                let worker_stop = stop.clone();
                #[cfg(test)]
                let hooks = self.inner.hooks.clone();
                let handle = std::thread::Builder::new()
                    .name("knowledge-index-owner".into())
                    .spawn(move || {
                        run_owner(
                            root,
                            repository,
                            epoch,
                            receiver,
                            ready_tx,
                            worker_stop,
                            worker_lease,
                            #[cfg(test)]
                            hooks,
                        )
                    })
                    .map_err(|_| error(Code::Interrupted))?;
                let join = JoinPayload {
                    handle: Some(handle),
                    base_lease: Some(lease),
                    done: done_tx,
                    stop: stop.clone(),
                };
                *state = Some(Generation {
                    epoch,
                    running: false,
                    stopping: false,
                    sender: Some(sender),
                    ready: ready.clone(),
                    done,
                    stop,
                    join: Some(join),
                });
                (epoch, ready)
            }
        };
        if let Err(failure) = observe(ready, READY_TIMEOUT).await {
            self.signal_stop(Some(epoch));
            return Err(failure);
        }
        let mut state = self.inner.generation.lock().unwrap();
        let generation = state
            .as_mut()
            .filter(|g| g.epoch == epoch && !g.stopping)
            .ok_or_else(|| error(Code::Interrupted))?;
        generation.running = true;
        Ok(epoch)
    }
    fn signal_stop(&self, expected: Option<Uuid>) -> Option<StopObservation> {
        let (observation, sender, join) = {
            let mut state = self.inner.generation.lock().unwrap();
            let generation = state.as_mut()?;
            if expected.is_some_and(|epoch| epoch != generation.epoch) {
                return None;
            }
            generation.stopping = true;
            generation.running = false;
            let attempt = generation.stop.request();
            let observation = StopObservation {
                epoch: generation.epoch,
                attempt,
                stop: generation.stop.clone(),
                done: generation.done.clone(),
                attempts: generation.stop.subscribe(),
            };
            (
                observation,
                generation.sender.take(),
                generation.join.take(),
            )
        };
        // No lifecycle lock is held while EOF wakes cleanup or fallback joins.
        drop(sender);
        if let Some(join) = join {
            start_join(
                join,
                #[cfg(test)]
                Some(self.inner.hooks.clone()),
            );
        }
        Some(observation)
    }
    pub(crate) async fn shutdown(&self) -> AppResult<()> {
        let Some(mut observation) = self.signal_stop(None) else {
            return Ok(());
        };
        let result = tokio::time::timeout(STOP_TIMEOUT, async {
            loop {
                let attempt = observation.attempts.borrow_and_update().clone();
                if attempt.number == observation.attempt {
                    if let Some(Err(failure)) = attempt.result {
                        observation.stop.observed_failure(observation.attempt);
                        return Err(error(failure));
                    }
                }
                if let Some(done) = *observation.done.borrow_and_update() {
                    return done.map_err(error);
                }
                tokio::select! {
                                    changed=observation.done.changed()=>if changed.is_err(){
                return Err(error(Code::Interrupted));
                }
                ,
                                    changed=observation.attempts.changed()=>if changed.is_err(){
                return Err(error(Code::Interrupted));
                }
                ,
                                }
            }
        })
        .await
        .map_err(|_| error(Code::Timeout))??;
        #[cfg(test)]
        {
            let gate = self
                .inner
                .hooks
                .stop_return
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take();
            if let Some(gate) = gate {
                let _ = gate.entered.send(());
                let _ = gate.release.recv_timeout(Duration::from_secs(10));
            }
        }
        self.clear_transitions_for_epoch(observation.epoch);
        Ok(result)
    }
}
async fn observe(mut receiver: watch::Receiver<Completion>, timeout: Duration) -> AppResult<()> {
    tokio::time::timeout(timeout, async {
        loop {
            if let Some(result) = *receiver.borrow_and_update() {
                return result.map_err(error);
            }
            receiver
                .changed()
                .await
                .map_err(|_| error(Code::Interrupted))?;
        }
    })
    .await
    .map_err(|_| error(Code::Timeout))?
}
struct StopObservation {
    epoch: Uuid,
    attempt: u64,
    stop: Arc<StopControl>,
    done: watch::Receiver<Completion>,
    attempts: watch::Receiver<Attempt>,
}
#[derive(Clone)]
struct Attempt {
    number: u64,
    result: Completion,
}
struct StopFlags {
    requested: u64,
    completed: Completion,
    failed_observed: bool,
    force: bool,
    updates: watch::Sender<Attempt>,
}
struct StopControl {
    flags: Mutex<StopFlags>,
    wake: Condvar,
}
impl StopControl {
    fn new() -> Self {
        Self {
            flags: Mutex::new(StopFlags {
                requested: 0,
                completed: None,
                failed_observed: false,
                force: false,
                updates: watch::channel(Attempt {
                    number: 0,
                    result: None,
                })
                .0,
            }),
            wake: Condvar::new(),
        }
    }
    fn subscribe(&self) -> watch::Receiver<Attempt> {
        self.flags
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .updates
            .subscribe()
    }
    fn request(&self) -> u64 {
        let mut flags = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        if flags.requested == 0
            || (matches!(flags.completed, Some(Err(_))) && flags.failed_observed)
        {
            flags.requested = flags.requested.saturating_add(1);
            flags.completed = None;
            flags.failed_observed = false;
            // Old observers retain the completed old channel when a retry starts.
            flags.updates = watch::channel(Attempt {
                number: flags.requested,
                result: None,
            })
            .0;
            self.wake.notify_all();
        }
        flags.requested
    }
    fn force(&self) {
        let mut flags = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        flags.force = true;
        if flags.requested == 0 {
            flags.requested = 1;
        }
        self.wake.notify_all();
    }
    fn current(&self) -> u64 {
        self.flags
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .requested
            .max(1)
    }
    fn publish(&self, attempt: u64, result: Outcome) {
        let mut flags = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        flags.completed = Some(result);
        flags.updates.send_replace(Attempt {
            number: attempt,
            result: Some(result),
        });
    }
    fn observed_failure(&self, attempt: u64) {
        let mut flags = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        if flags.requested == attempt && matches!(flags.completed, Some(Err(_))) {
            flags.failed_observed = true;
        }
    }
    fn wait_retry(&self, attempt: u64) -> bool {
        let mut flags = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        while flags.requested == attempt && !flags.force {
            flags = self.wake.wait(flags).unwrap_or_else(|e| e.into_inner());
        }
        !flags.force
    }
}
fn run_owner(
    root: PathBuf,
    repository: Arc<IndexRepository>,
    epoch: Uuid,
    receiver: Receiver<Envelope>,
    ready: watch::Sender<Completion>,
    stop: Arc<StopControl>,
    lease: OperationLease,
    #[cfg(test)] hooks: Arc<Hooks>,
) -> Outcome {
    let _worker_base = lease;
    let mut runtime = match Runtime::new(root, repository, epoch) {
        Ok(runtime) => runtime,
        Err(failure) => {
            let failure = code(failure);
            ready.send_replace(Some(Err(failure)));
            return Err(failure);
        }
    };
    #[cfg(test)]
    {
        let gate = hooks
            .ready
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(gate) = gate {
            let _ = gate.entered.send(());
            if gate.release.recv_timeout(Duration::from_secs(10)).is_err() {
                ready.send_replace(Some(Err(Code::Interrupted)));
                return Err(Code::Interrupted);
            }
        }
        if hooks.fail_ready.swap(false, Ordering::AcqRel) {
            ready.send_replace(Some(Err(Code::Interrupted)));
            return Err(Code::Interrupted);
        }
    }
    ready.send_replace(Some(Ok(())));
    for envelope in receiver {
        if envelope.cancelled.load(Ordering::Acquire)
            && !matches!(
                &envelope.command,
                Command::EndMaintenance(_) | Command::EndRecovery(_)
            )
        {
            let _ = envelope.reply.send(Err(error(Code::Cancelled)));
            continue;
        }
        runtime.release_requested_maintenance();
        #[cfg(test)]
        {
            let gate = hooks.command.lock().unwrap().take();
            if let Some(gate) = gate {
                let _ = gate.entered.send(());
                if gate.release.recv_timeout(Duration::from_secs(10)).is_err() {
                    let _ = envelope.reply.send(Err(error(Code::Interrupted)));
                    continue;
                }
            }
        }
        // Cancellation is checked only before entry. Observer loss cannot
        // discard entered synchronous work or move engine Drop to its caller.
        let result = match envelope.command {
            Command::BeginRecovery(evidence, release) => runtime
                .begin_recovery(&envelope.intent, evidence, release, envelope._lease.clone())
                .map(Reply::MaintenanceStarted),
            Command::CopyRecovery {
                token,
                start,
                limit,
            } => runtime
                .copy_recovery(&envelope.intent, token, start, limit)
                .map(Reply::VectorsCopied),
            Command::VerifyRecovery(token) => runtime
                .verify_recovery(&envelope.intent, token)
                .map(|()| Reply::MaintenanceEnded),
            Command::EndRecovery(token) => runtime
                .end_recovery(token)
                .map(|()| Reply::MaintenanceEnded),
            Command::RemoveGeneration(source) => runtime
                .remove_generation(&envelope.intent, &source)
                .map(|()| Reply::MaintenanceEnded),
            Command::RetireRecovery(fence) => runtime
                .retire_recovery(&envelope.intent, &fence)
                .map(|()| Reply::MaintenanceEnded),
            Command::Open => runtime.open(&envelope.intent).map(Reply::Opened),
            Command::Add(rows) => runtime.add(&envelope.intent, rows).map(Reply::Added),
            Command::ScanRecords { start, limit } => runtime
                .scan_records(&envelope.intent, start, limit)
                .map(Reply::Records),
            Command::Scan { start, limit } => runtime
                .scan(&envelope.intent, start, limit)
                .map(Reply::Scanned),
            Command::Delete(ids) => runtime.delete(&envelope.intent, ids).map(Reply::Deleted),
            Command::Replace {
                id,
                vector,
                metadata,
            } => runtime
                .replace(&envelope.intent, id, vector, metadata)
                .map(Reply::Replaced),
            Command::Search { vector, limit } => runtime
                .search(&envelope.intent, vector, limit)
                .map(Reply::Searched),
            Command::BeginMaintenance(release_requested) => runtime
                .begin_maintenance(&envelope.intent, release_requested, envelope._lease.clone())
                .map(Reply::MaintenanceStarted),
            Command::CopyVectors {
                token,
                start,
                limit,
            } => runtime
                .copy_vectors(&envelope.intent, token, start, limit)
                .map(Reply::VectorsCopied),
            Command::EndMaintenance(token) => runtime
                .end_maintenance(&envelope.intent, token)
                .map(|()| Reply::MaintenanceEnded),
            Command::Persist => runtime.persist(&envelope.intent).map(Reply::Persisted),
            Command::Publish(receipt) => runtime.publish(&receipt).map(Reply::Published),
            Command::PublishPlan(receipt, plan) => {
                runtime.publish_plan(&receipt, &plan).map(Reply::Published)
            }
            Command::Retire => runtime
                .retire(&envelope.intent)
                .map(|()| Reply::Acknowledged),
            Command::Candidates { vector, limit } => runtime
                .candidates(&envelope.intent, vector, limit)
                .map(Reply::Candidates),
            #[cfg(test)]
            Command::CorruptMetadata {
                id,
                vector,
                metadata,
            } => runtime
                .corrupt_metadata(&envelope.intent, id, vector, metadata)
                .map(Reply::Replaced),
            Command::Acknowledge(ack) => runtime.acknowledge(&ack).map(|()| Reply::Acknowledged),
        };
        let _ = envelope.reply.send(result);
        runtime.release_requested_maintenance();
    }
    let outcome = loop {
        let attempt = stop.current();
        let result = runtime.shutdown().map_err(code);
        stop.publish(attempt, result);
        if result.is_ok() || !stop.wait_retry(attempt) {
            break result;
        }
    };
    drop(runtime);
    // Engines, pool termination and both Rayon joins precede exit.
    outcome
}
/// Exactly one payload can join this owner, including on runtime teardown.
struct JoinPayload {
    handle: Option<JoinHandle<Outcome>>,
    base_lease: Option<OperationLease>,
    done: watch::Sender<Completion>,
    stop: Arc<StopControl>,
}
impl JoinPayload {
    fn finish(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let result = handle.join().unwrap_or(Err(Code::Interrupted));
        drop(self.base_lease.take());
        self.done.send_replace(Some(result));
    }
}
impl Drop for JoinPayload {
    fn drop(&mut self) {
        if self.handle.is_some() {
            // An unstarted coordinator closure may be dropped by spawn failure.
            // Wake failed-close waiters before synchronously joining this thread.
            self.stop.force();
            self.finish();
        }
    }
}
fn start_join(mut payload: JoinPayload, #[cfg(test)] hooks: Option<Arc<Hooks>>) {
    #[cfg(test)]
    if hooks.is_some_and(|hooks| hooks.fail_join_spawn.load(Ordering::Acquire)) {
        // Dropping the payload executes the same force-then-join fallback used
        // when the coordinator thread cannot be created.
        drop(payload);
        return;
    }
    // If spawn fails, dropping its moved closure invokes the synchronous fallback.
    let _ = std::thread::Builder::new()
        .name("knowledge-index-join".into())
        .spawn(move || payload.finish());
}
impl Drop for Inner {
    fn drop(&mut self) {
        let (sender, join, stop) = {
            let state = self.generation.get_mut().unwrap_or_else(|e| e.into_inner());
            let Some(generation) = state.as_mut() else {
                return;
            };
            (
                generation.sender.take(),
                generation.join.take(),
                generation.stop.clone(),
            )
        };
        stop.force();
        drop(sender);
        if let Some(join) = join {
            start_join(
                join,
                #[cfg(test)]
                Some(self.hooks.clone()),
            );
        }
    }
}
fn error(code: Code) -> AppError {
    AppError::Intelligence(code)
}
fn code(error: AppError) -> Code {
    match error {
        AppError::Intelligence(code) => code,
        _ => Code::Storage,
    }
}
#[cfg(test)]
#[path = "tests/stop_control.rs"]
mod stop_control_tests;
