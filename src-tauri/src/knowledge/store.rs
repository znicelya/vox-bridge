//! Canonical document metadata. Reading never dereferences a legacy URI or writes
//! a title, timestamp, status, or managed file as a presentation fallback.
use super::{
    protocol::NativePath,
    types::{DocumentDetail, DocumentPage, DocumentSummary},
};
use crate::{
    error::{AppError, AppResult},
    intelligence::types::{IntelligenceErrorCode as Code, RecordOrigin},
    storage::LocalStore,
};
use rusqlite::{Row, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) const MAX_PATH_BYTES: usize = 32 * 1024;
pub(super) const MAX_PATH_JSON_BYTES: usize = 256 * 1024;
const MAX_FIELD_BYTES: usize = 32 * 1024;
const COLUMNS: &str = "d.id,d.title,d.kind,d.version,d.origin,d.sha256,d.status,d.created_at,d.updated_at_ms,d.error_code,(SELECT count(*) FROM document_chunks c WHERE c.document_id=d.id AND (d.origin='legacy' OR c.document_version=d.version)),d.original_path,d.managed_path,d.uri,d.index_identity";

pub(super) fn get(store: &LocalStore, id: Uuid) -> AppResult<Option<DocumentDetail>> {
    store
        .transaction(|tx| {
            let mut statement =
                tx.prepare(&format!("SELECT {COLUMNS} FROM documents d WHERE d.id=?1"))?;
            let mut rows = statement.query([id.to_string()])?;
            rows.next()?.map(|row| read_detail(row, 0)).transpose()
        })
        .map_err(safe)
}
pub(super) fn page(store: &LocalStore, after: Option<i64>, limit: u32) -> AppResult<DocumentPage> {
    if !(1..=200).contains(&limit) || after.is_some_and(|value| value < 0) {
        return Err(error(Code::Configuration));
    }
    store.transaction(|tx| {
        let mut statement=tx.prepare(&format!("SELECT d.rowid,{COLUMNS} FROM documents d WHERE d.rowid>?1 ORDER BY d.rowid LIMIT ?2"))?;
        let mut rows=statement.query(params![after.unwrap_or(0),limit+1])?;
        let mut values=Vec::new();let mut cursor=None;
        while let Some(row)=rows.next()? {
            if values.len()==limit as usize { return Ok(DocumentPage {rows:values,next_cursor:cursor}); }
            cursor=Some(row.get(0)?);values.push(read_summary(row,1)?);
        }
        Ok(DocumentPage {rows:values,next_cursor:None})
    }).map_err(safe)
}
fn read_summary(row: &Row<'_>, offset: usize) -> AppResult<DocumentSummary> {
    let id = Uuid::parse_str(&text(row, offset, 36)?).map_err(|_| error(Code::Storage))?;
    Ok(DocumentSummary {
        id,
        title: optional_text(row, offset + 1, MAX_FIELD_BYTES)?,
        kind: read_enum(row, offset + 2)?,
        version: row.get(offset + 3)?,
        origin: read_enum(row, offset + 4)?,
        sha256: text(row, offset + 5, MAX_FIELD_BYTES)?,
        status: read_enum(row, offset + 6)?,
        created_at_ms: row.get(offset + 7)?,
        updated_at_ms: row.get(offset + 8)?,
        error: optional_text(row, offset + 9, 128)?
            .map(|value| decode_enum(&value))
            .transpose()?,
        chunk_count: row.get(offset + 10)?,
    })
}
fn read_detail(row: &Row<'_>, offset: usize) -> AppResult<DocumentDetail> {
    let summary = read_summary(row, offset)?;
    let original_path = optional_text(row, offset + 11, MAX_PATH_JSON_BYTES)?
        .map(|value| decode_path(&value))
        .transpose()?;
    let managed_path = optional_text(row, offset + 12, MAX_PATH_JSON_BYTES)?
        .map(|value| decode_path(&value))
        .transpose()?;
    let legacy_uri = if summary.origin == RecordOrigin::Legacy {
        Some(text(row, offset + 13, MAX_FIELD_BYTES)?)
    } else {
        None
    };
    Ok(DocumentDetail {
        summary,
        original_path,
        managed_path,
        legacy_uri,
        index_identity: optional_text(row, offset + 14, MAX_FIELD_BYTES)?,
    })
}
fn read_enum<T: serde::de::DeserializeOwned>(row: &Row<'_>, index: usize) -> AppResult<T> {
    decode_enum(&text(row, index, 128)?)
}
fn decode_enum<T: serde::de::DeserializeOwned>(value: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|_| error(Code::Storage))
}
fn optional_text(row: &Row<'_>, index: usize, cap: usize) -> AppResult<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Text(bytes) if bytes.len() <= cap => Ok(Some(
            std::str::from_utf8(bytes)
                .map_err(|_| error(Code::Storage))?
                .to_owned(),
        )),
        ValueRef::Text(_) => Err(error(Code::ResourceLimit)),
        _ => Err(error(Code::Storage)),
    }
}
fn text(row: &Row<'_>, index: usize, cap: usize) -> AppResult<String> {
    optional_text(row, index, cap)?.ok_or_else(|| error(Code::Storage))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPath {
    version: u32,
    native: NativePath,
}
pub(super) fn encode_path(path: &Path) -> AppResult<String> {
    let public = path
        .to_str()
        .ok_or_else(|| error(Code::UnsupportedDocument))?;
    if public.is_empty() || public.len() > MAX_PATH_BYTES || !path.is_absolute() {
        return Err(error(Code::UnsupportedDocument));
    }
    let value = serde_json::to_string(&StoredPath {
        version: 1,
        native: NativePath::from_path(path.to_owned()),
    })
    .map_err(|_| error(Code::Storage))?;
    if value.len() > MAX_PATH_JSON_BYTES {
        return Err(error(Code::ResourceLimit));
    }
    Ok(value)
}
pub(super) fn decode_path(value: &str) -> AppResult<PathBuf> {
    let value: StoredPath = serde_json::from_str(value).map_err(|_| error(Code::Storage))?;
    if value.version != 1 {
        return Err(error(Code::Storage));
    }
    let path = value.native.into_path().map_err(|_| error(Code::Storage))?;
    if !path.is_absolute()
        || path
            .to_str()
            .is_none_or(|text| text.is_empty() || text.len() > MAX_PATH_BYTES)
    {
        return Err(error(Code::Storage));
    }
    Ok(path)
}
pub(super) fn safe(value: AppError) -> AppError {
    crate::intelligence::store::safe_store_error(value)
}
pub(super) fn error(code: Code) -> AppError {
    AppError::Intelligence(code)
}
