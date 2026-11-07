//! Facade admissions retain the scheduler; executing collaborators do not.
use super::{
    embedding::{EmbeddingExecution, Embeddings, error},
    index::{IndexOwner, PublicationPlan},
    types::*,
};
use crate::{
    error::AppResult,
    intelligence::{
        config::ServiceConfigStore,
        jobs::{JobContext, JobHandler, JobScheduler, OperationLease},
        store::{JobDraft, admit_job, decode_payload},
        types::{
            IntelligenceErrorCode as Code, JobAdmission, JobKind, JobStatus, JobView, ModelProfile,
        },
    },
    storage::LocalStore,
};
use rusqlite::params;
use serde::Deserialize;
#[cfg(test)]
use std::sync::Mutex;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;
mod delete;
mod reindex;
pub(crate) mod state;
#[cfg(test)]
pub(crate) use delete::DeleteInput;
mod continuation;
mod recovery;
mod searches;
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitPoint {
    AfterSqliteIntent,
    AfterVectorMutation,
    AfterVectorPersist,
    BeforeReady,
    AfterPointerPublication,
    AdmissionCallback,
    BeforeLocalCopy,
    BeforeSearchRecheck,
}
#[derive(Clone)]
pub(crate) struct KnowledgeService {
    pub(crate) core: Arc<Core>,
    jobs: Arc<JobScheduler>,
    searches: Arc<searches::Searches>,
}
pub(crate) struct Core {
    pub(crate) store: Arc<LocalStore>,
    pub(crate) config: Arc<ServiceConfigStore>,
    pub(crate) embeddings: Arc<dyn Embeddings>,
    pub(crate) index: IndexOwner,
    pub(crate) data_dir: PathBuf,
    pub(crate) serial: tokio::sync::Mutex<()>,
    #[cfg(test)]
    hook: Mutex<Option<CommitPoint>>,
    #[cfg(test)]
    action: Mutex<Option<(CommitPoint, Box<dyn FnOnce() + Send>)>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexInput {
    document_id: Uuid,
    document_version: u32,
}
impl KnowledgeService {
    pub(crate) fn new(
        store: Arc<LocalStore>,
        config: Arc<ServiceConfigStore>,
        jobs: Arc<JobScheduler>,
        embeddings: Arc<dyn Embeddings>,
        index: IndexOwner,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            core: Arc::new(Core {
                store,
                config,
                embeddings,
                index,
                data_dir,
                serial: tokio::sync::Mutex::new(()),
                #[cfg(test)]
                hook: Mutex::new(None),
                #[cfg(test)]
                action: Mutex::new(None),
            }),
            jobs,
            searches: Arc::new(searches::Searches::new()),
        }
    }
    pub(crate) fn register_handlers(&self) -> AppResult<()> {
        self.jobs.register(
            JobKind::Reindex,
            Arc::new(ReindexHandler(self.core.clone())),
        )?;
        self.jobs.register(
            JobKind::DeleteDocument,
            Arc::new(delete::DeleteHandler(self.core.clone())),
        )
    }
    pub(crate) fn register_continuation(&self) -> AppResult<()> {
        self.jobs
            .register_after_terminal(Arc::new(continuation::ImportContinuation {
                config: self.core.config.clone(),
                events: self.jobs.event_port(),
                wake: self.jobs.wake_port(),
            }))
    }
    pub(crate) async fn reindex_owned(
        &self,
        id: Uuid,
        owner: OperationLease,
    ) -> AppResult<JobView> {
        let fence = self.jobs.admission_fence(JobKind::Reindex)?;
        #[cfg(test)]
        let core = self.core.clone();
        let events = self.jobs.event_port();
        let wake = self.jobs.wake_port();
        let job = self
            .core
            .config
            .scoped(owner)
            .with_selected_embedding_completion(
                move |tx, profile| {
                    fence.run(|| {
                        let profile = profile.ok_or_else(|| error(Code::Configuration))?;
                        let f = state::fence(tx, id)?;
                        let status: String = tx.query_row(
                            "SELECT status FROM documents WHERE id=?1",
                            [id.to_string()],
                            |r| r.get(0),
                        )?;
                        if !matches!(
                            status.as_str(),
                            "parsed" | "ready" | "repair_required" | "failed"
                        ) {
                            return Err(error(Code::Busy));
                        }
                        let job = admit_job(
                            tx,
                            JobDraft {
                                id: Uuid::new_v4(),
                                kind: JobKind::Reindex,
                                session_id: None,
                                object_id: Some(id),
                                admission: JobAdmission::Manual {
                                    exclusion_key: format!("document:{id}"),
                                },
                                payload: serde_json::json!({
                                "version":1,
                                "profiles":[profile],
                                "input":{
                                "document_id":id,
                                "document_version":f.version}
                                }
                                ),
                                initial_failure: None,
                            },
                        )?
                        .view()
                        .clone();
                        #[cfg(test)]
                        core.checkpoint(CommitPoint::AdmissionCallback)?;
                        Ok(job)
                    })
                },
                move |job: &JobView| {
                    events.publish(job.clone());
                    wake.notify_one();
                },
            )
            .await?;
        Ok(job)
    }
    pub(crate) async fn delete_owned(
        &self,
        id: Uuid,
        _owner: OperationLease,
    ) -> AppResult<JobView> {
        delete::admit(self, id)
    }
    pub(crate) async fn reconcile_owned(&self, owner: OperationLease) -> AppResult<()> {
        let _serial = self.core.serial.lock().await;
        self.core.reconcile(owner, false).await
    }
    #[cfg(test)]
    pub(crate) async fn rebuild_owned(&self, owner: OperationLease) -> AppResult<()> {
        let _serial = self.core.serial.lock().await;
        self.core.reconcile(owner, true).await
    }
    pub(crate) async fn search_owned(
        &self,
        request: KnowledgeSearch,
        owner: OperationLease,
    ) -> AppResult<KnowledgeResults> {
        self.searches.run(self.core.clone(), request, owner).await
    }
    pub(crate) async fn shutdown_searches(&self) -> AppResult<()> {
        self.searches.shutdown().await
    }
    pub(crate) fn reopen_searches(&self) -> AppResult<()> {
        self.searches.reopen()
    }
    #[cfg(test)]
    pub(crate) fn at_checkpoint(&self, point: CommitPoint, action: impl FnOnce() + Send + 'static) {
        *self.core.action.lock().unwrap() = Some((point, Box::new(action)));
    }
    #[cfg(test)]
    pub(crate) fn crash_at(&self, point: CommitPoint) {
        *self.core.hook.lock().unwrap() = Some(point);
    }
}
impl Core {
    #[cfg(test)]
    pub(crate) fn checkpoint(&self, point: CommitPoint) -> AppResult<()> {
        let action = {
            let mut a = self.action.lock().unwrap();
            if a.as_ref().is_some_and(|(p, _)| *p == point) {
                a.take()
            } else {
                None
            }
        };
        if let Some((_, action)) = action {
            action();
        }
        let mut h = self.hook.lock().unwrap();
        if *h == Some(point) {
            *h = None;
            return Err(error(Code::IndexRepairRequired));
        }
        Ok(())
    }
}
struct ReindexHandler(Arc<Core>);
fn code(error: crate::error::AppError) -> Code {
    match error {
        crate::error::AppError::Intelligence(c) => c,
        _ => Code::Storage,
    }
}
#[async_trait::async_trait]
impl JobHandler for ReindexHandler {
    async fn run(&self, context: JobContext, payload: serde_json::Value) -> Result<(), Code> {
        let envelope =
            decode_payload(&serde_json::to_string(&payload).map_err(|_| Code::Configuration)?)
                .map_err(code)?;
        if envelope.profiles.len() != 1 {
            return Err(Code::Configuration);
        }
        let input: IndexInput =
            serde_json::from_value(envelope.input).map_err(|_| Code::Configuration)?;
        self.0
            .reindex(context, input, &envelope.profiles[0])
            .await
            .map_err(code)
    }
    fn terminal_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        id: Uuid,
        status: JobStatus,
        _code: Option<Code>,
        _awaiting: bool,
    ) -> AppResult<()> {
        if status != JobStatus::Succeeded {
            tx.execute(
                "UPDATE documents SET \
                    status='repair_required',\
                    error_code='index_repair_required' WHERE \
                    status='indexing' AND id=(SELECT object_id \
                    FROM intelligence_jobs WHERE id=?1) AND \
                    version=(SELECT json_extract(payload_json,\
                    '$.input.document_version') FROM \
                    intelligence_jobs WHERE id=?1)",
                [id.to_string()],
            )?;
            tx.execute(
                "INSERT INTO knowledge_repairs(document_id,\
                    reason) SELECT d.id,'unsealed_generation' \
                    FROM documents d JOIN intelligence_jobs j ON \
                    j.object_id=d.id WHERE j.id=?1 AND \
                    d.status='repair_required' ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
                [id.to_string()],
            )?;
        }
        Ok(())
    }
}
