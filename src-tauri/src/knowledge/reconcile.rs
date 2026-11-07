//! Local repair and generation replacement never call an embedding provider.
use super::{
    embedding::error,
    index::{Generation, PublicationPlan, Seal, WorkTicket},
    service::{Core, state},
};
use crate::{
    error::AppResult,
    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode as Code},
};
use rusqlite::params;
use uuid::Uuid;
impl Core {
    pub(crate) async fn reconcile(&self, owner: OperationLease, rebuild: bool) -> AppResult<()> {
        self.resume_deletes(owner.clone()).await?;
        self.store.transaction(|tx| {
            tx.execute(
                "UPDATE documents SET \
                    status='repair_required',\
                    error_code='index_repair_required' WHERE \
                    status='indexing' AND NOT EXISTS(SELECT 1 \
                    FROM intelligence_jobs j WHERE \
                    j.kind='reindex' AND \
                    j.object_id=documents.id AND j.status IN \
                    ('queued','running') AND \
                    json_extract(j.payload_json,\
                    '$.input.document_version')=documents.version)",
                [],
            )?;
            tx.execute(
                "INSERT INTO knowledge_repairs(document_id,\
                    reason) SELECT id,'unsealed_generation' FROM \
                    documents WHERE status='repair_required' ON \
                    CONFLICT(document_id) DO NOTHING",
                [],
            )?;
            Ok(())
        })?;
        let mut after = String::new();
        loop {
            let rows = self.store.transaction(|tx| {
                Ok(tx
                    .prepare(
                        "SELECT identity,active_generation FROM \
                    knowledge_index_generations WHERE \
                    identity>?1 AND active_generation IS NOT \
                    NULL ORDER BY identity LIMIT 200",
                    )?
                    .query_map([&after], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?)
            })?;
            if rows.is_empty() {
                break;
            }
            for (identity, raw) in rows {
                after = identity.clone();
                let generation = Uuid::parse_str(&raw).map_err(|_| error(Code::Storage))?;
                let descriptor = self.store.transaction(|tx| {
                    let mut q = tx.prepare(
                        "SELECT seal_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2 AND dirty=0",
                    )?;
                    let mut rows = q.query(params![identity, raw])?;
                    let Some(r) = rows.next()? else {
                        return Ok(None);
                    };
                    let seal = Seal::decode(state::text(r, 0, 65536)?)?;
                    if seal.descriptor.key()? != identity
                        || seal.descriptor.generation != generation
                    {
                        return Err(error(Code::IndexRepairRequired));
                    }
                    Ok(Some(seal.descriptor))
                });
                let descriptor = match descriptor {
                    Ok(Some(g)) => g,
                    _ => {
                        state::mark_identity_repair(&self.store, &identity, "unsealed_generation")?;
                        continue;
                    }
                };
                let ticket = match self.index.open_existing(descriptor.clone(), owner.clone()) {
                    Ok(t) => t,
                    Err(_) => {
                        state::mark_identity_repair(&self.store, &identity, "unsealed_generation")?;
                        continue;
                    }
                };
                let count = match self.index.enqueue_open(&ticket)?.observe().await {
                    Ok(c) => c,
                    Err(_) => {
                        state::mark_identity_repair(&self.store, &identity, "index_missing")?;
                        continue;
                    }
                };
                self.check_inverse(&identity, generation, count)?;
                let mut start = 0u64;
                while start < count {
                    let page = match self.index.scan_records(&ticket, start, 200).await {
                        Ok(p) => p,
                        Err(_) => {
                            state::mark_identity_repair(&self.store, &identity, "vector_missing")?;
                            break;
                        }
                    };
                    let mut orphans = Vec::new();
                    for record in page {
                        let (id, metadata) = (record.id, record.metadata);
                        match state::classify(
                            &self.store,
                            &identity,
                            generation,
                            id,
                            metadata.as_ref(),
                        )? {
                            state::Record::Healthy(_, _) => {}
                            state::Record::Tombstone if record.deleted => {}
                            state::Record::Tombstone | state::Record::Orphan => {
                                self.store.transaction(|tx| {
                                    tx.execute(
                                        "INSERT OR IGNORE INTO \
                    knowledge_index_work_items(operation_id,\
                    ordinal,identity,generation,vector_id) \
                    VALUES (?1,?2,?3,?4,?5)",
                                        params![
                                            ticket.operation_id().to_string(),
                                            id,
                                            identity,
                                            generation.to_string(),
                                            id.to_string()
                                        ],
                                    )?;
                                    Ok(())
                                })?;
                                orphans.push(id);
                            }
                            state::Record::Repair(doc, version, reason) => {
                                state::mark_document_repair(&self.store, doc, version, reason)?
                            }
                        }
                    }
                    if !orphans.is_empty() {
                        self.index.delete(&ticket, orphans).await?;
                    }
                    start = start
                        .checked_add(200)
                        .ok_or_else(|| error(Code::ResourceLimit))?
                        .min(count);
                    tokio::task::yield_now().await;
                }
                let receipt = self.index.persist(&ticket).await?;
                let ack = self
                    .index
                    .publish_plan(
                        &ticket,
                        &receipt,
                        PublicationPlan::Cleanup {
                            operation: ticket.operation_id(),
                            work: ticket.operation_id(),
                        },
                    )
                    .await?;
                self.index.acknowledge(&ticket, ack).await?;
                self.cleanup_generations(&ticket).await?;
                if rebuild || self.rebuild_needed(&descriptor, 0)? {
                    self.rebuild_generation(descriptor, owner.clone()).await?;
                }
            }
        }
        crate::knowledge::documents::cleanup_managed(
            self.store.clone(),
            self.data_dir.clone(),
            owner,
        )
        .await
    }
    fn check_inverse(&self, identity: &str, generation: Uuid, count: u64) -> AppResult<()> {
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
                    d.status='ready' AND \
                    d.version=m.document_version ORDER BY \
                    m.chunk_id LIMIT 200",
                    )?
                    .query_map(params![identity, generation.to_string(), after], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, u32>(3)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?)
            })?;
            if rows.is_empty() {
                return Ok(());
            }
            for (chunk, doc, raw, version) in rows {
                after = chunk;
                let vector = raw.parse::<u64>().map_err(|_| error(Code::Storage))?;
                if vector >= count {
                    state::mark_document_repair(
                        &self.store,
                        Uuid::parse_str(&doc).map_err(|_| error(Code::Storage))?,
                        version,
                        "vector_missing",
                    )?;
                }
            }
        }
    }
    pub(crate) fn rebuild_needed(&self, generation: &Generation, pending: u64) -> AppResult<bool> {
        self.store.transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT seal_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2 AND dirty=0",
            )?;
            let key = generation.key()?;
            let mut rows = q.query(params![key, generation.generation.to_string()])?;
            let r = rows
                .next()?
                .ok_or_else(|| error(Code::IndexRepairRequired))?;
            let seal = Seal::decode(state::text(r, 0, 65536)?)?;
            let live: u64 = tx.query_row(
                "SELECT count(*) FROM \
                    knowledge_generation_mappings m JOIN \
                    documents d ON d.id=m.document_id AND \
                    d.version=m.document_version JOIN \
                    document_chunks c ON c.id=m.chunk_id AND \
                    c.document_id=d.id AND \
                    c.document_version=d.version AND \
                    c.content_hash=m.content_hash AND \
                    c.vector_id=m.vector_id WHERE m.identity=?1 \
                    AND m.generation=?2 AND m.published=1 AND \
                    d.status='ready' AND d.index_identity=?1 AND \
                    c.index_state='ready' AND \
                    c.embedding_identity=?1",
                params![key, generation.generation.to_string()],
                |r| r.get(0),
            )?;
            seal.needs_rebuild(live, pending)
        })
    }
    pub(crate) fn ensure_capacity(&self, generation: &Generation, pending: u64) -> AppResult<()> {
        self.store.transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT s.seal_json FROM \
                    knowledge_index_generation_seals s JOIN \
                    knowledge_index_generations g ON \
                    g.identity=s.identity AND \
                    g.active_generation=s.generation WHERE \
                    s.identity=?1 AND s.generation=?2 AND \
                    s.dirty=0",
            )?;
            let mut rows = q.query(params![
                generation.key()?,
                generation.generation.to_string()
            ])?;
            let row = rows
                .next()?
                .ok_or_else(|| error(Code::IndexRepairRequired))?;
            let seal = Seal::decode(state::text(row, 0, 65536)?)?;
            if seal.descriptor != *generation {
                return Err(error(Code::IndexRepairRequired));
            }
            seal.ensure_capacity(pending)
        })
    }
    pub(crate) async fn rebuild_generation(
        &self,
        source: Generation,
        owner: OperationLease,
    ) -> AppResult<()> {
        let source_ticket = self.index.open_existing(source.clone(), owner.clone())?;
        let count = self.index.enqueue_open(&source_ticket)?.observe().await?;
        let source_receipt = self.index.persist(&source_ticket).await?;
        let publication = async {
            let maintenance = self.index.begin_maintenance(&source_ticket).await?;
            let copied = async {
                let staging = Generation {
                    generation: Uuid::new_v4(),
                    ..source.clone()
                };
                let target = self.index.create(staging.clone(), owner.clone())?;
                self.index.enqueue_open(&target)?.observe().await?;
                let key = source.key()?;
                let mut start = 0u64;
                let mut ordinal = 0u32;
                while start < count {
                    let metadata = self.index.scan_records(&source_ticket, start, 200).await?;
                    let mut selected = Vec::new();
                    for record in metadata {
                        if let state::Record::Healthy(meta, fence) = state::classify(
                            &self.store,
                            &key,
                            source.generation,
                            record.id,
                            record.metadata.as_ref(),
                        )? {
                            selected.push((record.id, meta, fence));
                        }
                    }
                    #[cfg(test)]
                    self.checkpoint(super::service::CommitPoint::BeforeLocalCopy)?;
                    for (id, vector) in self.index.copy_vectors(&maintenance, start, 200).await? {
                        if let Some((_, meta, fence)) =
                            selected.iter().find(|(old, _, _)| *old == id)
                        {
                            let new = self
                                .index
                                .add(&target, vec![(vector, meta.clone())])
                                .await?[0];
                            self.stage_rebuild(&target, ordinal, meta, fence, id, new)?;
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
                Ok::<_, crate::error::AppError>((target, ordinal))
            }
            .await;
            let ended = self.index.end_maintenance(maintenance).await;
            ended?;
            let (target, ordinal) = copied?;
            let receipt = self.index.persist(&target).await?;
            #[cfg(test)]
            self.checkpoint(super::service::CommitPoint::AfterVectorPersist)?;
            let ack = self
                .index
                .publish_plan(
                    &target,
                    &receipt,
                    PublicationPlan::Swap {
                        operation: target.operation_id(),
                        work: target.operation_id(),
                        source: source.generation,
                        count: ordinal,
                    },
                )
                .await?;
            self.index.acknowledge(&target, ack).await?;
            Ok::<_, crate::error::AppError>(target)
        }
        .await;
        // The source was read only. Complete this exact refresh even when the
        // destination fails, before returning its error or retiring the source.
        let ack = self.index.publish(&source_ticket, &source_receipt).await?;
        self.index.acknowledge(&source_ticket, ack).await?;
        let target = publication?;
        self.index.retire(&source_ticket).await?;
        self.index
            .remove_generation(&target, source.clone())
            .await?;
        self.remove_generation_rows(&source)
    }
    pub(crate) fn stage_rebuild(
        &self,
        ticket: &WorkTicket,
        ordinal: u32,
        meta: &super::index::ChunkMetadata,
        fence: &super::index::DocumentFence,
        previous: u64,
        vector: u64,
    ) -> AppResult<()> {
        let key = ticket.generation().key()?;
        let generation = ticket.generation().generation.to_string();
        fence.validate()?;
        if fence.id != meta.document_id || fence.version != meta.document_version {
            return Err(error(Code::IndexRepairRequired));
        }
        self.store.transaction(|tx| {
            tx.execute(
                "INSERT INTO \
                    knowledge_index_work_items(operation_id,\
                    ordinal,identity,generation,vector_id,\
                    document_id,document_version,chunk_id,\
                    content_hash,document_fence_json,\
                    location_sha256,previous_vector_id) VALUES \
                    (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    ticket.operation_id().to_string(),
                    ordinal,
                    key,
                    generation,
                    vector.to_string(),
                    meta.document_id.to_string(),
                    meta.document_version,
                    meta.chunk_id.to_string(),
                    meta.content_hash,
                    serde_json::to_string(fence).map_err(|_| error(Code::Storage))?,
                    meta.location_sha256(),
                    previous.to_string()
                ],
            )?;
            tx.execute(
                "INSERT INTO \
                    knowledge_generation_mappings(identity,\
                    generation,chunk_id,document_id,\
                    document_version,vector_id,content_hash,\
                    published) VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
                params![
                    key,
                    generation,
                    meta.chunk_id.to_string(),
                    meta.document_id.to_string(),
                    meta.document_version,
                    vector.to_string(),
                    meta.content_hash
                ],
            )?;
            Ok(())
        })
    }
    async fn cleanup_generations(&self, target: &WorkTicket) -> AppResult<()> {
        let key = target.generation().key()?;
        let active = target.generation().generation.to_string();
        let mut after = String::new();
        loop {
            let rows = self.store.transaction(|tx| {
                let mut q = tx.prepare(
                    "SELECT generation,seal_json,intent_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation!=?2 AND \
                    generation>?3 ORDER BY generation LIMIT 200",
                )?;
                let mut rows = q.query(params![key, active, after])?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    let source = match r.get_ref(1)? {
                        rusqlite::types::ValueRef::Null => {
                            crate::knowledge::index::recovery_descriptor(state::text(r, 2, 16384)?)?
                        }
                        _ => Seal::decode(state::text(r, 1, 65536)?)?.descriptor,
                    };
                    if source.key()? != key
                        || source.generation.to_string() != state::text(r, 0, 36)?
                    {
                        return Err(error(Code::IndexRepairRequired));
                    }
                    out.push(source);
                }
                Ok(out)
            })?;
            if rows.is_empty() {
                break;
            }
            for source in rows {
                after = source.generation.to_string();
                if self
                    .index
                    .remove_generation(target, source.clone())
                    .await
                    .is_ok()
                {
                    // The index owner has actually dropped every reader/engine and
                    // removed the unreferenced managed bytes before acknowledging work.
                    loop {
                        let removed = self.store.transaction(|tx| {
                            let ids = tx
                                .prepare(
                                    "SELECT operation_id,ordinal FROM \
                    knowledge_index_work_items WHERE identity=?1 \
                    AND generation=?2 LIMIT 200",
                                )?
                                .query_map(params![key, source.generation.to_string()], |r| {
                                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                                })?
                                .collect::<Result<Vec<_>, _>>()?;
                            for (op, ordinal) in &ids {
                                tx.execute(
                                    "DELETE FROM knowledge_index_work_items WHERE \
                    operation_id=?1 AND ordinal=?2",
                                    params![op, ordinal],
                                )?;
                            }
                            Ok(ids.len())
                        })?;
                        if removed == 0 {
                            break;
                        }
                    }
                    self.remove_generation_rows(&source)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn remove_generation_rows(&self, source: &Generation) -> AppResult<()> {
        let key = source.key()?;
        let generation = source.generation.to_string();
        loop {
            let removed = self.store.transaction(|tx| {
                let ids = tx
                    .prepare(
                        "SELECT chunk_id FROM \
                    knowledge_generation_mappings WHERE \
                    identity=?1 AND generation=?2 LIMIT 200",
                    )?
                    .query_map(params![key, generation], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                for id in &ids {
                    tx.execute(
                        "DELETE FROM knowledge_generation_mappings \
                    WHERE identity=?1 AND generation=?2 AND \
                    chunk_id=?3",
                        params![key, generation, id],
                    )?;
                }
                Ok(ids.len())
            })?;
            if removed == 0 {
                break;
            }
        }
        self.store.transaction(|tx| {
            tx.execute(
                "DELETE FROM knowledge_index_generation_seals \
                    WHERE identity=?1 AND generation=?2 AND NOT \
                    EXISTS(SELECT 1 FROM \
                    knowledge_index_generations WHERE \
                    identity=?1 AND active_generation=?2)",
                params![key, generation],
            )?;
            Ok(())
        })
    }
}
