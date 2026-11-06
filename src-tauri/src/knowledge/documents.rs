//! Local admission. Its separately registered handler never retains the scheduler.
use super::{
    copying::{self, CopyRequest, FileCopier, ManagedCopier},
    parser::DocumentParser,
    store::{self, error, safe},
    types::{DocumentDetail, DocumentPage},
};
use crate::{
    error::AppResult,
    intelligence::{
        config::ServiceConfigStore,
        jobs::{JobHandler, JobScheduler, OperationLease},
        types::{IntelligenceErrorCode as Code, JobKind, JobView},
    },
    storage::LocalStore,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Semaphore, oneshot, watch};
use uuid::Uuid;
mod admission;
mod handler;
#[derive(Clone)]
pub struct DocumentService {
    store: Arc<LocalStore>,
    _config: Arc<ServiceConfigStore>,
    jobs: Arc<JobScheduler>,
    parser: Arc<dyn DocumentParser>,
    data_dir: PathBuf,
    pub(crate) copier: Arc<dyn FileCopier>,
    file_gate: Arc<Semaphore>,
    pub(crate) observation_timeout: Duration,
}
struct CancelObserver(watch::Sender<bool>);
impl Drop for CancelObserver {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}
impl DocumentService {
    /// Inert: no files, registration, worker, parser or configuration I/O.
    pub fn new(
        store: Arc<LocalStore>,
        config: Arc<ServiceConfigStore>,
        jobs: Arc<JobScheduler>,
        parser: Arc<dyn DocumentParser>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            _config: config,
            jobs,
            parser,
            data_dir,
            copier: Arc::new(ManagedCopier),
            file_gate: Arc::new(Semaphore::new(2)),
            observation_timeout: Duration::from_secs(30),
        }
    }
    pub async fn import(&self, path: PathBuf) -> AppResult<JobView> {
        self.import_owned(path, Arc::new(())).await
    }
    pub async fn replace(&self, id: Uuid, path: PathBuf) -> AppResult<JobView> {
        self.replace_owned(id, path, Arc::new(())).await
    }
    pub(crate) async fn import_owned(
        &self,
        path: PathBuf,
        lease: OperationLease,
    ) -> AppResult<JobView> {
        self.operation(None, path, lease).await
    }
    pub(crate) async fn replace_owned(
        &self,
        id: Uuid,
        path: PathBuf,
        lease: OperationLease,
    ) -> AppResult<JobView> {
        self.operation(Some(id), path, lease).await
    }
    pub fn page(&self, after: Option<i64>, limit: u32) -> AppResult<DocumentPage> {
        store::page(&self.store, after, limit)
    }
    pub fn get(&self, id: Uuid) -> AppResult<Option<DocumentDetail>> {
        store::get(&self.store, id)
    }
    /// Explicit composition seam; duplicate or post-start registration fails closed.
    pub(crate) fn register_import_handler(&self) -> AppResult<()> {
        self.jobs.register(JobKind::Import, self.handler())
    }
    pub(crate) fn handler(&self) -> Arc<dyn JobHandler> {
        Arc::new(handler::ImportHandler {
            store: self.store.clone(),
            parser: self.parser.clone(),
            gate: self.jobs.parser_gate(),
            events: self.jobs.event_port(),
            data_dir: self.data_dir.clone(),
        })
    }
    async fn operation(
        &self,
        replacement: Option<Uuid>,
        path: PathBuf,
        lease: OperationLease,
    ) -> AppResult<JobView> {
        let fence = self.jobs.admission_fence(JobKind::Import)?;
        let permit = Arc::new(
            self.file_gate
                .clone()
                .try_acquire_owned()
                .map_err(|_| error(Code::Busy))?,
        );
        let (cancel, cancelled) = watch::channel(false);
        let _observer = CancelObserver(cancel.clone());
        let (reply, result) = oneshot::channel();
        let owner = self.clone();
        let mut stop = fence.cancellation();
        tokio::spawn(async move {
            let _lease = lease;
            let _permit = permit;
            let file_id = Uuid::new_v4();
            let copier = owner.copier.clone();
            let request = CopyRequest {
                source: path,
                data_dir: owner.data_dir.clone(),
                file_id,
                store: owner.store.clone(),
                cancel: cancelled.clone(),
            };
            let blocking_lease = _lease.clone();
            let blocking_permit = _permit.clone();
            let mut work = tokio::task::spawn_blocking(move || {
                let _lease = blocking_lease;
                let _permit = blocking_permit;
                copier.copy(request)
            });
            let mut observer_cancel = cancelled.clone();
            let copied=tokio::select! {biased;
                _=crate::intelligence::http::cancelled(&mut stop)=>{cancel.send_replace(true);work.await},
                _=crate::intelligence::http::cancelled(&mut observer_cancel)=>work.await,
                value=&mut work=>value,
            }.map_err(|_|error(Code::Interrupted)).and_then(|value|value);
            let candidate_owned = copied.is_ok();
            let admitted = match copied {
                Ok(copy) if !*cancelled.borrow() && !*stop.borrow() => admission::admit(
                    &owner.store,
                    &fence,
                    replacement,
                    file_id,
                    &copy,
                    &cancelled,
                ),
                Ok(_) => Err(error(Code::Cancelled)),
                Err(error) => Err(error),
            };
            let result = match admitted {
                Ok((job, true)) => {
                    owner.jobs.event_port().publish(job.clone());
                    owner.jobs.wake();
                    Ok(job)
                }
                other => {
                    let cleaned = if candidate_owned {
                        owner.store.transaction(|tx|{tx.execute("UPDATE knowledge_managed_files SET state='delete_pending' WHERE file_id=?1 AND state='copying'",[file_id.to_string()])?;Ok(())}).map_err(safe).and_then(|_|copying::cleanup_candidate(&owner.store,&owner.data_dir,file_id))
                    } else {
                        Ok(())
                    };
                    match cleaned {
                        Err(e) => Err(e),
                        Ok(()) => other.map(|(job, _)| job),
                    }
                }
            };
            let _ = reply.send(result);
        });
        tokio::time::timeout(self.observation_timeout, result)
            .await
            .map_err(|_| error(Code::Timeout))?
            .map_err(|_| error(Code::Interrupted))?
    }
}
