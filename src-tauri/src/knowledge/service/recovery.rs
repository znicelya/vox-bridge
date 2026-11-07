//! Durable local deletion continuation, including witnessed dirty sources.
use super::delete::DeleteInput;
use super::*;
use crate::knowledge::index::{ChunkMetadata, DocumentFence, Generation};
impl Core {
    pub(crate) async fn resume_deletes(&self, owner: OperationLease) -> AppResult<()> {
        let mut after = String::new();
        loop {
            let inputs = self.store.transaction(|tx| {
                let mut q = tx.prepare(
                    "SELECT d.id,j.payload_json,j.id FROM \
                    documents d JOIN intelligence_jobs j ON \
                    j.id=(SELECT id FROM intelligence_jobs WHERE \
                    kind='delete_document' AND object_id=d.id \
                    ORDER BY created_at_ms DESC LIMIT 1) WHERE \
                    d.status='deleting' AND d.id>?1 AND j.status \
                    NOT IN ('queued','running') ORDER BY d.id \
                    LIMIT 200",
                )?;
                let mut rows = q.query([&after])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    let envelope = decode_payload(state::text(r, 1, 1024 * 1024)?)?;
                    let input: DeleteInput =
                        serde_json::from_value(envelope.input).map_err(|_| error(Code::Storage))?;
                    out.push((
                        state::text(r, 0, 36)?.to_owned(),
                        state::text(r, 2, 36)?.to_owned(),
                        input,
                    ));
                }
                Ok(out)
            })?;
            if inputs.is_empty() {
                break;
            }
            for (id, job, input) in inputs {
                after = id;
                if input.validate().is_err()
                    || input.document_id.to_string() != after
                    || input.operation_id.to_string() != job
                {
                    continue;
                }
                let result = if let (Some(identity), Some(generation)) =
                    (&input.identity, input.generation)
                {
                    match self.index.recovery_evidence(identity, generation) {
                        Ok(evidence) => self.recover_delete(&input, evidence, owner.clone()).await,
                        Err(_) => self.delete(&input, owner.clone(), None).await,
                    }
                } else {
                    self.delete(&input, owner.clone(), None).await
                };
                if let Err(failure) = result {
                    let remains: bool = self.store.transaction(|tx| {
                        Ok(tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                            [input.document_id.to_string()],
                            |r| r.get(0),
                        )?)
                    })?;
                    if !remains {
                        return Err(failure);
                    }
                    if let Some(identity) = &input.identity {
                        state::mark_identity_repair(&self.store, identity, "unsealed_generation")?;
                    }
                }
            }
        }
        Ok(())
    }
    async fn recover_delete(
        &self,
        input: &DeleteInput,
        evidence: crate::knowledge::index::RecoveryEvidence,
        owner: OperationLease,
    ) -> AppResult<()> {
        input.validate()?;
        if !self
            .store
            .transaction(|tx| input.document.matches(tx, "deleting"))?
        {
            return Err(error(Code::IndexRepairRequired));
        }
        let source = evidence.source().clone();
        let count = evidence.count();
        let destination = Generation {
            generation: Uuid::new_v4(),
            ..source.clone()
        };
        let target = self.index.create(destination, owner.clone())?;
        // No source engine is opened. Only the destination uses SrvDB.
        let reader = self.index.begin_recovery(&target, evidence.clone()).await?;
        self.validate_survivors(&source, count)?;
        self.index.enqueue_open(&target)?.observe().await?;
        let mut start = 0u64;
        let mut ordinal = 0u32;
        while start < count {
            let mut selected = Vec::new();
            for id in start
                ..start
                    .checked_add(200)
                    .ok_or_else(|| error(Code::ResourceLimit))?
                    .min(count)
            {
                if let Some((metadata, fence)) = self.recovery_chunk(&source, id)? {
                    selected.push((id, metadata, fence));
                }
            }
            #[cfg(test)]
            self.checkpoint(CommitPoint::BeforeLocalCopy)?;
            for (id, vector) in self.index.copy_recovery(&reader, start, 200).await? {
                if let Some((_, metadata, fence)) = selected.iter().find(|(old, _, _)| *old == id) {
                    let added = self
                        .index
                        .add(&target, vec![(vector, metadata.clone())])
                        .await?[0];
                    self.stage_rebuild(&target, ordinal, metadata, fence, id, added)?;
                    ordinal = ordinal
                        .checked_add(1)
                        .ok_or_else(|| error(Code::ResourceLimit))?;
                }
            }
            start = start
                .checked_add(200)
                .ok_or_else(|| error(Code::ResourceLimit))?
                .min(count);
            tokio::task::yield_now().await;
        }
        self.index.verify_recovery(&reader).await?;
        let receipt = self.index.persist(&target).await?;
        #[cfg(test)]
        self.checkpoint(CommitPoint::AfterVectorPersist)?;
        let plan = PublicationPlan::Recovery {
            operation: target.operation_id(),
            work: target.operation_id(),
            token: reader.token(),
            fence: evidence.fence.clone(),
            count: ordinal,
            delete_work: input.operation_id,
            document: input.document.clone(),
        };
        let ack = self.index.publish_plan(&target, &receipt, plan).await?;
        self.index.acknowledge(&target, ack).await?;
        #[cfg(test)]
        self.checkpoint(CommitPoint::AfterPointerPublication)?;
        self.index.end_recovery(reader).await?;
        self.index.retire_recovery(&target, evidence.fence).await?;
        self.ack_delete(input.operation_id)?;
        self.remove_generation_rows(&source)?;
        crate::knowledge::documents::cleanup_managed(
            self.store.clone(),
            self.data_dir.clone(),
            owner,
        )
        .await
    }
    fn validate_survivors(&self, source: &Generation, count: u64) -> AppResult<()> {
        let mut start = 0u64;
        while start < count {
            for id in start..start.saturating_add(200).min(count) {
                self.recovery_chunk(source, id)?;
            }
            start = start
                .checked_add(200)
                .ok_or_else(|| error(Code::ResourceLimit))?
                .min(count);
        }
        // Current Ready mappings outside the certified half-open geometry can
        // never become survivors. Attribute them through the inverse mapping.
        let mut after = String::new();
        loop {
            let rows = self.store.transaction(|tx| {
                Ok(tx
                    .prepare(
                        "SELECT m.chunk_id,m.document_id,m.vector_id,\
                    m.document_version FROM \
                    knowledge_generation_mappings m JOIN \
                    documents d ON d.id=m.document_id WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.published=1 AND m.chunk_id>?3 AND \
                    d.version=m.document_version AND \
                    d.status='ready' ORDER BY m.chunk_id LIMIT \
                    200",
                    )?
                    .query_map(
                        params![source.key()?, source.generation.to_string(), after],
                        |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                                r.get::<_, u32>(3)?,
                            ))
                        },
                    )?
                    .collect::<Result<Vec<_>, _>>()?)
            })?;
            if rows.is_empty() {
                break;
            }
            for (chunk, doc, vector, version) in rows {
                after = chunk;
                if vector.parse::<u64>().map_err(|_| error(Code::Storage))? >= count {
                    state::mark_document_repair(
                        &self.store,
                        Uuid::parse_str(&doc).map_err(|_| error(Code::Storage))?,
                        version,
                        "vector_missing",
                    )?;
                }
            }
        }
        Ok(())
    }
    fn recovery_chunk(
        &self,
        source: &Generation,
        vector: u64,
    ) -> AppResult<Option<(ChunkMetadata, DocumentFence)>> {
        self.store.transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT m.document_id,m.document_version,\
                    m.chunk_id,m.content_hash,d.version,d.status,\
                    c.text,c.content_hash,c.location_json,\
                    c.vector_id,c.embedding_identity,\
                    d.index_identity,c.index_state,c.document_id,\
                    c.document_version FROM \
                    knowledge_generation_mappings m LEFT JOIN \
                    document_chunks c ON c.id=m.chunk_id LEFT \
                    JOIN documents d ON d.id=m.document_id WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.vector_id=?3 AND m.published=1",
            )?;
            let key = source.key()?;
            let mut rows = q.query(params![
                key,
                source.generation.to_string(),
                vector.to_string()
            ])?;
            let Some(r) = rows.next()? else {
                return Ok(None);
            };
            let doc = Uuid::parse_str(state::text(r, 0, 36)?).map_err(|_| error(Code::Storage))?;
            let version: u32 = r.get(1)?;
            let status: Option<String> = r.get(5)?;
            if r.get::<_, Option<u32>>(4)? != Some(version) {
                return Ok(None);
            }
            let quarantined: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_repairs WHERE document_id=?1 AND \
                    reason='unsealed_generation')",
                [doc.to_string()],
                |r| r.get(0),
            )?;
            if status.as_deref() != Some("ready")
                && (status.as_deref() != Some("repair_required") || !quarantined)
            {
                return Ok(None);
            }
            let parsed = (|| {
                let location: TextLocation =
                    serde_json::from_str(state::text(r, 8, 16 * 1024 * 1024)?)
                        .map_err(|_| error(Code::IndexRepairRequired))?;
                let body = state::text(r, 6, 9600)?;
                let hash = state::text(r, 3, 64)?;
                if body.chars().count() > 2400
                    || crate::knowledge::chunk_hash(body) != hash
                    || state::text(r, 7, 64)? != hash
                    || r.get::<_, Option<String>>(9)?.as_deref() != Some(&vector.to_string())
                    || r.get::<_, Option<String>>(10)?.as_deref() != Some(&key)
                    || r.get::<_, Option<String>>(11)?.as_deref() != Some(&key)
                    || r.get::<_, Option<String>>(12)?.as_deref() != Some("ready")
                    || r.get::<_, Option<String>>(13)?.as_deref() != Some(&doc.to_string())
                    || r.get::<_, Option<u32>>(14)? != Some(version)
                {
                    return Err(error(Code::IndexRepairRequired));
                }
                Ok((
                    ChunkMetadata::new(
                        doc,
                        version,
                        Uuid::parse_str(state::text(r, 2, 36)?)
                            .map_err(|_| error(Code::Storage))?,
                        hash.into(),
                        &location,
                    )?,
                    state::fence(tx, doc)?,
                ))
            })();
            match parsed {
                Ok(found) => Ok(Some(found)),
                Err(_) => {
                    tx.execute(
                        "UPDATE documents SET \
                    status='repair_required',\
                    error_code='index_repair_required' WHERE \
                    id=?1 AND version=?2 AND status IN ('ready',\
                    'repair_required')",
                        params![doc.to_string(), version],
                    )?;
                    tx.execute(
                        "UPDATE document_chunks SET \
                    index_state='repair_required' WHERE \
                    document_id=?1 AND document_version=?2",
                        params![doc.to_string(), version],
                    )?;
                    tx.execute(
                        "INSERT INTO knowledge_repairs(document_id,\
                    reason) VALUES (?1,'stale_hash') ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason='stale_hash'",
                        [doc.to_string()],
                    )?;
                    Ok(None)
                }
            }
        })
    }
}
