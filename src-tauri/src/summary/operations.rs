//! Independent actual summary-child completion. Entries never retain owners.
use super::{
    SummaryCore,
    job_proof::SummaryJobProof,
    recovery::{SummaryReady, SummaryRecoveryFence},
    store::{error, optional_text, storage, text, uuid},
};
use crate::{
    error::AppResult,
    intelligence::{
        jobs::{JobReapPort, JobScheduler, OperationLease},
        types::IntelligenceErrorCode as Code,
    },
};
use rusqlite::{OptionalExtension, Transaction, params};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use uuid::Uuid;
struct ActualAttempt {
    owner: Option<OperationLease>,
    done: watch::Sender<bool>,
}
impl Drop for ActualAttempt {
    fn drop(&mut self) {
        // Actual process/job-child and shared slot release precede completion.
        drop(self.owner.take());
        self.done.send_replace(true);
    }
}
struct Entry {
    owner: Weak<ActualAttempt>,
    done: watch::Receiver<bool>,
}
#[derive(Default)]
pub(crate) struct AttemptRegistry {
    entries: Mutex<HashMap<Uuid, Entry>>,
}
impl AttemptRegistry {
    pub(crate) fn register(
        &self,
        job_id: Uuid,
        owner: OperationLease,
    ) -> AppResult<OperationLease> {
        let mut entries = self.entries.lock().map_err(|_| storage())?;
        entries.retain(|_, entry| !(*entry.done.borrow() && entry.owner.upgrade().is_none()));
        if entries.len() >= 16 || entries.contains_key(&job_id) {
            return Err(error(Code::Busy));
        }
        let (done, rx) = watch::channel(false);
        let attempt = Arc::new(ActualAttempt {
            owner: Some(owner),
            done,
        });
        entries.insert(
            job_id,
            Entry {
                owner: Arc::downgrade(&attempt),
                done: rx,
            },
        );
        Ok(attempt)
    }
    pub(crate) fn with_idle<T>(
        &self,
        jobs: &[Uuid],
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<Option<T>> {
        let entries = self.entries.lock().map_err(|_| storage())?;
        if jobs.iter().any(|id| {
            entries
                .get(id)
                .is_some_and(|e| !(*e.done.borrow() && e.owner.upgrade().is_none()))
        }) {
            return Ok(None);
        }
        operation().map(Some)
    }
    fn with_idle_where<T>(
        &self,
        mut matching: impl FnMut(Uuid) -> AppResult<bool>,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<Option<T>> {
        let entries = self.entries.lock().map_err(|_| storage())?;
        for (id, entry) in entries.iter() {
            if matching(*id)? && !(*entry.done.borrow() && entry.owner.upgrade().is_none()) {
                return Ok(None);
            }
        }
        operation().map(Some)
    }
    pub(crate) async fn drain(&self) -> AppResult<()> {
        loop {
            let idle = {
                let entries = self.entries.lock().map_err(|_| storage())?;
                entries
                    .values()
                    .all(|e| *e.done.borrow() && e.owner.upgrade().is_none())
            };
            if idle {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

pub(super) fn deadline_check(deadline: Instant) -> AppResult<()> {
    if Instant::now() >= deadline {
        Err(error(Code::Timeout))
    } else {
        Ok(())
    }
}
/// An observer never owns the actual SQLite lease. Even a dropped/timed-out
/// JoinHandle leaves this blocking closure and its original direct slot owned.
pub(super) async fn sql_owned<T: Send + 'static>(
    core: Arc<SummaryCore>,
    owner: OperationLease,
    deadline: Instant,
    operation: impl FnOnce(&Transaction<'_>) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tokio::task::spawn_blocking(move || {
        let _owner = owner;
        #[cfg(test)]
        core.sql_entry.enter();
        core.store
            .transaction(|tx| {
                deadline_check(deadline)?;
                let result = operation(tx)?;
                deadline_check(deadline)?;
                Ok(result)
            })
            .map_err(crate::intelligence::store::safe_store_error)
    })
    .await
    .map_err(|_| error(Code::Interrupted))?
}
#[derive(Clone)]
pub(super) enum DeleteFence {
    Ready(SummaryReady),
    Delivery(super::SummaryDeliveryReady),
    Recovery(SummaryRecoveryFence),
}
impl DeleteFence {
    pub(super) fn run<T>(&self, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        match self {
            Self::Ready(f) => f.run(operation),
            Self::Delivery(f) => f.run(operation),
            Self::Recovery(f) => f.run(operation),
        }
    }
}
#[derive(Clone)]
struct DeleteRecord {
    id: Uuid,
    session: Uuid,
    native: Option<SummaryJobProof>,
}
#[derive(Clone)]
enum Scope {
    Summary(DeleteRecord),
    Session(Uuid),
}
fn record(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<DeleteRecord>> {
    let mut q = tx.prepare("SELECT session_id,origin,job_id FROM session_summaries WHERE id=?1")?;
    let mut rows = q.query([id.to_string()])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let session = uuid(text(row, 0, 36)?)?;
    let native = match text(row, 1, 6)? {
        "legacy" => None,
        "native" => {
            let job = optional_text(row, 2, 36)?
                .map(uuid)
                .transpose()?
                .ok_or_else(storage)?;
            let proof = SummaryJobProof::anchored_in_tx(tx, job)?.ok_or_else(storage)?;
            if proof.summary_id() != id || proof.session_id() != session {
                return Err(storage());
            }
            Some(proof)
        }
        _ => return Err(storage()),
    };
    Ok(Some(DeleteRecord {
        id,
        session,
        native,
    }))
}
impl Scope {
    fn mark(&self, tx: &Transaction<'_>, deadline: Instant) -> AppResult<Vec<Uuid>> {
        match self {
            Self::Summary(expected) => {
                let Some(current) = record(tx, expected.id)? else {
                    return Ok(Vec::new());
                };
                self.require_record(expected, &current, tx)?;
                tx.execute(
                    "UPDATE session_summaries SET deleting=1 WHERE id=?1",
                    [expected.id.to_string()],
                )?;
                Ok(expected
                    .native
                    .iter()
                    .map(SummaryJobProof::job_id)
                    .collect())
            }
            Self::Session(session) => {
                // The session fence and all version fences share this one
                // Immediate transaction with any already-entered admission.
                tx.execute("UPDATE session_lifecycle SET summary_session_deleting=1,summary_terminal_pending=0 WHERE session_id=?1",[session.to_string()])?;
                self.all_native_reaped(tx, None, deadline)?;
                tx.execute(
                    "UPDATE session_summaries SET deleting=1 WHERE session_id=?1",
                    [session.to_string()],
                )?;
                let mut q=tx.prepare("SELECT id FROM intelligence_jobs WHERE kind='summary' AND session_id=?1 AND status IN ('queued','running') ORDER BY id LIMIT 33")?;
                let mut rows = q.query([session.to_string()])?;
                let mut jobs = Vec::new();
                while let Some(row) = rows.next()? {
                    deadline_check(deadline)?;
                    if jobs.len() == 32 {
                        return Err(error(Code::ResourceLimit));
                    }
                    jobs.push(uuid(text(row, 0, 36)?)?);
                }
                Ok(jobs)
            }
        }
    }
    fn require_record(
        &self,
        expected: &DeleteRecord,
        current: &DeleteRecord,
        tx: &Transaction<'_>,
    ) -> AppResult<()> {
        if current.id != expected.id
            || current.session != expected.session
            || current.native.is_some() != expected.native.is_some()
        {
            return Err(storage());
        }
        if let Some(proof) = &expected.native {
            proof.require_in_tx(tx)?;
            if current
                .native
                .as_ref()
                .is_none_or(|p| p.job_id() != proof.job_id())
            {
                return Err(storage());
            }
        }
        Ok(())
    }
    fn all_native_reaped(
        &self,
        tx: &Transaction<'_>,
        reap: Option<&JobReapPort>,
        deadline: Instant,
    ) -> AppResult<bool> {
        let (summary, session) = match self {
            Self::Summary(r) => (Some(r.id.to_string()), r.session.to_string()),
            Self::Session(s) => (None, s.to_string()),
        };
        let mut q=tx.prepare("SELECT id,job_id FROM session_summaries WHERE session_id=?1 AND (?2 IS NULL OR id=?2) AND origin='native' ORDER BY id LIMIT 200001")?;
        let mut rows = q.query(params![session, summary])?;
        let mut count = 0;
        let mut all = true;
        while let Some(row) = rows.next()? {
            deadline_check(deadline)?;
            count += 1;
            if count > 200000 {
                return Err(error(Code::ResourceLimit));
            }
            let id = uuid(text(row, 0, 36)?)?;
            let job = uuid(text(row, 1, 36)?)?;
            let proof = SummaryJobProof::anchored_in_tx(tx, job)?.ok_or_else(storage)?;
            if proof.summary_id() != id || proof.session_id().to_string() != session {
                return Err(storage());
            }
            if let Some(reap) = reap {
                if reap.prove(tx, job)?.is_none() {
                    all = false;
                }
            }
        }
        Ok(all)
    }
    fn idle<T>(
        &self,
        core: &SummaryCore,
        tx: &Transaction<'_>,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<Option<T>> {
        match self {
            Self::Summary(r)=>core.attempts.with_idle(&r.native.iter().map(SummaryJobProof::job_id).collect::<Vec<_>>(),operation),
            Self::Session(session)=>core.attempts.with_idle_where(|id|tx.query_row("SELECT EXISTS(SELECT 1 FROM session_summaries WHERE job_id=?1 AND session_id=?2 AND origin='native')",params![id.to_string(),session.to_string()],|r|r.get::<_,bool>(0)).map_err(Into::into),operation),
        }
    }
    fn require_fence(&self, tx: &Transaction<'_>) -> AppResult<()> {
        match self {
            Self::Summary(expected) => {
                let Some(current) = record(tx, expected.id)? else {
                    return Ok(());
                };
                self.require_record(expected, &current, tx)?;
                if tx.query_row(
                    "SELECT deleting FROM session_summaries WHERE id=?1",
                    [expected.id.to_string()],
                    |r| r.get::<_, i64>(0),
                )? != 1
                {
                    return Err(storage());
                }
            }
            Self::Session(session) => {
                let flag:Option<i64>=tx.query_row("SELECT summary_session_deleting FROM session_lifecycle WHERE session_id=?1",[session.to_string()],|r|r.get(0)).optional()?;
                if flag.is_some_and(|v| v != 1) {
                    return Err(storage());
                }
                let unfenced:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM session_summaries WHERE session_id=?1 AND deleting<>1)",[session.to_string()],|r|r.get(0))?;
                if unfenced {
                    return Err(storage());
                }
            }
        }
        Ok(())
    }
    fn hard_delete(&self, tx: &Transaction<'_>) -> AppResult<()> {
        match self {
            Self::Summary(r) => {
                tx.execute(
                    "DELETE FROM session_summaries WHERE id=?1 AND deleting=1",
                    [r.id.to_string()],
                )?;
            }
            Self::Session(s) => {
                tx.execute("DELETE FROM sessions WHERE id=?1 AND EXISTS(SELECT 1 FROM session_lifecycle WHERE session_id=?1 AND summary_session_deleting=1)",[s.to_string()])?;
            }
        }
        Ok(())
    }
}
async fn wait_scope(
    core: Arc<SummaryCore>,
    owner: OperationLease,
    fence: DeleteFence,
    scope: Scope,
    reap: Arc<JobReapPort>,
    deadline: Instant,
    delete: bool,
) -> AppResult<()> {
    loop {
        deadline_check(deadline)?;
        let (read_core, read_scope, read_reap, read_fence) =
            (core.clone(), scope.clone(), reap.clone(), fence.clone());
        let done = sql_owned(core.clone(), owner.clone(), deadline, move |tx| {
            read_fence.run(|| {
                read_scope.require_fence(tx)?;
                Ok(read_scope
                    .idle(&read_core, tx, || {
                        if !read_scope.all_native_reaped(tx, Some(&read_reap), deadline)? {
                            return Ok(false);
                        }
                        deadline_check(deadline)?;
                        if delete {
                            read_scope.hard_delete(tx)?;
                        }
                        Ok(true)
                    })?
                    .unwrap_or(false))
            })
        })
        .await?;
        if done {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
pub(super) async fn delete_summary(
    core: Arc<SummaryCore>,
    jobs: Arc<JobScheduler>,
    id: Uuid,
    owner: OperationLease,
    fence: DeleteFence,
) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let (sent, observed) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = async {
            let mark_fence = fence.clone();
            let scope = sql_owned(core.clone(), owner.clone(), deadline, move |tx| {
                mark_fence.run(|| {
                    let Some(record) = record(tx, id)? else {
                        return Ok(None);
                    };
                    let scope = Scope::Summary(record);
                    scope.mark(tx, deadline)?;
                    Ok(Some(scope))
                })
            })
            .await?;
            let Some(scope) = scope else {
                return Ok(());
            };
            if let Scope::Summary(record) = &scope {
                if let Some(proof) = &record.native {
                    jobs.cancel(proof.job_id()).await?;
                }
            }
            wait_scope(
                core.clone(),
                owner.clone(),
                fence,
                scope,
                Arc::new(jobs.reap_port()),
                deadline,
                true,
            )
            .await
        }
        .await;
        let _owner = owner;
        let _ = sent.send(result);
    });
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), observed)
        .await
        .map_err(|_| error(Code::Timeout))?
        .map_err(|_| error(Code::Interrupted))?
}
pub(crate) struct SummarySessionDeleteGuard {
    core: Arc<SummaryCore>,
    scope: Scope,
    reap: Arc<JobReapPort>,
    fence: DeleteFence,
    owner: OperationLease,
    deadline: Instant,
}
impl SummarySessionDeleteGuard {
    pub(crate) async fn delete_owned(self, session_id: Uuid) -> AppResult<()> {
        if !matches!(self.scope,Scope::Session(id) if id==session_id) {
            return Err(storage());
        }
        // Observation ends at the shared deadline. Any entered blocking SQL
        // still retains its actual owner and checks this deadline before DELETE.
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(self.deadline),
            wait_scope(
                self.core,
                self.owner,
                self.fence,
                self.scope,
                self.reap,
                self.deadline,
                true,
            ),
        )
        .await
        .map_err(|_| error(Code::Timeout))?
    }
}
pub(super) async fn prepare_session_delete(
    core: Arc<SummaryCore>,
    jobs: Arc<JobScheduler>,
    session: Uuid,
    owner: OperationLease,
    ready: SummaryReady,
) -> AppResult<SummarySessionDeleteGuard> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let fence = DeleteFence::Ready(ready);
    let (sent, observed) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = async {
            let (mark_fence, mark_scope) = (fence.clone(), Scope::Session(session));
            let pending = sql_owned(core.clone(), owner.clone(), deadline, move |tx| {
                mark_fence.run(|| mark_scope.mark(tx, deadline))
            })
            .await?;
            for job in pending {
                deadline_check(deadline)?;
                jobs.cancel(job).await?;
            }
            let scope = Scope::Session(session);
            let reap = Arc::new(jobs.reap_port());
            wait_scope(
                core.clone(),
                owner.clone(),
                fence.clone(),
                scope.clone(),
                reap.clone(),
                deadline,
                false,
            )
            .await?;
            Ok(SummarySessionDeleteGuard {
                core,
                scope,
                reap,
                fence,
                owner,
                deadline,
            })
        }
        .await;
        let _ = sent.send(result);
    });
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), observed)
        .await
        .map_err(|_| error(Code::Timeout))?
        .map_err(|_| error(Code::Interrupted))?
}
pub(super) async fn recover_local(
    core: Arc<SummaryCore>,
    owner: OperationLease,
    fence: SummaryRecoveryFence,
    reap: Arc<JobReapPort>,
    deadline: Instant,
) -> AppResult<()> {
    // Audit all actual summary claims, including terminal/unentered old work.
    loop {
        let (read_core, read_fence, read_reap) = (core.clone(), fence.clone(), reap.clone());
        let done=sql_owned(core.clone(),owner.clone(),deadline,move |tx|read_fence.run(|| {
            Ok(read_core.attempts.with_idle_where(|_|Ok(true),|| {
                let mut q=tx.prepare("SELECT job_id FROM session_summaries WHERE origin='native' ORDER BY job_id LIMIT 200001")?;
                let mut rows=q.query([])?;let mut count=0;let mut all=true;
                while let Some(row)=rows.next()? {deadline_check(deadline)?;count+=1;if count>200000 {return Err(error(Code::ResourceLimit));}let job=uuid(text(row,0,36)?)?;SummaryJobProof::anchored_in_tx(tx,job)?.ok_or_else(storage)?;if read_reap.prove(tx,job)?.is_none() {all=false;}}
                Ok(all)
            })?.unwrap_or(false))
        })).await?;
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Bound retained identities to32. Deleting a page precedes the next read.
    loop {
        let read_fence = fence.clone();
        let sessions=sql_owned(core.clone(),owner.clone(),deadline,move |tx|read_fence.run(|| {
            let mut q=tx.prepare("SELECT session_id FROM session_lifecycle WHERE summary_session_deleting=1 ORDER BY session_id LIMIT 32")?;let mut rows=q.query([])?;let mut out=Vec::new();while let Some(row)=rows.next()? {deadline_check(deadline)?;out.push(uuid(text(row,0,36)?)?);}Ok(out)
        })).await?;
        if sessions.is_empty() {
            break;
        }
        for session in sessions {
            wait_scope(
                core.clone(),
                owner.clone(),
                DeleteFence::Recovery(fence.clone()),
                Scope::Session(session),
                reap.clone(),
                deadline,
                true,
            )
            .await?;
        }
    }
    loop {
        let read_fence = fence.clone();
        let summaries = sql_owned(core.clone(), owner.clone(), deadline, move |tx| {
            read_fence.run(|| {
                let mut q = tx.prepare(
                    "SELECT id FROM session_summaries WHERE deleting=1 ORDER BY id LIMIT 32",
                )?;
                let mut rows = q.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    deadline_check(deadline)?;
                    let id = uuid(text(row, 0, 36)?)?;
                    out.push(record(tx, id)?.ok_or_else(storage)?);
                }
                Ok(out)
            })
        })
        .await?;
        if summaries.is_empty() {
            break;
        }
        for record in summaries {
            wait_scope(
                core.clone(),
                owner.clone(),
                DeleteFence::Recovery(fence.clone()),
                Scope::Summary(record),
                reap.clone(),
                deadline,
                true,
            )
            .await?;
        }
    }
    Ok(())
}
