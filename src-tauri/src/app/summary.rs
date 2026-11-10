use super::AppState;
use crate::{
    error::{AppError, AppResult},
    intelligence::types::{IntelligenceErrorCode, JobView},
    summary::types::{SummaryCursor, SummaryPage},
};
use uuid::Uuid;
impl AppState {
    pub(super) fn summary_readiness(
        &self,
        epoch: u64,
    ) -> AppResult<crate::summary::SummaryDeliveryReady> {
        let ready = {
            self.summary_ready
                .lock()
                .map_err(|_| AppError::Intelligence(IntelligenceErrorCode::Storage))?
                .clone()
                .ok_or(AppError::Intelligence(IntelligenceErrorCode::Busy))?
        };
        ready.require_epoch(epoch)?;
        Ok(ready)
    }
    pub async fn generate_summary(
        &self,
        session_id: Uuid,
        profile_id: Option<Uuid>,
    ) -> AppResult<JobView> {
        let epoch = self.admitted_epoch()?;
        let ready = self.summary_readiness(epoch)?;
        self.summaries
            .generate_owned(session_id, profile_id, self.intelligence_lease()?, ready)
            .await
    }
    pub fn list_summaries(
        &self,
        session_id: Uuid,
        after_version: Option<SummaryCursor>,
        limit: Option<u32>,
    ) -> AppResult<SummaryPage> {
        self.summaries
            .list(session_id, after_version, limit.unwrap_or(50))
    }
    pub async fn delete_summary(&self, summary_id: Uuid) -> AppResult<()> {
        let epoch = self.admitted_epoch()?;
        let ready = self.summary_readiness(epoch)?;
        self.summaries
            .delete_owned(summary_id, self.intelligence_lease()?, ready)
            .await
    }
}
