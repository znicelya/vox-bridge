use super::*;
use crate::knowledge::{
    embedding::request_body,
    identity::EmbeddingIdentity,
    index::{Generation, ReadyPublication},
};
impl Core {
    pub(super) async fn reindex(
        &self,
        context: JobContext,
        input: IndexInput,
        profile: &ModelProfile,
    ) -> AppResult<()> {
        let owner = context.operation_owner()?;
        let _serial = self.serial.lock().await;
        let (proof, admitted_document) = self.store.transaction(|tx| {
            context.commit(tx, || {
                let job = job_proof::reindex(tx, context.id)?
                    .ok_or_else(|| error(Code::Configuration))?;
                if job.input != input || job.profile != *profile {
                    return Err(error(Code::Configuration));
                }
                let document = state::fence(tx, input.document_id)?;
                let status: String = tx.query_row(
                    "SELECT status FROM documents WHERE id=?1",
                    [input.document_id.to_string()],
                    |r| r.get(0),
                )?;
                if document.version != input.document_version
                    || matches!(status.as_str(), "parsing" | "deleting")
                {
                    return Err(error(Code::Cancelled));
                }
                Ok((job.proof, document))
            })
        })?;
        let pending: u64 = self.store.transaction(|tx| {
            Ok(tx.query_row(
                "SELECT count(*) FROM document_chunks WHERE \
                    document_id=?1 AND document_version=?2",
                params![input.document_id.to_string(), input.document_version],
                |r| r.get(0),
            )?)
        })?;
        if pending > 10000 {
            return Err(error(Code::ResourceLimit));
        }
        if let Some(binding) = state::binding(&self.store, profile)? {
            if self.rebuild_needed(&binding.generation, pending)? {
                self.rebuild_generation(binding.generation, owner.clone())
                    .await?;
            }
            let current = state::binding(&self.store, profile)?
                .ok_or_else(|| error(Code::IndexRepairRequired))?;
            self.ensure_capacity(&current.generation, pending)?;
        }
        let fence = self.store.transaction(|tx| {
            context.commit(tx, || {
                proof.require(tx, &admitted_document)?;
                let f = state::fence(tx, input.document_id)?;
                if f != admitted_document {
                    return Err(error(Code::Cancelled));
                }
                let status: String = tx.query_row(
                    "SELECT status FROM documents WHERE id=?1",
                    [f.id.to_string()],
                    |r| r.get(0),
                )?;
                if matches!(status.as_str(), "parsing" | "deleting") {
                    return Err(error(Code::Cancelled));
                }
                tx.execute(
                    "UPDATE documents SET status='indexing',\
                    error_code='index_repair_required' WHERE \
                    id=?1",
                    [f.id.to_string()],
                )?;
                tx.execute(
                    "UPDATE document_chunks SET \
                    index_state='indexing' WHERE document_id=?1 \
                    AND document_version=?2",
                    params![f.id.to_string(), f.version],
                )?;
                tx.execute(
                    "INSERT INTO knowledge_repairs(document_id,\
                    reason) VALUES (?1,'unsealed_generation') ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
                    [f.id.to_string()],
                )?;
                Ok(f)
            })
        })?;
        #[cfg(test)]
        self.checkpoint(CommitPoint::AfterSqliteIntent)?;
        let existing = state::binding(&self.store, profile)?;
        let mut ticket = None;
        let mut expected = None;
        let mut dimension = None;
        let mut after = -1i64;
        let mut staged = 0u32;
        loop {
            let page = state::chunks(&self.store, &fence, after)?;
            if page.is_empty() {
                break;
            }
            let mut cursor = 0;
            while cursor < page.len() {
                let mut inputs = Vec::new();
                let start = cursor;
                while cursor < page.len() {
                    inputs.push(page[cursor].text.clone());
                    if request_body(profile, &inputs).is_err() {
                        inputs.pop();
                        break;
                    }
                    cursor += 1;
                }
                if inputs.is_empty() {
                    return Err(error(Code::ResourceLimit));
                }
                self.store.transaction(|tx| {
                    context.commit(tx, || {
                        proof.require(tx, &fence)?;
                        if !fence.matches(tx, "indexing")? {
                            return Err(error(Code::Cancelled));
                        }
                        Ok(())
                    })
                })?;
                let vectors = self
                    .embeddings
                    .embed(
                        profile,
                        &inputs,
                        EmbeddingExecution::new(owner.clone(), context.cancel.clone()),
                    )
                    .await?;
                if vectors.len() != inputs.len() {
                    return Err(error(Code::InvalidResponse));
                }
                let actual = vectors
                    .first()
                    .map(Vec::len)
                    .ok_or_else(|| error(Code::InvalidResponse))?;
                if dimension.is_some_and(|d| d != actual)
                    || vectors.iter().any(|v| v.len() != actual)
                {
                    return Err(error(Code::InvalidResponse));
                }
                dimension = Some(actual);
                let identity = EmbeddingIdentity::from_profile(
                    profile,
                    u32::try_from(actual).map_err(|_| error(Code::InvalidResponse))?,
                )?;
                if ticket.is_none() {
                    let key = identity.key()?;
                    expected = state::active_pointer(&self.store, &key)?;
                    let mut chosen = None;
                    if let Some(binding) = &existing {
                        if binding.generation.identity != identity {
                            return Err(error(Code::IndexRepairRequired));
                        }
                        if let Ok(t) = self
                            .index
                            .open_existing(binding.generation.clone(), owner.clone())
                        {
                            if self.index.enqueue_open(&t)?.observe().await.is_ok() {
                                chosen = Some(t);
                            }
                        }
                    }
                    if chosen.is_none() {
                        let t = self.index.create(
                            Generation {
                                identity,
                                requested_dimension: profile.dimensions,
                                generation: Uuid::new_v4(),
                            },
                            owner.clone(),
                        )?;
                        self.index.enqueue_open(&t)?.observe().await?;
                        chosen = Some(t);
                    }
                    ticket = chosen;
                }
                let t = ticket.as_ref().unwrap();
                let key = t.generation().key()?;
                for (chunk, vector) in page[start..cursor].iter().zip(vectors) {
                    self.store.transaction(|tx| {
                        context.commit(tx, || {
                            proof.require(tx, &fence)?;
                            if !fence.matches(tx, "indexing")? {
                                return Err(error(Code::Cancelled));
                            }
                            Ok(())
                        })
                    })?;
                    if *context.cancel.borrow() {
                        return Err(error(Code::Cancelled));
                    }
                    let id = if let Some((previous, id)) = &chunk.previous {
                        if previous == &key && expected == Some(t.generation().generation) {
                            self.index
                                .replace(t, *id, vector, chunk.metadata.clone())
                                .await?
                        } else {
                            self.index
                                .add(t, vec![(vector, chunk.metadata.clone())])
                                .await?[0]
                        }
                    } else {
                        self.index
                            .add(t, vec![(vector, chunk.metadata.clone())])
                            .await?[0]
                    };
                    #[cfg(test)]
                    self.checkpoint(CommitPoint::AfterVectorMutation)?;
                    state::stage(&self.store, &context, t, staged, chunk, id)?;
                    staged = staged
                        .checked_add(1)
                        .ok_or_else(|| error(Code::ResourceLimit))?;
                    after = i64::from(chunk.ordinal);
                }
            }
        }
        let ticket = ticket.ok_or_else(|| error(Code::NoFinalText))?;
        let receipt = self.index.persist(&ticket).await?;
        #[cfg(test)]
        self.checkpoint(CommitPoint::AfterVectorPersist)?;
        #[cfg(test)]
        self.checkpoint(CommitPoint::BeforeReady)?;
        let plan = PublicationPlan::Ready(ReadyPublication {
            operation: ticket.operation_id(),
            work: ticket.operation_id(),
            document: fence,
            expected_generation: expected,
            count: staged,
            job: context.publication_fence(),
            job_proof: proof,
            cleanup_directory: crate::knowledge::index::delete_completion::resolve_directory(
                self.data_dir.clone(),
                owner.clone(),
            )
            .await?,
        });
        let ack = self.index.publish_plan(&ticket, &receipt, plan).await?;
        self.index.acknowledge(&ticket, ack).await?;
        self.resume_completions(owner).await
    }
}
