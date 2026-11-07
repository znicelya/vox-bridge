use super::*;
use crate::intelligence::{jobs::OperationLease, subscriptions::IntelligenceEmitter, types::*};
struct Operation {
    _process: Arc<File>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
impl AppState {
    pub async fn start_intelligence(&self) -> AppResult<()> {
        // Hold this through actual index readiness/job publication. Close takes
        // the same gate before treating the previous generation as joined.
        let _startup = self.intelligence_startup.lock().await;
        let epoch = self.admitted_epoch()?;
        self.with_open_epoch(|| self.probes.reopen())?;
        let _index = match self
            .index_owner
            .start_owned(self._process_lock.clone())
            .await
        {
            Ok(index) => index,
            Err(failure) => {
                let _ = self.probes.shutdown().await;
                let _ = self.shutdown_index_after_startup_failure().await;
                return Err(failure);
            }
        };
        if let Err(failure) = self.check_epoch(epoch) {
            let _ = self.shutdown_index_after_startup_failure().await;
            return Err(failure);
        }
        let local = match self
            .knowledge
            .reopen_searches()
            .and_then(|()| self.intelligence_lease())
        {
            Ok(lease) => self.knowledge.reconcile_owned(lease).await,
            Err(failure) => Err(failure),
        };
        if let Err(failure) = local {
            let _ = self.knowledge.shutdown_searches().await;
            let _ = self.shutdown_index_after_startup_failure().await;
            return Err(failure);
        }
        let jobs = self.with_open_epoch(|| self.jobs.start_owned(self._process_lock.clone()));
        if jobs.is_err() {
            let _ = self.shutdown_index_after_startup_failure().await;
        }
        jobs
    }
    async fn shutdown_index_after_startup_failure(&self) -> AppResult<()> {
        let _direct_operations = self
            .intelligence_operations
            .clone()
            .acquire_many_owned(16)
            .await
            .map_err(|_| super::lifecycle::closing_error())?;
        self.index_owner.shutdown().await
    }
    /// Host-only successful reuse: rearm workers only after acknowledged cleanup.
    pub async fn reopen_window(&self) -> AppResult<()> {
        self.open_window()?;
        self.start_intelligence().await
    }
    pub(super) fn intelligence_lease(&self) -> AppResult<OperationLease> {
        self.with_open_epoch(|| {
            let permit = self
                .intelligence_operations
                .clone()
                .try_acquire_owned()
                .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Busy))?;
            Ok(Arc::new(Operation {
                _process: self._process_lock.clone(),
                _permit: permit,
            }) as OperationLease)
        })
    }
    pub fn get_managed_data_location(&self) -> ManagedDataLocation {
        ManagedDataLocation {
            data_dir: self.data_dir().to_path_buf(),
        }
    }
    pub async fn search_knowledge(
        &self,
        request: crate::knowledge::types::KnowledgeSearch,
    ) -> AppResult<crate::knowledge::types::KnowledgeResults> {
        self.knowledge
            .search_owned(request, self.intelligence_lease()?)
            .await
    }
    pub async fn reindex_document(&self, id: Uuid) -> AppResult<JobView> {
        self.knowledge
            .reindex_owned(id, self.intelligence_lease()?)
            .await
    }
    pub async fn delete_document(&self, id: Uuid) -> AppResult<JobView> {
        self.knowledge
            .delete_owned(id, self.intelligence_lease()?)
            .await
    }
    pub async fn list_model_profiles(&self) -> AppResult<Vec<ModelProfileView>> {
        self.service_config
            .scoped(self.intelligence_lease()?)
            .list()
            .await
    }
    pub async fn save_model_profile(
        &self,
        request: SaveModelProfile,
    ) -> AppResult<ModelProfileView> {
        self.service_config
            .scoped(self.intelligence_lease()?)
            .save(request)
            .await
    }
    pub async fn delete_model_profile(&self, id: Uuid) -> AppResult<()> {
        self.service_config
            .scoped(self.intelligence_lease()?)
            .delete(id)
            .await
    }
    pub async fn get_intelligence_settings(&self) -> AppResult<IntelligenceSettings> {
        self.service_config
            .scoped(self.intelligence_lease()?)
            .settings()
            .await
    }
    pub async fn save_intelligence_settings(
        &self,
        settings: IntelligenceSettings,
    ) -> AppResult<IntelligenceSettings> {
        self.service_config
            .scoped(self.intelligence_lease()?)
            .save_settings(settings)
            .await
    }
    pub async fn test_model_profile(&self, id: Uuid) -> AppResult<ConnectionProbe> {
        self.probes
            .model(
                self.service_config.clone(),
                self.json_http.clone(),
                id,
                self.intelligence_lease()?,
            )
            .await
    }
    pub async fn test_translation_provider(&self, id: Uuid) -> AppResult<ConnectionProbe> {
        self.probes
            .translation(
                self.config_io.clone(),
                self.factory.clone(),
                self.store.clone(),
                id,
                self.intelligence_lease()?,
            )
            .await
    }
    pub fn get_job(&self, id: Uuid) -> AppResult<Option<JobView>> {
        self.store.get_job(id)
    }
    pub fn list_jobs(&self, after: Option<i64>, limit: Option<u32>) -> AppResult<JobPage> {
        self.store.list_jobs(after, limit.unwrap_or(50))
    }
    pub async fn cancel_job(&self, id: Uuid) -> AppResult<JobView> {
        let lease = self.intelligence_lease()?;
        let jobs = self.jobs.clone();
        let (reply, result) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let _lease = lease;
            let outcome = async {
                let job = jobs.cancel(id).await?;
                jobs.cleanup_cancelled().await?;
                Ok(job)
            }
            .await;
            let _ = reply.send(outcome);
        });
        tokio::time::timeout(std::time::Duration::from_secs(10), result)
            .await
            .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Timeout))?
            .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Interrupted))?
    }
    pub async fn subscribe_intelligence(
        &self,
        subscriber_id: String,
        emitter: Arc<dyn IntelligenceEmitter>,
    ) -> AppResult<SubscriptionTicket> {
        let epoch = self.admitted_epoch()?;
        let _permit = self
            .subscription_admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Busy))?;
        let _mutation = self.subscription_mutation.lock().await;
        self.check_epoch(epoch)?;
        self.intelligence_subscriptions
            .subscribe(subscriber_id, self.jobs.subscribe(), emitter)
            .await
    }
    pub async fn unsubscribe_intelligence(&self, id: Uuid) -> AppResult<()> {
        self.intelligence_subscriptions.unsubscribe(id).await
    }

    #[cfg(test)]
    pub(super) fn is_index_running_for_test(&self) -> bool {
        self.index_owner.running_epoch_for_test().is_ok()
    }

    #[cfg(test)]
    pub(super) fn index_epoch_for_test(&self) -> AppResult<Uuid> {
        self.index_owner.running_epoch_for_test()
    }
}
#[cfg(test)]
mod tests;
