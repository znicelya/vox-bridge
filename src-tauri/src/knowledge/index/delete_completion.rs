//! Explicit durable completion, independent of changing destination seals.
mod files;
mod journal;
mod publication;
mod retirement;

pub(crate) use files::{
    ManagedDirectory, check_collection, check_files, guard_managed_cleanup, mark_files_cleaned,
    resolve_directory,
};
pub(crate) use journal::{
    CompletionKind, load, operations, optional_publication, publication as read_publication,
    resume_input, scan_operations, scan_publications,
};
pub(crate) use publication::{preflight, publish_delete, publish_fresh};
#[cfg(test)]
pub(crate) use retirement::acknowledge;
pub(super) use retirement::mark_source_removed;
pub(crate) use retirement::{
    acknowledge_batch, collect, collection_ready, consume, guard_physical, guard_sql,
    retire_writer_work, settle_local, validate_complete,
};

use super::{Generation, delete_work, repair};
use crate::{
    error::AppResult,
    knowledge::service::{DeleteInput, job_proof, state},
};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const PREFIX: &str = "knowledge.delete.v1";
const VALUE_CAP: usize = 64 * 1024;
const ANCHOR_PAGE: u64 = 200;
fn key(kind: &str, id: Uuid) -> String {
    format!("{PREFIX}.{kind}.{id}")
}
fn page_key(kind: &str, id: Uuid, page: u64) -> String {
    format!("{}.{}", key(kind, id), format_args!("{page:020}"))
}
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn finish(hash: Sha256) -> String {
    format!("{:x}", hash.finalize())
}
fn hash(value: &impl Serialize) -> AppResult<String> {
    Ok(super::seal::digest(encode(value)?))
}
fn encode(value: &impl Serialize) -> AppResult<String> {
    let raw = serde_json::to_string(value).map_err(|_| repair())?;
    if raw.len() > VALUE_CAP {
        return Err(repair());
    }
    Ok(raw)
}
fn read<T: serde::de::DeserializeOwned + Serialize>(
    tx: &Transaction<'_>,
    key: &str,
) -> AppResult<Option<T>> {
    if key.len() > 160 || !key.starts_with(PREFIX) {
        return Err(repair());
    }
    let mut q = tx.prepare("SELECT value_json FROM intelligence_meta WHERE key=?1")?;
    let mut rows = q.query([key])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    let raw = state::text(r, 0, VALUE_CAP).map_err(|_| repair())?;
    let value: T = serde_json::from_str(raw).map_err(|_| repair())?;
    if encode(&value)? != raw {
        return Err(repair());
    }
    Ok(Some(value))
}
fn insert(tx: &Transaction<'_>, key: &str, value: &impl Serialize) -> AppResult<()> {
    tx.execute(
        "INSERT INTO intelligence_meta(key,value_json) VALUES (?1,?2)",
        params![key, encode(value)?],
    )?;
    Ok(())
}
fn update(tx: &Transaction<'_>, key: &str, value: &impl Serialize) -> AppResult<()> {
    if tx.execute(
        "UPDATE intelligence_meta SET value_json=?2 WHERE key=?1",
        params![key, encode(value)?],
    )? != 1
    {
        return Err(repair());
    }
    Ok(())
}
fn present_document(tx: &Transaction<'_>, document: Uuid) -> AppResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
        [document.to_string()],
        |r| r.get(0),
    )?)
}
fn work_count(tx: &Transaction<'_>, operation: Uuid) -> AppResult<u64> {
    Ok(tx.query_row(
        "SELECT count(*) FROM knowledge_index_work_items WHERE operation_id=?1",
        [operation.to_string()],
        |r| r.get(0),
    )?)
}
