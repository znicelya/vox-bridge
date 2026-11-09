use super::AppState;
use crate::{
    error::AppResult,
    intelligence::types::JobView,
    summary::types::{SummaryCursor, SummaryPage},
};
use uuid::Uuid;
impl AppState {
    pub async fn generate_summary(
        &self,
        session_id: Uuid,
        profile_id: Option<Uuid>,
    ) -> AppResult<JobView> {
        let epoch = self.admitted_epoch()?;
        self.summaries.require_epoch(epoch)?;
        self.summaries
            .generate_owned(session_id, profile_id, self.intelligence_lease()?)
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
        self.summaries.require_epoch(epoch)?;
        self.summaries
            .delete_owned(summary_id, self.intelligence_lease()?)
            .await
    }
}
