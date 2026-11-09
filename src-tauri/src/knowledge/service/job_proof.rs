//! Bounded durable domain association shared by run, startup and publication.
use super::*;
use crate::knowledge::index::DocumentFence;
use rusqlite::Transaction;

const MAX_PAYLOAD: usize = 64 * 1024;

#[derive(Clone)]
pub(crate) struct DocumentJobProof {
    id: Uuid,
    kind: JobKind,
    document: Uuid,
    version: u32,
    payload_sha256: String,
}
pub(crate) struct ReindexJob {
    pub(crate) proof: DocumentJobProof,
    pub(crate) input: IndexInput,
    pub(crate) profile: ModelProfile,
    pub(crate) status: JobStatus,
}
pub(crate) struct DeleteJob {
    pub(crate) proof: DocumentJobProof,
    pub(crate) input: DeleteInput,
    pub(crate) status: JobStatus,
}
struct StoredJob {
    id: Uuid,
    kind: JobKind,
    object: Uuid,
    status: JobStatus,
    payload: crate::intelligence::types::JobPayload,
    payload_sha256: String,
}
impl StoredJob {
    fn proof(&self, version: u32) -> DocumentJobProof {
        DocumentJobProof {
            id: self.id,
            kind: self.kind,
            document: self.object,
            version,
            payload_sha256: self.payload_sha256.clone(),
        }
    }
}
fn configuration() -> crate::error::AppError {
    error(Code::Configuration)
}
fn read(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<StoredJob>> {
    let mut q = tx.prepare(
        "SELECT kind,object_id,exclusion_key,session_id,permanent_key,\
                    payload_json,status FROM intelligence_jobs WHERE id=?1",
    )?;
    let mut rows = q.query([id.to_string()])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    let text = |column, max| state::text(r, column, max).map_err(|_| configuration());
    let object = text(1, 36)?;
    let object_id = Uuid::parse_str(object).map_err(|_| configuration())?;
    if object_id.to_string() != object
        || text(2, 160)? != format!("document:{object}")
        || state::optional_text(r, 3, 36)?.is_some()
        || state::optional_text(r, 4, 160)?.is_some()
    {
        return Err(configuration());
    }
    let kind =
        serde_json::from_value(serde_json::json!(text(0, 32)?)).map_err(|_| configuration())?;
    let status =
        serde_json::from_value(serde_json::json!(text(6, 32)?)).map_err(|_| configuration())?;
    let raw = text(5, MAX_PAYLOAD)?;
    Ok(Some(StoredJob {
        id,
        kind,
        object: object_id,
        status,
        payload: decode_payload(raw)?,
        payload_sha256: crate::knowledge::chunk_hash(raw),
    }))
}
pub(crate) fn reindex(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<ReindexJob>> {
    let Some(row) = read(tx, id)? else {
        return Ok(None);
    };
    if row.kind != JobKind::Reindex
        || row.payload.profiles.len() != 1
        || row.payload.profiles[0].kind != crate::intelligence::types::ServiceKind::Embedding
    {
        return Err(configuration());
    }
    let input: IndexInput =
        serde_json::from_value(row.payload.input.clone()).map_err(|_| configuration())?;
    if input.document_id != row.object || input.document_version == 0 {
        return Err(configuration());
    }
    Ok(Some(ReindexJob {
        proof: row.proof(input.document_version),
        input,
        profile: row.payload.profiles.into_iter().next().unwrap(),
        status: row.status,
    }))
}
pub(crate) fn delete(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<DeleteJob>> {
    let Some(row) = read(tx, id)? else {
        return Ok(None);
    };
    if row.kind != JobKind::DeleteDocument || !row.payload.profiles.is_empty() {
        return Err(configuration());
    }
    let input: DeleteInput =
        serde_json::from_value(row.payload.input.clone()).map_err(|_| configuration())?;
    input.validate()?;
    if input.document_id != row.object || input.operation_id != id {
        return Err(configuration());
    }
    Ok(Some(DeleteJob {
        proof: row.proof(input.document_version),
        input,
        status: row.status,
    }))
}
fn local_proof<T>(result: AppResult<Option<T>>) -> AppResult<Option<T>> {
    match result {
        Err(crate::error::AppError::Intelligence(code))
            if matches!(
                code,
                Code::Configuration | Code::ResourceLimit | Code::IndexRepairRequired
            ) =>
        {
            Ok(None)
        }
        other => other,
    }
}
pub(crate) fn local_reindex(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<ReindexJob>> {
    local_proof(reindex(tx, id))
}
pub(crate) fn local_delete(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<DeleteJob>> {
    local_proof(delete(tx, id))
}
impl DocumentJobProof {
    pub(crate) fn payload_hash(&self) -> &str {
        &self.payload_sha256
    }
    pub(crate) fn require(&self, tx: &Transaction<'_>, document: &DocumentFence) -> AppResult<()> {
        let row = read(tx, self.id)?.ok_or_else(configuration)?;
        if row.kind != self.kind
            || row.object != self.document
            || document.id != self.document
            || document.version != self.version
            || row.payload_sha256 != self.payload_sha256
        {
            return Err(configuration());
        }
        Ok(())
    }
}
pub(super) fn canonical_uuid<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Uuid, D::Error> {
    let raw = String::deserialize(d)?;
    let id = Uuid::parse_str(&raw).map_err(|_| serde::de::Error::custom("invalid UUID"))?;
    if id.to_string() != raw {
        return Err(serde::de::Error::custom("noncanonical UUID"));
    }
    Ok(id)
}
pub(super) fn optional_canonical_uuid<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Uuid>, D::Error> {
    let raw = Option::<String>::deserialize(d)?;
    raw.map(|raw| {
        let id = Uuid::parse_str(&raw).map_err(|_| serde::de::Error::custom("invalid UUID"))?;
        if id.to_string() != raw {
            return Err(serde::de::Error::custom("noncanonical UUID"));
        }
        Ok(id)
    })
    .transpose()
}

pub(crate) fn reconcile_indexing(store: &LocalStore) -> AppResult<()> {
    let mut after = String::new();
    loop {
        let page = store.transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT id,version FROM documents WHERE status='indexing' AND \
                    id>?1 ORDER BY id LIMIT 200",
            )?;
            let mut rows = q.query([&after])?;
            let mut out = Vec::new();
            while let Some(r) = rows.next()? {
                out.push((state::text(r, 0, 36)?.to_owned(), r.get::<_, u32>(1)?));
            }
            Ok(out)
        })?;
        if page.is_empty() {
            return Ok(());
        }
        for (raw, version) in page {
            after = raw.clone();
            let document = Uuid::parse_str(&raw).map_err(|_| error(Code::Storage))?;
            store.transaction(|tx| {
                let ids = {
                    let mut q = tx.prepare(
                        "SELECT id FROM intelligence_jobs WHERE object_id=?1 AND \
                    kind='reindex' AND status IN ('queued','running') LIMIT 33",
                    )?;
                    let mut rows = q.query([&raw])?;
                    let mut out = Vec::new();
                    while let Some(r) = rows.next()? {
                        out.push(state::text(r, 0, 36).ok().and_then(|raw_job| {
                            Uuid::parse_str(raw_job)
                                .ok()
                                .filter(|id| id.to_string() == raw_job)
                        }));
                    }
                    out
                };
                let mut owned = false;
                if ids.len() <= crate::intelligence::store::MAX_UNFINISHED {
                    for id in ids.into_iter().flatten() {
                        if local_reindex(tx, id)?.is_some_and(|job| {
                            job.input.document_id == document
                                && job.input.document_version == version
                                && matches!(job.status, JobStatus::Queued | JobStatus::Running)
                        }) {
                            owned = true;
                            break;
                        }
                    }
                }
                if owned {
                    return Ok(());
                }
                if tx.execute(
                    "UPDATE documents SET status='repair_required',\
                    error_code='index_repair_required' WHERE id=?1 AND \
                    version=?2 AND status='indexing'",
                    params![raw, version],
                )? == 1
                {
                    tx.execute(
                        "UPDATE document_chunks SET index_state='repair_required' WHERE \
                    document_id=?1 AND document_version=?2 AND index_state='indexing'",
                        params![raw, version],
                    )?;
                    tx.execute(
                        "INSERT INTO knowledge_repairs(document_id,reason) \
                    VALUES (?1,'unsealed_generation') ON CONFLICT(document_id) \
                    DO UPDATE SET reason=excluded.reason",
                        [&raw],
                    )?;
                }
                Ok(())
            })?;
        }
    }
}
