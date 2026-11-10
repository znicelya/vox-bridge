pub mod content;
mod handler;
mod job_proof;
mod operations;
mod planner;
mod recovery;
mod snapshot;
mod store;
mod terminal;
#[cfg(test)]
pub(crate) use terminal::readiness_test_support;
pub mod types;
use crate::{
    error::AppResult,
    intelligence::{
        config::ServiceConfigStore,
        jobs::{JobReapPort, JobScheduler, OperationLease},
        types::*,
    },
    llm::LlmProvider,
    storage::LocalStore,
};
pub(crate) use operations::SummarySessionDeleteGuard;
pub(crate) use recovery::SummaryReady;
#[allow(
    unused_imports,
    reason = "Backend-private recovery capability is reserved for Task2"
)]
pub(crate) use recovery::SummaryRecoveryFence;
use std::sync::Arc;
pub(crate) use store::SummaryJobRequest;
pub(crate) use terminal::{
    SessionTerminalPort, SummaryDeliveryOwner, SummaryDeliveryReady, TerminalCommitIntent,
};
use uuid::Uuid;
pub struct SummaryService {
    store: Arc<LocalStore>,
    config: Arc<ServiceConfigStore>,
    jobs: Arc<JobScheduler>,
    core: Arc<SummaryCore>,
    delivery: Arc<terminal::DeliveryFacade>,
}
pub(crate) struct SummaryCore {
    store: Arc<LocalStore>,
    llm: Arc<dyn LlmProvider>,
    attempts: operations::AttemptRegistry,
    recovery: Arc<recovery::RecoveryControl>,
    #[cfg(test)]
    sql_entry: Arc<readiness_test_support::BoundaryControl>,
}
enum ManualReadiness {
    Delivery(SummaryDeliveryReady),
    #[cfg(test)]
    Local(SummaryReady),
}
impl ManualReadiness {
    fn run<T>(&self, operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        match self {
            Self::Delivery(ready) => ready.run(operation),
            #[cfg(test)]
            Self::Local(ready) => ready.run(operation),
        }
    }
}
impl SummaryService {
    pub fn new(
        store: Arc<LocalStore>,
        config: Arc<ServiceConfigStore>,
        jobs: Arc<JobScheduler>,
        llm: Arc<dyn LlmProvider>,
    ) -> Self {
        Self {
            core: Arc::new(SummaryCore {
                store: store.clone(),
                llm,
                attempts: operations::AttemptRegistry::default(),
                recovery: recovery::RecoveryControl::new(),
                #[cfg(test)]
                sql_entry: Arc::default(),
            }),
            store,
            delivery: terminal::DeliveryFacade::new(),
            config,
            jobs,
        }
    }
    pub(crate) fn register_handler(&self) -> AppResult<()> {
        self.jobs.register(
            JobKind::Summary,
            Arc::new(handler::SummaryHandler(self.core.clone())),
        )
    }
    pub(crate) async fn reconcile_local_owned(
        &self,
        owner: OperationLease,
        epoch: u64,
        reap: JobReapPort,
    ) -> AppResult<SummaryReady> {
        recovery::reconcile(self.core.clone(), owner, epoch, reap).await
    }
    pub(crate) fn require_epoch(&self, epoch: u64) -> AppResult<()> {
        self.core.recovery.ready()?.require_epoch(epoch)
    }
    pub(crate) fn signal_closing(&self) -> AppResult<()> {
        self.core.recovery.close()
    }
    pub(crate) async fn drain_actual(&self) -> AppResult<()> {
        self.core.recovery.drain().await?;
        self.core.attempts.drain().await
    }
    pub(crate) async fn generate_owned(
        &self,
        session_id: Uuid,
        profile_id: Option<Uuid>,
        owner: OperationLease,
        ready: SummaryDeliveryReady,
    ) -> AppResult<JobView> {
        ready.require_owner(&self.delivery)?;
        self.generate_with_readiness_owned(
            session_id,
            profile_id,
            owner,
            ManualReadiness::Delivery(ready),
        )
        .await
    }
    async fn generate_with_readiness_owned(
        &self,
        session_id: Uuid,
        profile_id: Option<Uuid>,
        owner: OperationLease,
        ready: ManualReadiness,
    ) -> AppResult<JobView> {
        let fence = self.jobs.admission_fence(JobKind::Summary)?;
        let events = self.jobs.event_port();
        let wake = self.jobs.wake_port();
        self.config
            .scoped(owner)
            .with_summary_selection_completion(
                crate::intelligence::config::SummarySelection::Manual { profile_id },
                move |tx, snapshot| {
                    ready.run(|| {
                        fence.run(|| {
                            LocalStore::create_summary_job_in_tx(
                                tx,
                                SummaryJobRequest {
                                    session_id,
                                    profile: snapshot.profile,
                                    automatic: false,
                                    initial_failure: None,
                                },
                            )
                        })
                    })
                },
                move |result| {
                    if let crate::intelligence::store::JobAdmissionResult::Created(view) = result {
                        events.publish(view.clone());
                        wake.notify_one();
                    }
                },
            )
            .await
            .map(|r| r.view().clone())
    }
    pub fn list(
        &self,
        session_id: Uuid,
        after: Option<types::SummaryCursor>,
        limit: u32,
    ) -> AppResult<types::SummaryPage> {
        self.store.summary_page(session_id, after, limit)
    }
    pub(crate) async fn delete_owned(
        &self,
        summary_id: Uuid,
        owner: OperationLease,
        ready: SummaryDeliveryReady,
    ) -> AppResult<()> {
        ready.require_owner(&self.delivery)?;
        operations::delete_summary(
            self.core.clone(),
            self.jobs.clone(),
            summary_id,
            owner,
            operations::DeleteFence::Delivery(ready),
        )
        .await
    }
    pub(crate) async fn prepare_session_delete_owned(
        &self,
        session_id: Uuid,
        owner: OperationLease,
    ) -> AppResult<SummarySessionDeleteGuard> {
        operations::prepare_session_delete(
            self.core.clone(),
            self.jobs.clone(),
            session_id,
            owner,
            self.core.recovery.ready()?,
        )
        .await
    }
}
#[cfg(test)]
mod tests;
