//! Compact plans reference durable rows, never a whole-document vector array.
use super::{
    repair,
    seal::{Generation, IntentKind, Seal},
};
use crate::{
    error::AppResult,
    intelligence::jobs::{JobPublicationFence, now_ms},
};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentFence {
    #[serde(deserialize_with = "super::codec::canonical_uuid")]
    pub(crate) id: Uuid,
    pub(crate) version: u32,
    pub(crate) sha256: String,
    pub(crate) parser: String,
    #[serde(deserialize_with = "optional_canonical_uuid")]
    pub(crate) import_job: Option<Uuid>,
}
impl DocumentFence {
    pub(crate) fn validate(&self) -> AppResult<()> {
        if self.version == 0
            || !super::codec::is_hash(&self.sha256)
            || self.parser.is_empty()
            || self.parser.len() > 128
        {
            return Err(repair());
        }
        Ok(())
    }
    pub(crate) fn matches(&self, tx: &Transaction<'_>, status: &str) -> AppResult<bool> {
        self.validate()?;
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE \
                    id=?1 AND version=?2 AND sha256=?3 AND \
                    parser_identity=?4 AND import_job_id IS ?5 \
                    AND status=?6)",
            params![
                self.id.to_string(),
                self.version,
                self.sha256,
                self.parser,
                self.import_job.map(|i| i.to_string()),
                status
            ],
            |r| r.get(0),
        )?)
    }
}
#[derive(Clone)]
pub(crate) struct ReadyPublication {
    pub(crate) operation: Uuid,
    pub(crate) work: Uuid,
    pub(crate) document: DocumentFence,
    pub(crate) expected_generation: Option<Uuid>,
    pub(crate) count: u32,
    pub(crate) job: JobPublicationFence,
    pub(crate) job_proof: crate::knowledge::service::job_proof::DocumentJobProof,
    pub(crate) cleanup_directory: super::delete_completion::ManagedDirectory,
}
impl ReadyPublication {
    fn ready(&self, tx: &Transaction<'_>, generation: &Generation) -> AppResult<()> {
        self.job_proof.require(tx, &self.document)?;
        if !self.document.matches(tx, "indexing")? {
            return Err(repair());
        }
        let identity = generation.key()?;
        let generation_id = generation.generation.to_string();
        let operation = self.work.to_string();
        let prior: Option<String> = tx.query_row(
            "SELECT active_generation FROM \
                    knowledge_index_generations WHERE identity=?1",
            [&identity],
            |r| r.get(0),
        )?;
        if prior != self.expected_generation.map(|g| g.to_string()) {
            return Err(repair());
        }
        let count: u32 = tx.query_row(
            "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
            [&operation],
            |r| r.get(0),
        )?;
        let chunks: u32 = tx.query_row(
            "SELECT count(*) FROM document_chunks WHERE \
                    document_id=?1 AND document_version=?2",
            params![self.document.id.to_string(), self.document.version],
            |r| r.get(0),
        )?;
        if count != self.count || chunks != self.count {
            return Err(repair());
        }
        let mut after = -1i64;
        let mut seen = 0u32;
        loop {
            let mut query = tx.prepare(
                "SELECT ordinal,identity,generation,vector_id,\
                    document_id,document_version,chunk_id,\
                    content_hash FROM knowledge_index_work_items \
                    WHERE operation_id=?1 AND ordinal>?2 ORDER \
                    BY ordinal LIMIT 200",
            )?;
            let mut rows = query.query(params![operation, after])?;
            let mut page = 0;
            while let Some(r) = rows.next()? {
                let ordinal: i64 = r.get(0)?;
                let vector: String = r.get(3)?;
                let chunk: String = r.get(6)?;
                let hash: String = r.get(7)?;
                staged_associations(tx, self.work, ordinal, false)?;
                if ordinal != i64::from(seen)
                    || r.get::<_, String>(1)? != identity
                    || r.get::<_, String>(2)? != generation_id
                    || r.get::<_, String>(4)? != self.document.id.to_string()
                    || r.get::<_, u32>(5)? != self.document.version
                {
                    return Err(repair());
                }
                let valid: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings m JOIN \
                    document_chunks c ON c.id=m.chunk_id WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.chunk_id=?3 AND m.vector_id=?4 AND \
                    m.document_id=?5 AND m.document_version=?6 \
                    AND m.content_hash=?7 AND m.published=0 AND \
                    c.document_id=m.document_id AND \
                    c.document_version=m.document_version AND \
                    c.content_hash=m.content_hash AND \
                    c.index_state='indexing')",
                    params![
                        identity,
                        generation_id,
                        chunk,
                        vector,
                        self.document.id.to_string(),
                        self.document.version,
                        hash
                    ],
                    |r| r.get(0),
                )?;
                if !valid {
                    return Err(repair());
                }
                tx.execute(
                    "UPDATE knowledge_generation_mappings SET \
                    published=1 WHERE identity=?1 AND \
                    generation=?2 AND chunk_id=?3",
                    params![identity, generation_id, chunk],
                )?;
                if tx.execute(
                    "UPDATE document_chunks SET vector_id=?1,\
                    embedding_identity=?2,index_state='ready' \
                    WHERE id=?3 AND document_version=?4 AND \
                    content_hash=?5",
                    params![vector, identity, chunk, self.document.version, hash],
                )? != 1
                {
                    return Err(repair());
                }
                after = ordinal;
                seen = seen.checked_add(1).ok_or_else(repair)?;
                page += 1;
            }
            if page == 0 {
                break;
            }
        }
        if seen != self.count {
            return Err(repair());
        }
        let semantic = generation.identity.semantic_key()?;
        let bound: Option<String> = tx.query_row(
            "SELECT semantic_key FROM knowledge_index_generations WHERE identity=?1",
            [&identity],
            |r| r.get(0),
        )?;
        if bound.is_some_and(|b| b != semantic) {
            return Err(repair());
        }
        tx.execute(
            "UPDATE knowledge_index_generations SET \
                    semantic_key=?1,requested_dimension=?2,\
                    active_generation=?3,state='active',\
                    updated_at_ms=?4 WHERE identity=?5",
            params![
                semantic,
                generation.requested_dimension,
                generation_id,
                now_ms(),
                identity
            ],
        )
        .map_err(|_| repair())?;
        // A dirty source may be recovered into a fresh generation. Other documents
        // retain their text and are explicitly repair-required until reindexed.
        if self
            .expected_generation
            .is_some_and(|old| old != generation.generation)
        {
            tx.execute(
                "UPDATE documents SET \
                    status='repair_required',\
                    error_code='index_repair_required' WHERE \
                    index_identity=?1 AND id!=?2 AND \
                    status='ready'",
                params![identity, self.document.id.to_string()],
            )?;
            tx.execute(
                "INSERT INTO knowledge_repairs(document_id,\
                    reason) SELECT id,'unsealed_generation' FROM \
                    documents WHERE index_identity=?1 AND \
                    status='repair_required' ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
                [&identity],
            )?;
        }
        tx.execute(
            "UPDATE documents SET status='ready',\
                    index_identity=?1,error_code=NULL,\
                    updated_at_ms=?2 WHERE id=?3",
            params![identity, now_ms(), self.document.id.to_string()],
        )?;
        tx.execute(
            "DELETE FROM knowledge_repairs WHERE document_id=?1",
            [self.document.id.to_string()],
        )?;
        // Removal is in the same publication transaction; uncommitted rows stay
        // durable on any rollback and never qualify for authoritative hydration.
        tx.execute(
            "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
            [operation],
        )?;
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) enum PublicationPlan {
    Ready(ReadyPublication),
    Recovery {
        operation: Uuid,
        work: Uuid,
        token: Uuid,
        fence: super::recovery::RecoveryFence,
        count: u32,
        delete_work: Uuid,
        document: DocumentFence,
        job_proof: crate::knowledge::service::job_proof::DocumentJobProof,
        work_proof: super::delete_work::DeleteWorkProof,
        cleanup_directory: super::delete_completion::ManagedDirectory,
    },
    Delete {
        operation: Uuid,
        work: Uuid,
        document: DocumentFence,
        job: Option<JobPublicationFence>,
        job_proof: crate::knowledge::service::job_proof::DocumentJobProof,
        work_proof: super::delete_work::DeleteWorkProof,
        cleanup_directory: super::delete_completion::ManagedDirectory,
    },
    Cleanup {
        operation: Uuid,
        work: Uuid,
    },
    Swap {
        operation: Uuid,
        work: Uuid,
        source: Uuid,
        count: u32,
        cleanup_directory: super::delete_completion::ManagedDirectory,
    },
}
impl PublicationPlan {
    pub(super) fn apply(
        &self,
        tx: &Transaction<'_>,
        receipt: &super::owner::PersistedReceipt,
    ) -> AppResult<()> {
        let intent = receipt.intent().body();
        match self {
            Self::Recovery {
                operation,
                work,
                token: _,
                fence,
                count,
                delete_work,
                document,
                job_proof,
                work_proof,
                cleanup_directory,
            } => {
                if *operation != intent.operation_id
                    || *count > 200000
                    || fence.source.key()? != intent.identity
                    || fence.source.generation == intent.descriptor.generation
                {
                    return Err(repair());
                }
                super::recovery::validate(tx, fence, true)?;
                fresh_coverage(tx, receipt, *work, *count)?;
                job_proof.require(tx, document)?;
                let deletion = crate::knowledge::service::job_proof::delete(tx, *delete_work)?
                    .ok_or_else(repair)?;
                work_proof.require_live(tx, &deletion.input)?;
                if !document.matches(tx, "deleting")? {
                    return Err(repair());
                }
                let source = fence.source.generation.to_string();
                let target = intent.descriptor.generation.to_string();
                let invalid: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_index_work_items WHERE \
                    operation_id=?1 AND (identity!=?2 OR \
                    generation!=?3 OR document_id IS NOT ?4))",
                    params![
                        delete_work.to_string(),
                        intent.identity,
                        source,
                        document.id.to_string()
                    ],
                    |r| r.get(0),
                )?;
                if invalid {
                    return Err(repair());
                }
                let seen = visit(tx, *work, |r| {
                    let ordinal: i64 = r.get(0)?;
                    let chunk: String = r.get(6)?;
                    let hash: String = r.get(7)?;
                    let vector: String = r.get(3)?;
                    if r.get::<_, String>(1)? != intent.identity || r.get::<_, String>(2)? != target
                    {
                        return Err(repair());
                    }
                    staged_associations(tx, *work, ordinal, true)?;
                    let valid: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings old JOIN \
                    knowledge_generation_mappings new ON \
                    new.chunk_id=old.chunk_id JOIN \
                    knowledge_index_work_items w ON \
                    w.chunk_id=new.chunk_id AND \
                    w.operation_id=?7 WHERE old.identity=?1 AND \
                    old.generation=?2 AND old.published=1 AND \
                    new.identity=?1 AND new.generation=?3 AND \
                    new.chunk_id=?4 AND new.vector_id=?5 AND \
                    new.content_hash=?6 AND new.published=0 AND \
                    old.document_id=new.document_id AND \
                    old.document_version=new.document_version \
                    AND old.content_hash=new.content_hash AND \
                    old.vector_id=w.previous_vector_id)",
                        params![
                            intent.identity,
                            source,
                            target,
                            chunk,
                            vector,
                            hash,
                            work.to_string()
                        ],
                        |r| r.get(0),
                    )?;
                    if !valid {
                        return Err(repair());
                    }
                    tx.execute(
                        "UPDATE knowledge_generation_mappings SET \
                    published=1 WHERE identity=?1 AND \
                    generation=?2 AND chunk_id=?3",
                        params![intent.identity, target, chunk],
                    )?;
                    tx.execute(
                        "UPDATE document_chunks SET vector_id=?1,\
                    embedding_identity=?2,index_state='ready' \
                    WHERE id=?3",
                        params![vector, intent.identity, chunk],
                    )?;
                    Ok(())
                })?;
                if seen != *count {
                    return Err(repair());
                }
                complete_survivors(tx, &intent.identity, &target, true)?;
                // Whole-document Ready requires complete destination coverage.
                tx.execute(
                    "UPDATE documents SET status='ready',\
                    error_code=NULL WHERE index_identity=?1 AND \
                    (status='ready' OR (status='repair_required' \
                    AND EXISTS(SELECT 1 FROM knowledge_repairs r \
                    WHERE r.document_id=documents.id AND \
                    r.reason='unsealed_generation'))) AND \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=documents.id AND \
                    c.document_version=documents.version) AND \
                    NOT EXISTS(SELECT 1 FROM document_chunks c \
                    WHERE c.document_id=documents.id AND \
                    c.document_version=documents.version AND NOT \
                    EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings m WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.published=1 AND m.chunk_id=c.id AND \
                    m.document_id=documents.id AND \
                    m.document_version=documents.version AND \
                    m.content_hash=c.content_hash AND \
                    m.vector_id=c.vector_id))",
                    params![intent.identity, target],
                )?;
                tx.execute(
                    "DELETE FROM knowledge_repairs WHERE \
                    document_id IN (SELECT id FROM documents \
                    WHERE index_identity=?1 AND status='ready')",
                    [&intent.identity],
                )?;
                super::delete_completion::publish_fresh(
                    tx,
                    receipt,
                    fence.source.generation,
                    *count,
                    super::delete_completion::CompletionKind::Recovery,
                    cleanup_directory,
                )?;
                tx.execute(
                    "UPDATE knowledge_index_generations SET \
                    active_generation=?1,state='active',\
                    updated_at_ms=?2 WHERE identity=?3",
                    params![target, now_ms(), intent.identity],
                )?;
                tx.execute(
                    "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
                    [work.to_string()],
                )?;
                Ok(())
            }
            Self::Ready(plan) => {
                if plan.operation != intent.operation_id || plan.count == 0 || plan.count > 10000 {
                    return Err(repair());
                }
                if intent.operation == IntentKind::Create {
                    fresh_coverage(tx, receipt, plan.work, plan.count)?;
                }
                plan.job.commit(tx, || {
                    if intent.operation == IntentKind::Create {
                        if let Some(source) = plan
                            .expected_generation
                            .filter(|source| *source != intent.descriptor.generation)
                        {
                            super::delete_completion::publish_fresh(
                                tx,
                                receipt,
                                source,
                                plan.count,
                                super::delete_completion::CompletionKind::Ready,
                                &plan.cleanup_directory,
                            )?;
                        }
                    }
                    plan.ready(tx, &intent.descriptor)
                })
            }
            Self::Delete {
                operation,
                work,
                document,
                job,
                job_proof,
                work_proof,
                cleanup_directory,
            } => {
                if *operation != intent.operation_id {
                    return Err(repair());
                }
                let remove = || {
                    job_proof.require(tx, document)?;
                    let deletion = crate::knowledge::service::job_proof::delete(tx, *work)?
                        .ok_or_else(repair)?;
                    if deletion.input.identity.as_deref() != Some(&intent.identity)
                        || deletion.input.generation != Some(intent.descriptor.generation)
                    {
                        return Err(repair());
                    }
                    work_proof.require_live(tx, &deletion.input)?;
                    if !document.matches(tx, "deleting")? {
                        return Err(repair());
                    }
                    let foreign: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_index_work_items WHERE \
                    operation_id=?1 AND (identity!=?2 OR \
                    generation!=?3 OR document_id IS NOT ?4))",
                        params![
                            work.to_string(),
                            intent.identity,
                            intent.descriptor.generation.to_string(),
                            document.id.to_string()
                        ],
                        |r| r.get(0),
                    )?;
                    if foreign {
                        return Err(repair());
                    }
                    super::delete_completion::publish_delete(
                        tx,
                        &deletion.input,
                        Some(receipt),
                        cleanup_directory,
                    )
                };
                match job {
                    Some(j) => j.commit(tx, remove),
                    None => remove(),
                }
            }
            Self::Cleanup { operation, work } => {
                if *operation != intent.operation_id {
                    return Err(repair());
                }
                visit(tx, *work, |r| {
                    if r.get::<_, String>(1)? != intent.identity
                        || r.get::<_, String>(2)? != intent.descriptor.generation.to_string()
                    {
                        return Err(repair());
                    }
                    tx.execute(
                        "DELETE FROM knowledge_generation_mappings \
                    WHERE identity=?1 AND generation=?2 AND \
                    vector_id=?3",
                        params![
                            intent.identity,
                            intent.descriptor.generation.to_string(),
                            r.get::<_, String>(3)?
                        ],
                    )?;
                    Ok(())
                })?;
                tx.execute(
                    "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
                    [work.to_string()],
                )?;
                Ok(())
            }
            Self::Swap {
                operation,
                work,
                source,
                count,
                cleanup_directory,
            } => {
                if *operation != intent.operation_id
                    || *count > 200000
                    || *source == intent.descriptor.generation
                {
                    return Err(repair());
                }
                let target = intent.descriptor.generation.to_string();
                let source_id = *source;
                let source = source.to_string();
                let active: Option<String> = tx.query_row(
                    "SELECT active_generation FROM \
                    knowledge_index_generations WHERE identity=?1",
                    [&intent.identity],
                    |r| r.get(0),
                )?;
                if active.as_deref() != Some(&source) {
                    return Err(repair());
                }
                fresh_coverage(tx, receipt, *work, *count)?;
                let seen = visit(tx, *work, |r| {
                    let chunk: String = r.get(6)?;
                    let hash: String = r.get(7)?;
                    let vector: String = r.get(3)?;
                    staged_associations(tx, *work, r.get(0)?, true)?;
                    if r.get::<_, String>(1)? != intent.identity || r.get::<_, String>(2)? != target
                    {
                        return Err(repair());
                    }
                    let valid: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings old JOIN \
                    knowledge_generation_mappings new ON \
                    new.chunk_id=old.chunk_id JOIN \
                    document_chunks c ON c.id=old.chunk_id JOIN \
                    documents d ON d.id=c.document_id WHERE \
                    old.identity=?1 AND old.generation=?2 AND \
                    old.published=1 AND new.identity=?1 AND \
                    new.generation=?3 AND new.chunk_id=?4 AND \
                    new.vector_id=?5 AND new.content_hash=?6 AND \
                    new.published=0 AND \
                    old.document_id=new.document_id AND \
                    old.document_version=new.document_version \
                    AND old.content_hash=new.content_hash AND \
                    d.version=new.document_version AND \
                    d.status='ready' AND d.index_identity=?1 AND \
                    c.document_version=d.version AND \
                    c.content_hash=new.content_hash AND \
                    c.vector_id=old.vector_id AND \
                    c.index_state='ready')",
                        params![intent.identity, source, target, chunk, vector, hash],
                        |r| r.get(0),
                    )?;
                    if !valid {
                        return Err(repair());
                    }
                    tx.execute(
                        "UPDATE knowledge_generation_mappings SET \
                    published=1 WHERE identity=?1 AND \
                    generation=?2 AND chunk_id=?3",
                        params![intent.identity, target, chunk],
                    )?;
                    tx.execute(
                        "UPDATE document_chunks SET vector_id=?1,embedding_identity=?2 WHERE id=?3",
                        params![vector, intent.identity, chunk],
                    )?;
                    Ok(())
                })?;
                if seen != *count {
                    return Err(repair());
                }
                complete_survivors(tx, &intent.identity, &target, false)?;
                super::delete_completion::publish_fresh(
                    tx,
                    receipt,
                    source_id,
                    *count,
                    super::delete_completion::CompletionKind::Swap,
                    cleanup_directory,
                )?;
                tx.execute(
                    "UPDATE knowledge_index_generations SET \
                    active_generation=?1,state='active',\
                    updated_at_ms=?2 WHERE identity=?3",
                    params![target, now_ms(), intent.identity],
                )?;
                tx.execute(
                    "DELETE FROM knowledge_index_work_items WHERE operation_id=?1",
                    [work.to_string()],
                )?;
                Ok(())
            }
        }
    }
}
fn fresh_coverage(
    tx: &Transaction<'_>,
    receipt: &super::owner::PersistedReceipt,
    work: Uuid,
    count: u32,
) -> AppResult<()> {
    use crate::knowledge::service::state;
    let intent = receipt.intent().body();
    let seal = Seal::decode(receipt.seal_json())?;
    if intent.operation != IntentKind::Create
        || intent.prior_seal_sha256.is_some()
        || seal.operation_id != intent.operation_id
        || seal.descriptor != intent.descriptor
        || seal.stored_count != u64::from(count)
        || count > 200_000
    {
        return Err(repair());
    }
    let target = intent.descriptor.generation.to_string();
    let mappings: u64 = tx.query_row(
        "SELECT count(*) FROM knowledge_generation_mappings WHERE identity=?1 AND generation=?2",
        params![intent.identity, target],
        |r| r.get(0),
    )?;
    if mappings != seal.stored_count {
        return Err(repair());
    }
    let mut next = 0u32;
    let seen = visit(tx, work, |r| {
        let ordinal: u32 = r.get(0).map_err(|_| repair())?;
        let text = |column, max| state::text(r, column, max).map_err(|_| repair());
        if ordinal != next
            || text(1, 64)? != intent.identity
            || text(2, 36)? != target
            || text(3, 20)? != next.to_string()
        {
            return Err(repair());
        }
        let mapped: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_generation_mappings WHERE identity=?1 \
             AND generation=?2 AND vector_id=?3 AND document_id=?4 AND document_version=?5 \
             AND chunk_id=?6 AND content_hash=?7)",
            params![
                intent.identity,
                target,
                text(3, 20)?,
                text(4, 36)?,
                r.get::<_, u32>(5).map_err(|_| repair())?,
                text(6, 36)?,
                text(7, 64)?
            ],
            |r| r.get(0),
        )?;
        if !mapped {
            return Err(repair());
        }
        next = next.checked_add(1).ok_or_else(repair)?;
        Ok(())
    })?;
    if seen != count || next != count {
        return Err(repair());
    }
    Ok(())
}
fn visit(
    tx: &Transaction<'_>,
    operation: Uuid,
    mut action: impl FnMut(&rusqlite::Row<'_>) -> AppResult<()>,
) -> AppResult<u32> {
    let mut after = -1i64;
    let mut count = 0u32;
    loop {
        let mut q = tx.prepare(
            "SELECT ordinal,identity,generation,vector_id,\
                    document_id,document_version,chunk_id,\
                    content_hash FROM knowledge_index_work_items \
                    WHERE operation_id=?1 AND ordinal>?2 ORDER \
                    BY ordinal LIMIT 200",
        )?;
        let mut rows = q.query(params![operation.to_string(), after])?;
        let mut page = 0;
        while let Some(r) = rows.next()? {
            let ordinal: i64 = r.get(0)?;
            if ordinal <= after || count >= 200000 {
                return Err(repair());
            }
            action(r)?;
            after = ordinal;
            count += 1;
            page += 1;
        }
        if page == 0 {
            return Ok(count);
        }
    }
}

fn staged_associations(
    tx: &Transaction<'_>,
    operation: Uuid,
    ordinal: i64,
    survivor: bool,
) -> AppResult<()> {
    use crate::knowledge::{service::state, types::TextLocation};
    let mut q = tx.prepare(
        "SELECT w.document_fence_json,\
                    w.location_sha256,c.text,c.location_json,\
                    c.content_hash,d.status,w.previous_vector_id,\
                    c.vector_id,c.index_state,w.document_id,\
                    w.document_version FROM \
                    knowledge_index_work_items w JOIN \
                    document_chunks c ON c.id=w.chunk_id JOIN \
                    documents d ON d.id=w.document_id WHERE \
                    w.operation_id=?1 AND w.ordinal=?2 AND \
                    c.document_id=w.document_id AND \
                    c.document_version=w.document_version AND \
                    c.content_hash=w.content_hash",
    )?;
    let mut rows = q.query(params![operation.to_string(), ordinal])?;
    let r = rows.next()?.ok_or_else(repair)?;
    let f: DocumentFence = serde_json::from_str(state::text(r, 0, 1024)?).map_err(|_| repair())?;
    let status = state::text(r, 5, 32)?;
    if f.id.to_string() != state::text(r, 9, 36)?
        || f.version != r.get::<_, u32>(10)?
        || !f.matches(tx, status)?
    {
        return Err(repair());
    }
    if survivor {
        let quarantined: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_repairs WHERE document_id=?1 AND \
                    reason='unsealed_generation')",
            [f.id.to_string()],
            |r| r.get(0),
        )?;
        if (status != "ready" && (status != "repair_required" || !quarantined))
            || state::text(r, 8, 32)? != "ready"
            || state::text(r, 6, 20)? != state::text(r, 7, 20)?
        {
            return Err(repair());
        }
    } else if status != "indexing" || state::text(r, 8, 32)? != "indexing" {
        return Err(repair());
    }
    if crate::knowledge::chunk_hash(state::text(r, 2, 9600)?) != state::text(r, 4, 64)? {
        return Err(repair());
    }
    let location: TextLocation =
        serde_json::from_str(state::text(r, 3, 16 * 1024 * 1024)?).map_err(|_| repair())?;
    if super::codec::location_digest(&location)? != state::text(r, 1, 64)? {
        return Err(repair());
    }
    Ok(())
}

fn optional_canonical_uuid<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Uuid>, D::Error> {
    let raw = Option::<String>::deserialize(d)?;
    raw.map(|s| {
        let id = Uuid::parse_str(&s).map_err(|_| serde::de::Error::custom("invalid UUID"))?;
        if id.to_string() != s {
            return Err(serde::de::Error::custom("noncanonical UUID"));
        }
        Ok(id)
    })
    .transpose()
}

fn complete_survivors(
    tx: &Transaction<'_>,
    identity: &str,
    target: &str,
    restore_unsealed: bool,
) -> AppResult<()> {
    let incomplete: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents d \
                    WHERE d.index_identity=?1 AND \
                    (d.status='ready' OR (?3 AND \
                    d.status='repair_required' AND EXISTS(SELECT \
                    1 FROM knowledge_repairs r WHERE \
                    r.document_id=d.id AND \
                    r.reason='unsealed_generation'))) AND (NOT \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=d.id AND \
                    c.document_version=d.version) OR \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=d.id AND \
                    c.document_version=d.version AND NOT \
                    EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings m WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.published=1 AND m.chunk_id=c.id AND \
                    m.document_id=d.id AND \
                    m.document_version=d.version AND \
                    m.content_hash=c.content_hash AND \
                    m.vector_id=c.vector_id))))",
        params![identity, target, restore_unsealed],
        |r| r.get(0),
    )?;
    if incomplete {
        return Err(repair());
    }
    Ok(())
}
