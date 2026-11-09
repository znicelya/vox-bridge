//! Complete, bounded deletion evidence, checked before an engine can mutate.
use super::{DocumentFence, codec::is_hash, repair};
use crate::{
    error::AppResult,
    knowledge::service::{DeleteInput, state},
};
use rusqlite::{Transaction, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) const PAGE_SIZE: u32 = 200;
const MAX_WORK: u32 = 200_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeleteWorkProof {
    pub(crate) count: u32,
    pub(crate) root: String,
    pub(crate) pages: Vec<String>,
    pub(crate) association_pages: Vec<String>,
}
pub(crate) struct WorkRow {
    pub(crate) ordinal: u32,
    identity: String,
    generation: Uuid,
    pub(crate) vector: u64,
    document: Uuid,
    version: u32,
    chunk: Uuid,
    hash: String,
    fence_json: String,
    fence: DocumentFence,
}
fn uuid(raw: &str) -> AppResult<Uuid> {
    let id = Uuid::parse_str(raw).map_err(|_| repair())?;
    if id.to_string() != raw {
        return Err(repair());
    }
    Ok(id)
}
fn read(r: &rusqlite::Row<'_>, input: &DeleteInput) -> AppResult<WorkRow> {
    let text = |column, max| state::text(r, column, max).map_err(|_| repair());
    let identity = text(1, 64)?;
    let generation = uuid(text(2, 36)?)?;
    let vector_raw = text(3, 20)?;
    let vector: u64 = vector_raw.parse().map_err(|_| repair())?;
    let document = uuid(text(4, 36)?)?;
    let version: u32 = r.get(5).map_err(|_| repair())?;
    let chunk = uuid(text(6, 36)?)?;
    let hash = text(7, 64)?;
    let fence_json = text(8, 1024)?;
    let fence: DocumentFence = serde_json::from_str(fence_json).map_err(|_| repair())?;
    fence.validate()?;
    if input.identity.as_deref() != Some(identity)
        || input.generation != Some(generation)
        || document != input.document_id
        || version == 0
        || version > input.document_version
        || vector.to_string() != vector_raw
        || !is_hash(hash)
        || fence != input.document
        || state::optional_text(r, 9, 64)
            .map_err(|_| repair())?
            .is_some()
        || state::optional_text(r, 10, 20)
            .map_err(|_| repair())?
            .is_some()
    {
        return Err(repair());
    }
    Ok(WorkRow {
        ordinal: r.get(0).map_err(|_| repair())?,
        identity: identity.into(),
        generation,
        vector,
        document,
        version,
        chunk,
        hash: hash.into(),
        fence_json: fence_json.into(),
        fence,
    })
}
impl WorkRow {
    pub(crate) fn mapping(&self, tx: &Transaction<'_>) -> AppResult<bool> {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_generation_mappings WHERE \
             identity=?1 AND generation=?2 AND vector_id=?3 AND document_id=?4 AND \
             document_version=?5 AND chunk_id=?6 AND content_hash=?7 AND published=1)",
            params![
                self.identity,
                self.generation.to_string(),
                self.vector.to_string(),
                self.document.to_string(),
                self.version,
                self.chunk.to_string(),
                self.hash
            ],
            |r| r.get(0),
        )?)
    }
    fn current_chunk(&self, tx: &Transaction<'_>) -> AppResult<bool> {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM document_chunks WHERE id=?1 AND document_id=?2 \
             AND document_version=?3 AND content_hash=?4 AND vector_id=?5 \
             AND embedding_identity=?6 AND index_state='deleting')",
            params![
                self.chunk.to_string(),
                self.document.to_string(),
                self.version,
                self.hash,
                self.vector.to_string(),
                self.identity
            ],
            |r| r.get(0),
        )?)
    }
    pub(crate) fn conflicting_mapping(&self, tx: &Transaction<'_>) -> AppResult<bool> {
        Ok(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_generation_mappings \
            WHERE identity=?1 AND generation=?2 AND (vector_id=?3 OR chunk_id=?4))",
            params![
                self.identity,
                self.generation.to_string(),
                self.vector.to_string(),
                self.chunk.to_string()
            ],
            |r| r.get(0),
        )?)
    }
    pub(crate) fn acknowledge(
        &self,
        tx: &Transaction<'_>,
        operation: Uuid,
        proved_absent: bool,
    ) -> AppResult<()> {
        let removed = tx.execute(
            "DELETE FROM knowledge_generation_mappings WHERE identity=?1 AND generation=?2 \
             AND vector_id=?3 AND document_id=?4 AND document_version=?5 AND chunk_id=?6 \
             AND content_hash=?7 AND published=1",
            params![
                self.identity,
                self.generation.to_string(),
                self.vector.to_string(),
                self.document.to_string(),
                self.version,
                self.chunk.to_string(),
                self.hash
            ],
        )?;
        if removed != 1 && !(removed == 0 && proved_absent) {
            return Err(repair());
        }
        if tx.execute("DELETE FROM knowledge_index_work_items WHERE operation_id=?1 AND ordinal=?2 \
            AND identity=?3 AND generation=?4 AND vector_id=?5 AND document_id=?6 AND document_version=?7 \
            AND chunk_id=?8 AND content_hash=?9 AND document_fence_json=?10 AND location_sha256 IS NULL \
            AND previous_vector_id IS NULL",
            params![operation.to_string(), self.ordinal, self.identity, self.generation.to_string(),
                self.vector.to_string(), self.document.to_string(), self.version, self.chunk.to_string(),
                self.hash, self.fence_json])? != 1 { return Err(repair()) }
        Ok(())
    }
}
pub(crate) fn require_document(tx: &Transaction<'_>, input: &DeleteInput) -> AppResult<()> {
    if !input.document.matches(tx, "deleting")? {
        return Err(repair());
    }
    let association: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1 AND index_identity IS ?2)",
        params![input.document_id.to_string(), input.identity],
        |r| r.get(0),
    )?;
    if !association {
        return Err(repair());
    }
    Ok(())
}
fn counts(tx: &Transaction<'_>, input: &DeleteInput) -> AppResult<u32> {
    let (count, distinct): (u64, u64) = tx.query_row(
        "SELECT count(*),count(DISTINCT chunk_id) FROM knowledge_index_work_items WHERE operation_id=?1",
        [input.operation_id.to_string()], |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if count > u64::from(MAX_WORK) || count != distinct {
        return Err(repair());
    }
    Ok(count as u32)
}
pub(crate) fn rows(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    page: u32,
    live: bool,
) -> AppResult<Vec<WorkRow>> {
    read_rows(tx, input, page, live, true)
}
pub(crate) fn certified_rows(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    page: u32,
) -> AppResult<Vec<WorkRow>> {
    read_rows(tx, input, page, false, false)
}
fn read_rows(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    page: u32,
    live: bool,
    require_mapping: bool,
) -> AppResult<Vec<WorkRow>> {
    let start = page
        .checked_mul(PAGE_SIZE)
        .filter(|n| *n < MAX_WORK)
        .ok_or_else(repair)?;
    let mut q = tx.prepare(
        "SELECT ordinal,identity,generation,vector_id,document_id,document_version,chunk_id,\
         content_hash,document_fence_json,location_sha256,previous_vector_id FROM \
         knowledge_index_work_items WHERE operation_id=?1 AND ordinal>=?2 AND ordinal<?3 \
         ORDER BY ordinal LIMIT 200",
    )?;
    let mut found = q.query(params![
        input.operation_id.to_string(),
        start,
        start + PAGE_SIZE
    ])?;
    let mut out = Vec::new();
    while let Some(r) = found.next()? {
        let row = read(r, input)?;
        if row.ordinal != start + out.len() as u32
            || (require_mapping && !row.mapping(tx)?)
            || (live && row.version == input.document_version && !row.current_chunk(tx)?)
        {
            return Err(repair());
        }
        out.push(row);
    }
    Ok(out)
}
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
pub(crate) fn page_hash(rows: &[WorkRow]) -> String {
    let mut hash = Sha256::new();
    field(&mut hash, b"voxbridge-delete-page-v1");
    field(&mut hash, &(rows.len() as u32).to_le_bytes());
    for row in rows {
        field(&mut hash, &row.ordinal.to_le_bytes());
        field(&mut hash, row.identity.as_bytes());
        field(&mut hash, row.generation.as_bytes());
        field(&mut hash, &row.vector.to_le_bytes());
        field(&mut hash, row.document.as_bytes());
        field(&mut hash, &row.version.to_le_bytes());
        field(&mut hash, row.chunk.as_bytes());
        field(&mut hash, row.hash.as_bytes());
        field(&mut hash, row.fence_json.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
pub(crate) fn association_page_hash(rows: &[WorkRow]) -> AppResult<String> {
    // Raw page_hash remains each operation's immutable original evidence.
    // Peers share complete typed associations, including the canonical fence,
    // rather than requiring identical valid JSON whitespace.
    let mut hash = Sha256::new();
    field(&mut hash, b"voxbridge-delete-semantic-page-v1");
    field(&mut hash, &(rows.len() as u32).to_le_bytes());
    for row in rows {
        field(&mut hash, &row.ordinal.to_le_bytes());
        field(&mut hash, row.identity.as_bytes());
        field(&mut hash, row.generation.as_bytes());
        field(&mut hash, &row.vector.to_le_bytes());
        field(&mut hash, row.document.as_bytes());
        field(&mut hash, &row.version.to_le_bytes());
        field(&mut hash, row.chunk.as_bytes());
        field(&mut hash, row.hash.as_bytes());
        field(
            &mut hash,
            &serde_json::to_vec(&row.fence).map_err(|_| repair())?,
        );
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(crate) fn capture(tx: &Transaction<'_>, input: &DeleteInput) -> AppResult<DeleteWorkProof> {
    require_document(tx, input)?;
    let count = counts(tx, input)?;
    let mappings: u64 = tx.query_row(
        "SELECT count(*) FROM knowledge_generation_mappings WHERE document_id=?1 \
         AND identity IS ?2 AND generation IS ?3 AND published=1",
        params![
            input.document_id.to_string(),
            input.identity,
            input.generation.map(|id| id.to_string())
        ],
        |r| r.get(0),
    )?;
    if mappings != u64::from(count) {
        return Err(repair());
    }
    let missing_current: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM document_chunks c WHERE document_id=?1 \
         AND document_version=?2 AND vector_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM \
         knowledge_generation_mappings m WHERE m.identity IS ?3 AND m.generation IS ?4 \
         AND m.chunk_id=c.id AND m.document_id=c.document_id AND m.document_version=c.document_version \
         AND m.content_hash=c.content_hash AND m.vector_id=c.vector_id AND m.published=1))",
        params![input.document_id.to_string(), input.document_version, input.identity,
            input.generation.map(|id| id.to_string())], |r| r.get(0),
    )?;
    if missing_current {
        return Err(repair());
    }
    fingerprint(tx, input, count, true)
}
fn fingerprint(
    tx: &Transaction<'_>,
    input: &DeleteInput,
    count: u32,
    live: bool,
) -> AppResult<DeleteWorkProof> {
    let mut pages = Vec::new();
    let mut association_pages = Vec::new();
    for page in 0..count.div_ceil(PAGE_SIZE) {
        let rows = rows(tx, input, page, live)?;
        if rows.len() as u32 != (count - page * PAGE_SIZE).min(PAGE_SIZE) {
            return Err(repair());
        }
        pages.push(page_hash(&rows));
        association_pages.push(association_page_hash(&rows)?);
    }
    from_pages(input, count, pages, association_pages)
}
pub(crate) fn from_pages(
    input: &DeleteInput,
    count: u32,
    pages: Vec<String>,
    association_pages: Vec<String>,
) -> AppResult<DeleteWorkProof> {
    if count > MAX_WORK
        || pages.len() != count.div_ceil(PAGE_SIZE) as usize
        || pages.iter().any(|p| !is_hash(p))
        || association_pages.len() != pages.len()
        || association_pages.iter().any(|p| !is_hash(p))
    {
        return Err(repair());
    }
    let mut hash = Sha256::new();
    field(&mut hash, b"voxbridge-delete-work-v1");
    let encoded = serde_json::to_vec(input).map_err(|_| repair())?;
    field(&mut hash, &encoded);
    field(&mut hash, &count.to_le_bytes());
    for page in &pages {
        field(&mut hash, page.as_bytes())
    }
    Ok(DeleteWorkProof {
        count,
        root: format!("{:x}", hash.finalize()),
        pages,
        association_pages,
    })
}
impl DeleteWorkProof {
    pub(crate) fn shared_root(&self, input: &DeleteInput) -> AppResult<String> {
        let encoded = serde_json::to_vec(&(
            &input.document_id,
            input.document_version,
            &input.document,
            &input.identity,
            input.generation,
        ))
        .map_err(|_| repair())?;
        let mut hash = Sha256::new();
        field(&mut hash, b"voxbridge-delete-shared-associations-v1");
        field(&mut hash, &encoded);
        field(&mut hash, &self.count.to_le_bytes());
        for page in &self.association_pages {
            field(&mut hash, page.as_bytes())
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    pub(crate) fn require_live(&self, tx: &Transaction<'_>, input: &DeleteInput) -> AppResult<()> {
        if capture(tx, input)? != *self {
            return Err(repair());
        }
        Ok(())
    }
    pub(crate) fn page(
        &self,
        tx: &Transaction<'_>,
        input: &DeleteInput,
        page: u32,
        live: bool,
    ) -> AppResult<Vec<WorkRow>> {
        if live {
            require_document(tx, input)?
        }
        let rows = rows(tx, input, page, live)?;
        if rows.len() as u32
            != (self
                .count
                .checked_sub(page * PAGE_SIZE)
                .ok_or_else(repair)?)
            .min(PAGE_SIZE)
            || self.pages.get(page as usize) != Some(&page_hash(&rows))
        {
            return Err(repair());
        }
        Ok(rows)
    }
}
