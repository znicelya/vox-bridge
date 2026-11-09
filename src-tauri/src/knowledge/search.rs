//! Candidates stay compact; SQLite alone authorizes full source hydration.
use super::{
    embedding::{EmbeddingExecution, error, json_bytes},
    identity::EmbeddingIdentity,
    index::ChunkMetadata,
    service::{Core, state},
    types::*,
};
use crate::{
    error::AppResult,
    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode as Code},
};
use rusqlite::params;
use serde::Serialize;
use uuid::Uuid;
const BUDGET: usize = 32 * 1024 * 1024;
impl Core {
    pub(crate) async fn search(
        &self,
        request: KnowledgeSearch,
        owner: OperationLease,
        mut cancel: tokio::sync::watch::Receiver<bool>,
    ) -> AppResult<KnowledgeResults> {
        validate(&self.store, &request)?;
        let total: u32 = self.store.transaction(|tx| {
            Ok(tx.query_row("SELECT count(*) FROM documents", [], |r| r.get(0))?)
        })?;
        if total == 0 {
            return Ok(KnowledgeResults {
                hits: vec![],
                identity: None,
                unavailable_documents: 0,
            });
        }
        let _serial = tokio::select! {
        biased;
                _=crate::intelligence::http::cancelled(&mut cancel)=>return Err(error(Code::Cancelled)),
        permit=self.serial.lock()=>permit};
        let profile = self
            .config
            .scoped(owner.clone())
            .selected_embedding_snapshot()
            .await?
            .ok_or_else(|| error(Code::IndexRepairRequired))?;
        let binding = state::binding(&self.store, &profile)?
            .ok_or_else(|| error(Code::IndexRepairRequired))?;
        let identity = binding.generation.key()?;
        let (healthy, unavailable) =
            state::health(&self.store, &identity, binding.generation.generation)?;
        if healthy == 0 {
            return Err(error(Code::IndexRepairRequired));
        }
        // Open validates the local sealed source before the metered request.
        let ticket = self
            .index
            .open_existing(binding.generation.clone(), owner.clone())?;
        let result = async {
            let open = self.index.enqueue_open(&ticket)?.observe();
            tokio::pin!(open);
            tokio::select! {
            biased;
                        _=crate::intelligence::http::cancelled(&mut cancel)=>{
            ticket.cancel();
                        let _=open.await;
                        return Err(error(Code::Cancelled));
                        }
            ,
            result=&mut open=>{
            result?;
                        }
            }
            let vectors = self
                .embeddings
                .embed(
                    &profile,
                    &[request.query],
                    EmbeddingExecution::new(owner, cancel.clone()),
                )
                .await?;
            if *cancel.borrow() {
                return Err(error(Code::Cancelled));
            }
            if vectors.len() != 1 {
                return Err(error(Code::InvalidResponse));
            }
            let vector = vectors.into_iter().next().unwrap();
            if EmbeddingIdentity::from_profile(
                &profile,
                u32::try_from(vector.len()).map_err(|_| error(Code::InvalidResponse))?,
            )? != binding.generation.identity
            {
                return Err(error(Code::IndexRepairRequired));
            }
            let work = self
                .index
                .candidates(&ticket, vector, (request.limit * 5).min(100));
            tokio::pin!(work);
            let candidates = tokio::select! {
            biased;
                        _=crate::intelligence::http::cancelled(&mut cancel)=>{
            ticket.cancel();
                        let _=work.await;
                        return Err(error(Code::Cancelled));
                        }
            ,
            value=&mut work=>value?};
            let mut results = KnowledgeResults {
                hits: vec![],
                identity: Some(identity.clone()),
                unavailable_documents: unavailable,
            };
            let mut bytes = json_bytes(&results)?;
            for candidate in candidates {
                if *cancel.borrow() {
                    return Err(error(Code::Cancelled));
                }
                let (id, score, metadata) = (candidate.id, candidate.score, candidate.metadata);
                if !score.is_finite() {
                    return Err(error(Code::IndexRepairRequired));
                }
                match hydrate(
                    &self.store,
                    &identity,
                    binding.generation.generation,
                    id,
                    score,
                    metadata.as_ref(),
                    bytes,
                    results.hits.is_empty(),
                )? {
                    Hydrated::Hit(hit, count) => {
                        bytes = bytes
                            .checked_add(count + usize::from(!results.hits.is_empty()))
                            .ok_or_else(|| error(Code::ResourceLimit))?;
                        results.hits.push(hit);
                        if results.hits.len() == request.limit as usize {
                            break;
                        }
                    }
                    Hydrated::Overflow => break,
                    Hydrated::Skip => {}
                }
            }
            Ok(results)
        }
        .await;
        // Read-only search still creates a writable Open intent. Complete its
        // seal/publish/ack lifecycle on success and on recoverable request errors.
        let cleanup = ticket.cleanup();
        if self.index.enqueue_open(&cleanup)?.observe().await.is_err() {
            state::mark_identity_repair(&self.store, &identity, "index_missing")?;
            return Err(error(Code::IndexRepairRequired));
        }
        let receipt = match self.index.persist(&cleanup).await {
            Ok(r) => r,
            Err(e) => {
                state::mark_identity_repair(&self.store, &identity, "unsealed_generation")?;
                return Err(e);
            }
        };
        let ack = self.index.publish(&cleanup, &receipt).await?;
        self.index.acknowledge(&cleanup, ack).await?;
        let mut result = result?;
        #[cfg(test)]
        self.checkpoint(super::service::CommitPoint::BeforeSearchRecheck)?;
        if *cancel.borrow() {
            return Err(error(Code::Cancelled));
        }
        self.store.transaction(|tx| {
            let current: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_index_generations WHERE \
                    identity=?1 AND active_generation=?2)",
                params![identity, binding.generation.generation.to_string()],
                |r| r.get(0),
            )?;
            let mut q = tx.prepare(
                "SELECT EXISTS(SELECT 1 FROM documents d JOIN \
                    document_chunks c ON c.document_id=d.id JOIN \
                    knowledge_generation_mappings m ON \
                    m.chunk_id=c.id JOIN \
                    knowledge_index_generations g ON \
                    g.identity=m.identity AND \
                    g.active_generation=m.generation WHERE \
                    d.id=?1 AND d.version=?2 AND \
                    d.status='ready' AND d.index_identity=?3 AND \
                    c.id=?4 AND c.document_version=d.version AND \
                    c.index_state='ready' AND \
                    c.embedding_identity=?3 AND \
                    c.vector_id=m.vector_id AND \
                    c.content_hash=?5 AND m.identity=?3 AND \
                    m.generation=?6 AND m.published=1 AND \
                    m.document_id=d.id AND \
                    m.document_version=d.version AND \
                    m.content_hash=c.content_hash)",
            )?;
            let mut keep = Vec::with_capacity(result.hits.len());
            for hit in &result.hits {
                let complete = state::ready_coverage(
                    tx,
                    hit.document_id,
                    hit.document_version,
                    &identity,
                    binding.generation.generation,
                )?;
                if current && !complete {
                    state::mark_document_repair_tx(
                        tx,
                        hit.document_id,
                        hit.document_version,
                        "vector_missing",
                    )?;
                }
                let eligible: bool = q.query_row(
                    params![
                        hit.document_id.to_string(),
                        hit.document_version,
                        identity,
                        hit.chunk_id.to_string(),
                        crate::knowledge::chunk_hash(&hit.text),
                        binding.generation.generation.to_string()
                    ],
                    |r| r.get(0),
                )?;
                keep.push(eligible && complete);
            }
            let mut i = 0;
            result.hits.retain(|_| {
                let retained = keep[i];
                i += 1;
                retained
            });
            Ok(())
        })?;
        let (healthy, unavailable) =
            state::health(&self.store, &identity, binding.generation.generation)?;
        if healthy == 0 {
            return Err(error(Code::IndexRepairRequired));
        }
        result.unavailable_documents = unavailable;
        while json_bytes(&result)? > BUDGET {
            if result.hits.pop().is_none() || result.hits.is_empty() {
                return Err(error(Code::ResourceLimit));
            }
        }
        Ok(result)
    }
}
pub(crate) fn validate(
    store: &crate::storage::LocalStore,
    request: &KnowledgeSearch,
) -> AppResult<()> {
    let chars = request.query.chars().count();
    if !(1..=4096).contains(&chars)
        || !(1..=20).contains(&request.limit)
        || request.selected_segment_ids.len() > 200
    {
        return Err(error(Code::Configuration));
    }
    if !request.selected_segment_ids.is_empty() {
        let session = request
            .session_id
            .ok_or_else(|| error(Code::Configuration))?;
        store.transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT EXISTS(SELECT 1 FROM \
                    translation_segments WHERE id=?1 AND \
                    session_id=?2)",
            )?;
            for id in &request.selected_segment_ids {
                let belongs: bool =
                    q.query_row(params![id.to_string(), session.to_string()], |r| r.get(0))?;
                if !belongs {
                    return Err(error(Code::Configuration));
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
enum Hydrated {
    Hit(KnowledgeHit, usize),
    Overflow,
    Skip,
}
#[derive(Serialize)]
struct BorrowedHit<'a> {
    document_id: Uuid,
    document_version: u32,
    chunk_id: Uuid,
    title: &'a str,
    location: &'a TextLocation,
    text: &'a str,
    score: f32,
}
fn hydrate(
    store: &crate::storage::LocalStore,
    identity: &str,
    generation: Uuid,
    vector: u64,
    score: f32,
    metadata: Option<&ChunkMetadata>,
    used: usize,
    first: bool,
) -> AppResult<Hydrated> {
    match state::classify(store, identity, generation, vector, metadata)? {
        state::Record::Healthy(_, _) => {}
        state::Record::Repair(doc, version, reason) => {
            state::mark_document_repair(store, doc, version, reason)?;
            return Ok(Hydrated::Skip);
        }
        state::Record::Orphan | state::Record::Tombstone | state::Record::Retained => {
            return Ok(Hydrated::Skip);
        }
    }
    store.transaction(|tx| {
        let mut q = tx.prepare(
            "SELECT d.id,d.version,c.id,COALESCE(d.title,\
                    ''),c.text,c.location_json,c.content_hash,\
                    m.document_version,m.content_hash FROM \
                    knowledge_generation_mappings m JOIN \
                    knowledge_index_generations g ON \
                    g.identity=m.identity AND \
                    g.active_generation=m.generation JOIN \
                    document_chunks c ON c.id=m.chunk_id JOIN \
                    documents d ON d.id=c.document_id WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.vector_id=?3 AND m.published=1 AND \
                    d.status='ready' AND \
                    d.index_identity=m.identity AND \
                    c.index_state='ready' AND \
                    c.embedding_identity=m.identity AND \
                    c.vector_id=m.vector_id AND \
                    c.document_version=d.version AND \
                    m.document_id=d.id AND \
                    m.document_version=d.version",
        )?;
        let mut rows = q.query(params![
            identity,
            generation.to_string(),
            vector.to_string()
        ])?;
        let Some(r) = rows.next()? else {
            return Ok(Hydrated::Skip);
        };
        let doc = Uuid::parse_str(state::text(r, 0, 36)?).map_err(|_| error(Code::Storage))?;
        let version: u32 = r.get(1)?;
        if !state::ready_coverage(tx, doc, version, identity, generation)? {
            state::mark_document_repair_tx(tx, doc, version, "vector_missing")?;
            return Ok(Hydrated::Skip);
        }
        let chunk = Uuid::parse_str(state::text(r, 2, 36)?).map_err(|_| error(Code::Storage))?;
        let title = match state::text(r, 3, 32768) {
            Ok(title) => title,
            Err(_) => {
                state::mark_document_repair_tx(tx, doc, version, "stale_hash")?;
                return Ok(Hydrated::Skip);
            }
        };
        let body = state::text(r, 4, 9600)?;
        let location_json = state::text(r, 5, 16 * 1024 * 1024)?;
        if body.chars().count() > 2400 {
            return Err(error(Code::ResourceLimit));
        }
        let location: TextLocation =
            serde_json::from_str(location_json).map_err(|_| error(Code::IndexRepairRequired))?;
        if location
            .heading
            .as_ref()
            .is_some_and(|h| h.len() > 16 * 1024 * 1024)
        {
            return Err(error(Code::ResourceLimit));
        }
        let hash = state::text(r, 6, 64)?;
        let valid = if let Some(metadata) = metadata {
            doc == metadata.document_id
                && version == metadata.document_version
                && chunk == metadata.chunk_id
                && hash == metadata.content_hash
                && state::text(r, 8, 64)? == hash
                && crate::knowledge::chunk_hash(body) == hash
                && metadata.matches_location(&location)?
        } else {
            false
        };
        if !valid {
            state::mark_document_repair_tx(tx, doc, version, "stale_hash")?;
            return Ok(Hydrated::Skip);
        }
        let hit = BorrowedHit {
            document_id: doc,
            document_version: version,
            chunk_id: chunk,
            title,
            location: &location,
            text: body,
            score,
        };
        let count = json_bytes(&hit)?;
        if used
            .checked_add(count + usize::from(!first))
            .is_none_or(|n| n > BUDGET)
        {
            return if first {
                Err(error(Code::ResourceLimit))
            } else {
                Ok(Hydrated::Overflow)
            };
        }
        Ok(Hydrated::Hit(
            KnowledgeHit {
                document_id: doc,
                document_version: version,
                chunk_id: chunk,
                title: title.into(),
                text: body.into(),
                location,
                score,
            },
            count,
        ))
    })
}
