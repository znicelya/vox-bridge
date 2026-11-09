//! Joined job execution. Each domain effect uses fresh SQL authority.
use super::{
    SummaryCore,
    content::SummaryContent,
    job_proof::SummaryJobProof,
    planner,
    snapshot::FinalSnapshot,
    store::{self, error, storage},
};
use crate::{
    error::{AppError, AppResult},
    intelligence::{
        jobs::{JobContext, JobHandler},
        types::*,
    },
    llm::LlmExecution,
};
use std::sync::Arc;
use uuid::Uuid;
pub(crate) struct SummaryHandler(pub(crate) Arc<SummaryCore>);
fn code(error: AppError) -> IntelligenceErrorCode {
    match error {
        AppError::Intelligence(code) => code,
        _ => IntelligenceErrorCode::Storage,
    }
}
#[async_trait::async_trait]
impl JobHandler for SummaryHandler {
    async fn run(
        &self,
        context: JobContext,
        payload: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        self.run_owned(context, payload).await.map_err(code)
    }
    async fn cleanup_after_stop(&self) -> AppResult<()> {
        self.0.attempts.drain().await
    }
    fn terminal_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        id: Uuid,
        status: JobStatus,
        error_code: Option<IntelligenceErrorCode>,
        _awaiting_reap: bool,
    ) -> AppResult<()> {
        let Some(proof) = SummaryJobProof::terminal_in_tx(tx, id, status, error_code)? else {
            return Ok(());
        };
        if tx.execute("UPDATE session_summaries SET status=?2,error_code=?3 WHERE id=?1 AND job_id=?4 AND origin='native'",
            rusqlite::params![proof.summary_id().to_string(),serde_json::to_value(status).map_err(|_|storage())?.as_str(),error_code.map(IntelligenceErrorCode::as_str),id.to_string()])?!=1 {return Err(storage());}
        proof.require_in_tx(tx)
    }
}
impl SummaryHandler {
    async fn run_owned(&self, context: JobContext, payload: serde_json::Value) -> AppResult<()> {
        let owner = context.operation_owner()?;
        let (proof,attempt)=self.0.store.transaction(|tx|context.commit(tx,|| {
            let proof=SummaryJobProof::claim_in_tx(tx,context.id,&payload)?;
            proof.running_non_deleting_in_tx(tx)?;
            let attempt=self.0.attempts.register(context.id,owner)?;
            if tx.execute("UPDATE session_summaries SET status='running' WHERE id=?1 AND job_id=?2 AND origin='native' AND deleting=0",rusqlite::params![proof.summary_id().to_string(),context.id.to_string()])?!=1 {return Err(storage());}
            proof.require_in_tx(tx)?;Ok((proof,attempt))
        })).map_err(crate::intelligence::store::safe_store_error)?;
        let snapshot = FinalSnapshot::capture(
            self.0.store.clone(),
            proof.clone(),
            attempt.clone(),
            context.cancel.clone(),
        )
        .await?;
        self.0.store.transaction(|tx|context.commit(tx,|| {
            proof.running_non_deleting_in_tx(tx)?;
            if tx.execute("UPDATE session_summaries SET input_hash=?2,input_final_count=?3,snapshot_last_rowid=?4 WHERE id=?1 AND status='running' AND deleting=0",rusqlite::params![proof.summary_id().to_string(),snapshot.input_hash,snapshot.input_final_count,snapshot.last_rowid])?!=1 {return Err(storage());}
            proof.require_in_tx(tx)?;Ok(())
        })).map_err(crate::intelligence::store::safe_store_error)?;
        if snapshot.input_final_count == 0 {
            return Err(error(IntelligenceErrorCode::NoFinalText));
        }
        let profile = proof
            .profile()
            .cloned()
            .ok_or_else(|| error(IntelligenceErrorCode::Configuration))?;
        let maps = planner::plan_maps(&snapshot, &profile).map_err(error)?;
        let mut counters = maps.counters;
        let mut frontier = Vec::with_capacity(maps.requests.len());
        for plan in &maps.requests {
            let request =
                planner::render_map(&snapshot, &maps.manifest, plan, &profile).map_err(error)?;
            self.0
                .store
                .transaction(|tx| context.commit(tx, || proof.running_non_deleting_in_tx(tx)))
                .map_err(crate::intelligence::store::safe_store_error)?;
            let response = self
                .0
                .llm
                .complete(
                    &profile,
                    request,
                    LlmExecution::new(attempt.clone(), context.cancel.clone()),
                )
                .await
                .map_err(error)?;
            self.0
                .store
                .transaction(|tx| {
                    context.commit(tx, || {
                        store::retain_response_in_tx(tx, &proof, &maps.manifest, plan, &response)
                    })
                })
                .map_err(crate::intelligence::store::safe_store_error)?;
            let content = SummaryContent::parse(&response.content).map_err(error)?;
            frontier.push(planner::SummaryNode {
                content,
                coverage: plan.coverage.clone(),
            });
        }
        while frontier.len() > 1 {
            let groups = planner::plan_reduce(&frontier, &profile, &mut counters).map_err(error)?;
            let mut next = Vec::with_capacity(groups.len());
            // Render complete children while the old frontier remains immutable;
            // singleton carry moves its content after all level calls finish.
            let mut reduced = Vec::with_capacity(groups.len());
            for group in &groups {
                let Some(plan) = &group.request else {
                    reduced.push(None);
                    continue;
                };
                let request = planner::render_reduce(&frontier, plan, &profile).map_err(error)?;
                self.0
                    .store
                    .transaction(|tx| context.commit(tx, || proof.running_non_deleting_in_tx(tx)))
                    .map_err(crate::intelligence::store::safe_store_error)?;
                let response = self
                    .0
                    .llm
                    .complete(
                        &profile,
                        request,
                        LlmExecution::new(attempt.clone(), context.cancel.clone()),
                    )
                    .await
                    .map_err(error)?;
                self.0
                    .store
                    .transaction(|tx| {
                        context.commit(tx, || {
                            store::retain_response_in_tx(
                                tx,
                                &proof,
                                &maps.manifest,
                                plan,
                                &response,
                            )
                        })
                    })
                    .map_err(crate::intelligence::store::safe_store_error)?;
                reduced.push(Some(
                    SummaryContent::parse(&response.content).map_err(error)?,
                ));
            }
            let mut old = frontier.into_iter();
            for (group, content) in groups.into_iter().zip(reduced) {
                if let Some(content) = content {
                    for _ in group.input_range {
                        old.next().ok_or_else(storage)?;
                    }
                    next.push(planner::SummaryNode {
                        content,
                        coverage: group.coverage,
                    });
                } else {
                    next.push(old.next().ok_or_else(storage)?);
                }
            }
            if old.next().is_some() {
                return Err(storage());
            }
            frontier = next;
        }
        let root = frontier.pop().ok_or_else(storage)?;
        self.0
            .store
            .transaction(|tx| {
                context.commit(tx, || {
                    store::publish_root_in_tx(tx, &proof, &maps.manifest, &root)
                })
            })
            .map_err(crate::intelligence::store::safe_store_error)
    }
}
