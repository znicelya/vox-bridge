//! Complete claim accounting, independent of SQL status and observer lifetime.
//! Lock order is SQLite -> generation control -> registry -> frame. The proof
//! port only takes registry/frame locks, never the scheduler/control mutex.
use super::*;
use rusqlite::{Transaction, types::ValueRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FRAME_CAP: usize = 18; // Two handlers plus sixteen actual child scopes.
const PAYLOAD_CAP: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum Lifecycle {
    Fresh,
    Active(Uuid),
    Joined(Uuid),
}
pub(super) struct Ledger {
    lifecycle: Lifecycle,
    issued: u64,
    joined: u64,
    pub(super) frames: HashMap<Uuid, Arc<Frame>>,
}
impl Ledger {
    fn audit(&self) -> AppResult<()> {
        if self.frames.len() > FRAME_CAP
            || self.joined.checked_add(self.frames.len() as u64) != Some(self.issued)
            || self.frames.iter().any(|(id, frame)| *id != frame.id
                || matches!(self.lifecycle, Lifecycle::Active(generation) if frame.generation != generation))
        { return Err(error(IntelligenceErrorCode::Interrupted)) }
        match self.lifecycle {
            Lifecycle::Fresh if self.issued != 0 || !self.frames.is_empty() => {
                Err(error(IntelligenceErrorCode::Interrupted))
            }
            Lifecycle::Joined(_) if !self.frames.is_empty() => {
                Err(error(IntelligenceErrorCode::Interrupted))
            }
            _ => Ok(()),
        }
    }
    fn prune(&mut self) -> AppResult<()> {
        self.audit()?;
        // No historical frame collection: keep only actual unjoined work.
        let before = self.frames.len();
        self.frames.retain(|_, frame| !frame.ready());
        self.joined = self
            .joined
            .checked_add((before - self.frames.len()) as u64)
            .ok_or_else(|| error(IntelligenceErrorCode::Interrupted))?;
        self.audit()
    }
}
pub(super) struct Registry {
    id: Uuid,
    pub(super) inner: Mutex<Ledger>,
    completed: Arc<Notify>,
}
impl Registry {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: Uuid::new_v4(),
            inner: Mutex::new(Ledger {
                lifecycle: Lifecycle::Fresh,
                issued: 0,
                joined: 0,
                frames: HashMap::new(),
            }),
            completed: Arc::new(Notify::new()),
        })
    }
    pub(super) fn begin(&self, generation: Uuid) -> AppResult<()> {
        let mut ledger = self.inner.lock().unwrap();
        ledger.audit()?;
        if matches!(ledger.lifecycle, Lifecycle::Active(_)) {
            return Err(error(IntelligenceErrorCode::Busy));
        }
        ledger.lifecycle = Lifecycle::Active(generation);
        ledger.issued = 0;
        ledger.joined = 0;
        Ok(())
    }
    pub(super) fn joined(&self, generation: Uuid) -> AppResult<()> {
        let mut ledger = self.inner.lock().unwrap();
        if !matches!(ledger.lifecycle, Lifecycle::Active(id) if id == generation) {
            return Err(error(IntelligenceErrorCode::Interrupted));
        }
        ledger.prune()?;
        if !ledger.frames.is_empty() {
            return Err(error(IntelligenceErrorCode::Interrupted));
        }
        ledger.lifecycle = Lifecycle::Joined(generation);
        ledger.audit()
    }
    /// Workers and owned cleanup have joined, and shared slots have drained.
    /// Slot release precedes per-job completion publication, so await that last
    /// drop without holding SQLite, control, registry or frame locks.
    pub(super) async fn quiesce(&self, generation: Uuid) -> AppResult<()> {
        loop {
            let completed = self.completed.notified();
            tokio::pin!(completed);
            completed.as_mut().enable();
            {
                let ledger = self.inner.lock().unwrap();
                ledger.audit()?;
                if !matches!(ledger.lifecycle, Lifecycle::Active(id) if id == generation) {
                    return Err(error(IntelligenceErrorCode::Interrupted));
                }
                let mut pending = false;
                for frame in ledger.frames.values() {
                    let state = frame.state.lock().unwrap();
                    // Worker join cannot repair an unentered or open factory.
                    // Fail closed instead of waiting for an impossible event.
                    if !state.committed || !state.closed || !state.handler_done {
                        return Err(error(IntelligenceErrorCode::Interrupted));
                    }
                    pending |= state.children != 0;
                }
                if !pending {
                    return Ok(());
                }
            }
            completed.await;
        }
    }
    pub(super) fn prune(&self) -> AppResult<()> {
        self.inner.lock().unwrap().prune()
    }
    pub(super) fn register(
        self: &Arc<Self>,
        id: Uuid,
        rowid: i64,
        payload: String,
    ) -> AppResult<PendingClaim> {
        let mut ledger = self.inner.lock().unwrap();
        ledger.prune()?;
        let Lifecycle::Active(generation) = ledger.lifecycle else {
            return Err(error(IntelligenceErrorCode::Busy));
        };
        if rowid <= 0 || ledger.frames.contains_key(&id) || ledger.frames.len() == FRAME_CAP {
            return Err(error(IntelligenceErrorCode::Interrupted));
        }
        let frame = Arc::new(Frame {
            id,
            rowid,
            payload,
            generation,
            completed: self.completed.clone(),
            state: Mutex::new(FrameState {
                committed: false,
                closed: false,
                handler_done: false,
                children: 0,
            }),
        });
        ledger.issued = ledger
            .issued
            .checked_add(1)
            .ok_or_else(|| error(IntelligenceErrorCode::Interrupted))?;
        ledger.frames.insert(id, frame.clone());
        Ok(PendingClaim {
            registry: self.clone(),
            frame: Some(frame),
        })
    }
}
pub(super) struct PendingClaim {
    registry: Arc<Registry>,
    frame: Option<Arc<Frame>>,
}
impl PendingClaim {
    pub(super) fn commit(mut self) -> Arc<Frame> {
        let frame = self.frame.take().unwrap();
        frame.state.lock().unwrap().committed = true;
        frame
    }
}
impl Drop for PendingClaim {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            let mut ledger = self.registry.inner.lock().unwrap();
            if ledger
                .frames
                .get(&frame.id)
                .is_some_and(|current| Arc::ptr_eq(current, &frame))
            {
                ledger.frames.remove(&frame.id);
                ledger.issued -= 1;
            }
        }
    }
}
struct FrameState {
    committed: bool,
    closed: bool,
    handler_done: bool,
    children: usize,
}
pub(super) struct Frame {
    id: Uuid,
    rowid: i64,
    payload: String,
    generation: Uuid,
    state: Mutex<FrameState>,
    completed: Arc<Notify>,
}
impl Frame {
    fn ready(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.committed && state.closed && state.handler_done && state.children == 0
    }
    /// Called only after the actual run future ended and continuation ownership
    /// was reserved. This factory never opens again, including after rearm.
    pub(super) fn close_handler(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.handler_done = true;
    }
    pub(super) fn child(self: &Arc<Self>) -> AppResult<ChildCompletion> {
        let mut state = self.state.lock().unwrap();
        if !state.committed || state.closed {
            return Err(error(IntelligenceErrorCode::Cancelled));
        }
        state.children += 1;
        Ok(ChildCompletion(self.clone()))
    }
}
pub(super) struct ChildCompletion(Arc<Frame>);
impl Drop for ChildCompletion {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().children -= 1;
        self.0.completed.notify_waiters();
    }
}

#[derive(Clone)]
pub(crate) struct JobReapPort {
    registry: Arc<Registry>,
}
impl JobReapPort {
    pub(super) fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }
    /// Nonmutating: no status writer, registry pruning or scheduler/control lock.
    /// Every actual claim is registered before Running in the same transaction;
    /// an absent UUID is safe only under the audited complete claim ledger or
    /// explicit fresh/joined lifecycle. Row order/frontiers are never authority.
    pub(crate) fn prove(&self, tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<JobReapProof>> {
        let Some(job) = store::job_in_transaction(tx, id)? else {
            return Ok(None);
        };
        let (rowid, payload) = identity(tx, id)?;
        let ledger = self.registry.inner.lock().unwrap();
        ledger.audit()?;
        let basis = if let Some(frame) = ledger.frames.get(&id) {
            if frame.rowid != rowid || frame.payload != payload {
                return Err(error(IntelligenceErrorCode::Interrupted));
            }
            if !frame.ready() {
                return Ok(None);
            }
            (Basis::Frame, Some(frame.generation))
        } else {
            match ledger.lifecycle {
                Lifecycle::Fresh => (Basis::Fresh, None),
                Lifecycle::Joined(generation) => (Basis::Joined, Some(generation)),
                Lifecycle::Active(generation) => (Basis::Ledger, Some(generation)),
            }
        };
        if matches!(job.status, JobStatus::Queued | JobStatus::Running) {
            return Ok(None);
        }
        Ok(Some(JobReapProof {
            evidence: JobReapEvidence {
                id,
                rowid,
                payload_sha256: payload,
                status: job.status,
                registry: self.registry.id,
                generation: basis.1,
                basis: basis.0,
            },
        }))
    }
}
pub(super) fn identity(tx: &Transaction<'_>, id: Uuid) -> AppResult<(i64, String)> {
    let mut q = tx.prepare("SELECT rowid,payload_json FROM intelligence_jobs WHERE id=?1")?;
    let mut rows = q.query([id.to_string()])?;
    let row = rows
        .next()?
        .ok_or_else(|| error(IntelligenceErrorCode::Interrupted))?;
    let ValueRef::Text(raw) = row.get_ref(1)? else {
        return Err(error(IntelligenceErrorCode::Configuration));
    };
    if raw.len() > PAYLOAD_CAP {
        return Err(error(IntelligenceErrorCode::ResourceLimit));
    }
    let rowid: i64 = row.get(0)?;
    if rowid <= 0 {
        return Err(error(IntelligenceErrorCode::Interrupted));
    }
    Ok((rowid, format!("{:x}", Sha256::digest(raw))))
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Basis {
    Fresh,
    Joined,
    Ledger,
    Frame,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct JobReapEvidence {
    id: Uuid,
    rowid: i64,
    payload_sha256: String,
    status: JobStatus,
    registry: Uuid,
    generation: Option<Uuid>,
    basis: Basis,
}
impl JobReapEvidence {
    pub(crate) fn matches(&self, id: Uuid, payload: &str, status: JobStatus) -> bool {
        self.id == id
            && self.payload_sha256 == payload
            && self.status == status
            && self.rowid > 0
            && !self.registry.is_nil()
            && (self.generation.is_none() == (self.basis == Basis::Fresh))
            && self
                .generation
                .is_none_or(|generation| !generation.is_nil())
            && !matches!(status, JobStatus::Queued | JobStatus::Running)
    }
}
pub(crate) struct JobReapProof {
    evidence: JobReapEvidence,
}
impl JobReapProof {
    pub(crate) fn evidence(&self) -> JobReapEvidence {
        self.evidence.clone()
    }
}
