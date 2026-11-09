use super::*;
use crate::knowledge::{
    identity::EmbeddingIdentity,
    index::{ChunkMetadata, DocumentFence, Generation, Seal},
    types::TextLocation,
};
use rusqlite::{OptionalExtension, Transaction, params, types::ValueRef};
#[derive(Clone)]
pub(crate) struct Binding {
    pub(crate) generation: Generation,
}
pub(crate) fn text<'a>(
    row: &'a rusqlite::Row<'_>,
    column: usize,
    max: usize,
) -> AppResult<&'a str> {
    match row.get_ref(column)? {
        ValueRef::Text(bytes) if bytes.len() <= max => {
            std::str::from_utf8(bytes).map_err(|_| error(Code::Storage))
        }
        _ => Err(error(Code::ResourceLimit)),
    }
}
pub(crate) fn optional_text<'a>(
    row: &'a rusqlite::Row<'_>,
    column: usize,
    max: usize,
) -> AppResult<Option<&'a str>> {
    if matches!(row.get_ref(column)?, ValueRef::Null) {
        Ok(None)
    } else {
        text(row, column, max).map(Some)
    }
}
pub(crate) fn fence(tx: &Transaction<'_>, id: Uuid) -> AppResult<DocumentFence> {
    let mut q = tx.prepare(
        "SELECT version,sha256,parser_identity,\
                    import_job_id FROM documents WHERE id=?1",
    )?;
    let mut rows = q.query([id.to_string()])?;
    let r = rows.next()?.ok_or_else(|| error(Code::Configuration))?;
    let raw = optional_text(r, 3, 36)?;
    let f = DocumentFence {
        id,
        version: r.get(0)?,
        sha256: text(r, 1, 64)?.into(),
        parser: text(r, 2, 128)?.into(),
        import_job: raw
            .map(|i| {
                let id = Uuid::parse_str(i).map_err(|_| error(Code::Storage))?;
                if id.to_string() != i {
                    return Err(error(Code::Storage));
                }
                Ok(id)
            })
            .transpose()?,
    };
    f.validate()?;
    Ok(f)
}
pub(crate) fn binding(store: &LocalStore, profile: &ModelProfile) -> AppResult<Option<Binding>> {
    let semantic = EmbeddingIdentity::from_profile(profile, profile.dimensions.unwrap_or(128))?
        .semantic_key()?;
    store.transaction(|tx| {
        let mut query = tx.prepare(
            "SELECT g.identity,g.dimension,s.seal_json,\
                    g.active_generation,g.requested_dimension \
                    FROM knowledge_index_generations g JOIN \
                    knowledge_index_generation_seals s ON \
                    s.identity=g.identity AND \
                    s.generation=g.active_generation WHERE \
                    g.semantic_key=?1 AND g.state='active' AND \
                    s.dirty=0 AND (?2 IS NULL OR g.dimension=?2) \
                    ORDER BY g.identity LIMIT 2",
        )?;
        let mut rows = query.query(params![semantic, profile.dimensions])?;
        let mut found = None;
        while let Some(r) = rows.next()? {
            let seal = Seal::decode(text(r, 2, 65536)?)?;
            if seal.descriptor.generation.to_string() != text(r, 3, 36)?
                || seal.descriptor.requested_dimension != r.get::<_, Option<u32>>(4)?
                || seal.descriptor.key()? != text(r, 0, 64)?
                || seal.descriptor.identity.dimensions != r.get::<_, u32>(1)?
                || seal.descriptor.identity.semantic_key()? != semantic
            {
                return Err(error(Code::IndexRepairRequired));
            }
            if found.is_some() {
                return Err(error(Code::IndexRepairRequired));
            }
            found = Some(Binding {
                generation: seal.descriptor,
            });
        }
        Ok(found)
    })
}
pub(crate) fn active_pointer(store: &LocalStore, identity: &str) -> AppResult<Option<Uuid>> {
    store.transaction(|tx| {
        let raw: Option<Option<String>> = tx
            .query_row(
                "SELECT active_generation FROM \
                    knowledge_index_generations WHERE identity=?1",
                [identity],
                |r| r.get(0),
            )
            .optional()?;
        raw.flatten()
            .map(|s| Uuid::parse_str(&s).map_err(|_| error(Code::Storage)))
            .transpose()
    })
}
pub(crate) fn health(
    store: &LocalStore,
    identity: &str,
    generation: Uuid,
) -> AppResult<(u32, u32)> {
    store.transaction(|tx| {
        let total: u32 = tx.query_row("SELECT count(*) FROM documents", [], |r| r.get(0))?;
        let healthy: u32 = tx.query_row(
            "SELECT count(*) FROM documents d WHERE \
                    d.status='ready' AND d.index_identity=?1 AND \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=d.id AND \
                    c.document_version=d.version) AND NOT \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=d.id AND \
                    c.document_version=d.version AND \
                    (c.index_state!='ready' OR \
                    c.embedding_identity IS NOT ?1 OR NOT \
                    EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings m WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.published=1 AND m.chunk_id=c.id AND \
                    m.document_id=d.id AND \
                    m.document_version=d.version AND \
                    m.content_hash=c.content_hash AND \
                    m.vector_id=c.vector_id)))",
            params![identity, generation.to_string()],
            |r| r.get(0),
        )?;
        Ok((healthy, total.saturating_sub(healthy)))
    })
}
/// A single surviving chunk never makes an incomplete document searchable.
pub(crate) fn ready_coverage(
    tx: &Transaction<'_>,
    id: Uuid,
    version: u32,
    identity: &str,
    generation: Uuid,
) -> AppResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents d WHERE \
                    d.id=?1 AND d.version=?2 AND \
                    d.status='ready' AND d.index_identity=?3 AND \
                    EXISTS(SELECT 1 FROM \
                    knowledge_index_generations g WHERE \
                    g.identity=?3 AND g.active_generation=?4) AND \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=d.id AND \
                    c.document_version=d.version) AND NOT \
                    EXISTS(SELECT 1 FROM document_chunks c WHERE \
                    c.document_id=d.id AND \
                    c.document_version=d.version AND \
                    (c.index_state!='ready' OR \
                    c.embedding_identity IS NOT ?3 OR NOT \
                    EXISTS(SELECT 1 FROM \
                    knowledge_generation_mappings m WHERE \
                    m.identity=?3 AND m.generation=?4 AND \
                    m.published=1 AND m.chunk_id=c.id AND \
                    m.document_id=d.id AND \
                    m.document_version=d.version AND \
                    m.content_hash=c.content_hash AND \
                    m.vector_id=c.vector_id))))",
        params![id.to_string(), version, identity, generation.to_string()],
        |r| r.get(0),
    )?)
}
pub(crate) fn mark_identity_repair(
    store: &LocalStore,
    identity: &str,
    reason: &str,
) -> AppResult<()> {
    store.transaction(|tx| {
        tx.execute(
            "UPDATE documents SET \
                    status='repair_required',\
                    error_code='index_repair_required' WHERE \
                    index_identity=?1 AND status='ready'",
            [identity],
        )?;
        tx.execute(
            "INSERT INTO knowledge_repairs(document_id,\
                    reason) SELECT id,?2 FROM documents WHERE \
                    index_identity=?1 AND \
                    status='repair_required' ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
            params![identity, reason],
        )?;
        Ok(())
    })
}
pub(crate) struct Chunk {
    pub(crate) ordinal: u32,
    pub(crate) text: String,
    pub(crate) metadata: ChunkMetadata,
    pub(crate) previous: Option<(String, u64)>,
}
pub(crate) fn chunks(
    store: &LocalStore,
    fence: &DocumentFence,
    after: i64,
) -> AppResult<Vec<Chunk>> {
    store.transaction(|tx| {
        if !fence.matches(tx, "indexing")? {
            return Err(error(Code::Cancelled));
        }
        let mut q = tx.prepare(
            "SELECT chunk_index,id,text,location_json,\
                    content_hash,embedding_identity,vector_id \
                    FROM document_chunks WHERE document_id=?1 \
                    AND document_version=?2 AND chunk_index>?3 \
                    ORDER BY chunk_index LIMIT 32",
        )?;
        let mut rows = q.query(params![fence.id.to_string(), fence.version, after])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            let body = text(r, 2, 9600)?;
            if body.chars().count() > 2400 {
                return Err(error(Code::ResourceLimit));
            }
            let hash = text(r, 4, 64)?;
            if crate::knowledge::chunk_hash(body) != hash {
                return Err(error(Code::IndexRepairRequired));
            }
            let location: TextLocation = serde_json::from_str(text(r, 3, 16 * 1024 * 1024)?)
                .map_err(|_| error(Code::Storage))?;
            let metadata = ChunkMetadata::new(
                fence.id,
                fence.version,
                Uuid::parse_str(text(r, 1, 36)?).map_err(|_| error(Code::Storage))?,
                hash.into(),
                &location,
            )?;
            let previous = match (
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ) {
                (Some(i), Some(v)) => Some((i, v.parse().map_err(|_| error(Code::Storage))?)),
                _ => None,
            };
            out.push(Chunk {
                ordinal: r.get(0)?,
                text: body.into(),
                metadata,
                previous,
            });
        }
        Ok(out)
    })
}
pub(crate) fn stage(
    store: &LocalStore,
    context: &JobContext,
    ticket: &crate::knowledge::index::WorkTicket,
    ordinal: u32,
    chunk: &Chunk,
    id: u64,
) -> AppResult<()> {
    let identity = ticket.generation().key()?;
    let generation = ticket.generation().generation.to_string();
    store.transaction(|tx| {
        context.commit(tx, || {
            tx.execute(
                "INSERT INTO \
                    knowledge_index_work_items(operation_id,\
                    ordinal,identity,generation,vector_id,\
                    document_id,document_version,chunk_id,\
                    content_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,\
                    ?8,?9)",
                params![
                    ticket.operation_id().to_string(),
                    ordinal,
                    identity,
                    generation,
                    id.to_string(),
                    chunk.metadata.document_id.to_string(),
                    chunk.metadata.document_version,
                    chunk.metadata.chunk_id.to_string(),
                    chunk.metadata.content_hash
                ],
            )?;
            let f = fence(tx, chunk.metadata.document_id)?;
            let mut q = tx.prepare("SELECT location_json FROM document_chunks WHERE id=?1")?;
            let mut rows = q.query([chunk.metadata.chunk_id.to_string()])?;
            let r = rows.next()?.ok_or_else(|| error(Code::Storage))?;
            let location: TextLocation = serde_json::from_str(text(r, 0, 16 * 1024 * 1024)?)
                .map_err(|_| error(Code::Storage))?;
            if !chunk.metadata.matches_location(&location)? {
                return Err(error(Code::IndexRepairRequired));
            }
            tx.execute(
                "UPDATE knowledge_index_work_items SET \
                    document_fence_json=?1,location_sha256=?2 \
                    WHERE operation_id=?3 AND ordinal=?4",
                params![
                    serde_json::to_string(&f).map_err(|_| error(Code::Storage))?,
                    crate::knowledge::index::location_digest(&location)?,
                    ticket.operation_id().to_string(),
                    ordinal
                ],
            )?;
            tx.execute(
                "INSERT INTO \
                    knowledge_generation_mappings(identity,\
                    generation,chunk_id,document_id,\
                    document_version,vector_id,content_hash,\
                    published) VALUES (?1,?2,?3,?4,?5,?6,?7,0) \
                    ON CONFLICT(identity,generation,chunk_id) DO \
                    UPDATE SET vector_id=excluded.vector_id,\
                    content_hash=excluded.content_hash,\
                    published=0",
                params![
                    identity,
                    generation,
                    chunk.metadata.chunk_id.to_string(),
                    chunk.metadata.document_id.to_string(),
                    chunk.metadata.document_version,
                    id.to_string(),
                    chunk.metadata.content_hash
                ],
            )?;
            Ok(())
        })
    })
}
pub(crate) enum Record {
    Healthy(ChunkMetadata, DocumentFence),
    Orphan,
    Repair(Uuid, u32, &'static str),
    Tombstone,
    Retained,
}
pub(crate) fn classify(
    store: &LocalStore,
    identity: &str,
    generation: Uuid,
    vector: u64,
    metadata: Option<&ChunkMetadata>,
) -> AppResult<Record> {
    store.transaction(|tx| {
        let mut q = tx.prepare(
            "SELECT m.document_id,m.document_version,\
                    m.chunk_id,m.content_hash,d.version,d.status,\
                    c.text,c.content_hash,c.location_json,\
                    c.vector_id,c.embedding_identity,\
                    d.index_identity,c.document_id,\
                    c.document_version,c.index_state,COALESCE(d.title,'') FROM \
                    knowledge_generation_mappings m JOIN \
                    knowledge_index_generations g ON \
                    g.identity=m.identity AND \
                    g.active_generation=m.generation LEFT JOIN \
                    document_chunks c ON c.id=m.chunk_id LEFT \
                    JOIN documents d ON d.id=m.document_id WHERE \
                    m.identity=?1 AND m.generation=?2 AND \
                    m.vector_id=?3 AND m.published=1",
        )?;
        let mut rows = q.query(params![
            identity,
            generation.to_string(),
            vector.to_string()
        ])?;
        let Some(r) = rows.next()? else {
            let retained: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_index_work_items WHERE \
                 identity=?1 AND generation=?2 AND vector_id=?3 AND document_id IS NOT NULL)",
                params![identity, generation.to_string(), vector.to_string()],
                |r| r.get(0),
            )?;
            if retained {
                return Ok(Record::Retained);
            }
            return Ok(if metadata.is_some() {
                Record::Orphan
            } else {
                Record::Tombstone
            });
        };
        let current: Option<u32> = r.get(4)?;
        let status: Option<String> = r.get(5)?;
        let version: u32 = r.get(1)?;
        if status.as_deref() == Some("deleting") {
            return Ok(Record::Retained);
        }
        if current != Some(version) || status.as_deref() != Some("ready") {
            return Ok(if metadata.is_some() {
                Record::Orphan
            } else {
                Record::Tombstone
            });
        }
        let doc = Uuid::parse_str(text(r, 0, 36)?).map_err(|_| error(Code::Storage))?;
        if !ready_coverage(tx, doc, version, identity, generation)? {
            return Ok(Record::Repair(doc, version, "vector_missing"));
        }
        if text(r, 15, 32768).is_err() {
            return Ok(Record::Repair(doc, version, "stale_hash"));
        }
        let Some(metadata) = metadata else {
            return Ok(Record::Repair(doc, version, "vector_missing"));
        };
        let location: TextLocation = match text(r, 8, 16 * 1024 * 1024)
            .and_then(|s| serde_json::from_str(s).map_err(|_| error(Code::Storage)))
        {
            Ok(l) => l,
            Err(_) => return Ok(Record::Repair(doc, version, "vector_missing")),
        };
        let valid = (|| {
            Ok::<_, crate::error::AppError>(
                metadata.document_id == doc
                    && metadata.document_version == version
                    && metadata.chunk_id.to_string() == text(r, 2, 36)?
                    && metadata.content_hash == text(r, 3, 64)?
                    && metadata.content_hash == text(r, 7, 64)?
                    && text(r, 6, 9600)?.chars().count() <= 2400
                    && crate::knowledge::chunk_hash(text(r, 6, 9600)?) == metadata.content_hash
                    && optional_text(r, 9, 20)? == Some(vector.to_string().as_str())
                    && optional_text(r, 10, 64)? == Some(identity)
                    && optional_text(r, 11, 64)? == Some(identity)
                    && optional_text(r, 12, 36)? == Some(doc.to_string().as_str())
                    && r.get::<_, Option<u32>>(13)? == Some(version)
                    && optional_text(r, 14, 32)? == Some("ready")
                    && metadata.matches_location(&location)?,
            )
        })()
        .unwrap_or(false);
        if !valid {
            return Ok(Record::Repair(doc, version, "stale_hash"));
        }
        let frozen = match fence(tx, doc) {
            Ok(f) => f,
            Err(_) => return Ok(Record::Repair(doc, version, "stale_hash")),
        };
        Ok(Record::Healthy(
            ChunkMetadata::new(
                doc,
                version,
                metadata.chunk_id,
                metadata.content_hash.clone(),
                &location,
            )?,
            frozen,
        ))
    })
}
pub(crate) fn mark_document_repair(
    store: &LocalStore,
    id: Uuid,
    version: u32,
    reason: &str,
) -> AppResult<()> {
    store.transaction(|tx| mark_document_repair_tx(tx, id, version, reason))
}
pub(crate) fn mark_document_repair_tx(
    tx: &Transaction<'_>,
    id: Uuid,
    version: u32,
    reason: &str,
) -> AppResult<()> {
    if tx.execute(
        "UPDATE documents SET \
                    status='repair_required',\
                    error_code='index_repair_required' WHERE \
                    id=?1 AND version=?2 AND status='ready'",
        params![id.to_string(), version],
    )? != 1
    {
        return Ok(());
    }
    tx.execute(
        "UPDATE document_chunks SET \
                    index_state='repair_required' WHERE \
                    document_id=?1 AND document_version=?2 AND \
                    index_state='ready'",
        params![id.to_string(), version],
    )?;
    tx.execute(
        "INSERT INTO knowledge_repairs(document_id,\
                    reason) VALUES (?1,?2) ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
        params![id.to_string(), reason],
    )?;
    Ok(())
}
