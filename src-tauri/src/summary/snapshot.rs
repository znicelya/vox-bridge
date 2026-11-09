//! A whole, insertion-ordered bounded snapshot; raw metadata stays in SQLite.
use super::{
    job_proof::SummaryJobProof,
    store::{error, storage, text, uuid},
};
use crate::{
    domain::transcript::TranscriptKind,
    error::{AppError, AppResult},
    intelligence::{jobs::OperationLease, types::IntelligenceErrorCode as Code},
    storage::{LocalStore, SummarySnapshotReadScope},
};
use rusqlite::{Transaction, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    io::{self, Write},
    sync::Arc,
};
use tokio::sync::watch;
use uuid::Uuid;
const INPUT_BYTES: usize = 16 * 1024 * 1024;
const METADATA_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Default, Serialize)]
pub(crate) struct SnapshotIdentity {
    pub(crate) provider: Option<String>,
    pub(crate) provider_config_id: Option<Uuid>,
    pub(crate) stream_epoch: Option<u64>,
    pub(crate) sentence_id: Option<String>,
    pub(crate) local_correlation_id: Option<u64>,
    pub(crate) provider_timestamp_ms: Option<u64>,
    pub(crate) start_ms: Option<u64>,
    pub(crate) end_ms: Option<u64>,
}
pub(crate) struct SnapshotRow {
    pub(crate) rowid: i64,
    pub(crate) id: Uuid,
    pub(crate) direction: String,
    pub(crate) sequence: i64,
    pub(crate) occurred_at_seconds: i64,
    pub(crate) occurred_at_ms: Option<i64>,
    pub(crate) language: String,
    pub(crate) text: String,
    pub(crate) kind: Option<TranscriptKind>,
    pub(crate) identity: SnapshotIdentity,
}
pub(crate) struct FinalSnapshot {
    pub(crate) session_id: Uuid,
    pub(crate) last_rowid: Option<i64>,
    pub(crate) rows: Vec<SnapshotRow>,
    pub(crate) input_final_count: u32,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Retained by the fixed snapshot shape; production enforces this count during normalization"
        )
    )]
    pub(crate) normalized_bytes: usize,
    pub(crate) input_hash: String,
}
impl FinalSnapshot {
    pub(crate) async fn capture(
        store: Arc<LocalStore>,
        proof: SummaryJobProof,
        owner: OperationLease,
        cancel: watch::Receiver<bool>,
    ) -> AppResult<Self> {
        store
            .with_summary_snapshot_owned(owner, cancel, move |tx, scope| capture(tx, scope, &proof))
            .await
    }
}
#[derive(Default, Deserialize)]
struct Metadata<'a> {
    kind: Option<TranscriptKind>,
    #[serde(borrow)]
    provider: Option<Cow<'a, str>>,
    #[serde(borrow)]
    provider_config_id: Option<Cow<'a, str>>,
    stream_epoch: Option<u64>,
    #[serde(borrow)]
    sentence_id: Option<Cow<'a, str>>,
    local_correlation_id: Option<u64>,
    provider_timestamp_ms: Option<u64>,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
}
struct CountHash<'a> {
    bytes: usize,
    hash: Sha256,
    scope: &'a SummarySnapshotReadScope,
    failure: Option<Code>,
}
impl Write for CountHash<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(err) = self.scope.check() {
            self.failure = Some(match err {
                AppError::Intelligence(code) => code,
                _ => Code::Storage,
            });
            return Err(io::Error::other("snapshot interrupted"));
        }
        self.bytes = match self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= INPUT_BYTES)
        {
            Some(n) => n,
            None => {
                self.failure = Some(Code::ContextTooLarge);
                return Err(io::Error::other("snapshot limit"));
            }
        };
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl CountHash<'_> {
    fn io<T>(&mut self, result: io::Result<T>) -> AppResult<T> {
        result.map_err(|_| error(self.failure.unwrap_or(Code::Storage)))
    }
}
// Standard JSON escaping, in fixed stack chunks. Counts and SHA are streamed
// once; even a huge quoted row is rejected before creating an owned row/string.
fn quoted(w: &mut CountHash<'_>, raw: &str) -> io::Result<()> {
    w.write_all(b"\"")?;
    let mut chunk = [0u8; 8192];
    let mut used = 0;
    for b in raw.bytes() {
        let mut escaped = [0u8; 6];
        let part: &[u8] = match b {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            8 => b"\\b",
            12 => b"\\f",
            0..=31 => {
                escaped[0] = b'\\';
                escaped[1] = b'u';
                escaped[2] = b'0';
                escaped[3] = b'0';
                escaped[4] = b"0123456789abcdef"[(b >> 4) as usize];
                escaped[5] = b"0123456789abcdef"[(b & 15) as usize];
                &escaped
            }
            _ => {
                escaped[0] = b;
                &escaped[..1]
            }
        };
        if used + part.len() > chunk.len() {
            w.write_all(&chunk[..used])?;
            used = 0;
        }
        chunk[used..used + part.len()].copy_from_slice(part);
        used += part.len();
    }
    w.write_all(&chunk[..used])?;
    w.write_all(b"\"")
}
fn optional_string(w: &mut CountHash<'_>, v: Option<&str>) -> io::Result<()> {
    match v {
        Some(v) => quoted(w, v),
        None => w.write_all(b"null"),
    }
}
fn scalar(w: &mut CountHash<'_>, v: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(w, v).map_err(io::Error::other)
}
fn input_text<'a>(row: &'a rusqlite::Row<'_>, column: usize) -> AppResult<&'a str> {
    text(row, column, INPUT_BYTES).map_err(|err| match err {
        AppError::Intelligence(Code::ResponseTooLarge) => error(Code::ContextTooLarge),
        other => other,
    })
}
fn write_row(
    w: &mut CountHash<'_>,
    rowid: i64,
    id: Uuid,
    direction: &str,
    sequence: i64,
    seconds: i64,
    millis: Option<i64>,
    language: &str,
    raw_text: &str,
    meta: &Metadata<'_>,
    config: Option<Uuid>,
) -> io::Result<()> {
    write!(w, "[{rowid},")?;
    quoted(w, &id.to_string())?;
    w.write_all(b",")?;
    quoted(w, direction)?;
    write!(w, ",{sequence},{seconds},")?;
    scalar(w, &millis)?;
    w.write_all(b",")?;
    scalar(w, &meta.kind)?;
    w.write_all(b",")?;
    quoted(w, language)?;
    w.write_all(b",")?;
    quoted(w, raw_text)?;
    w.write_all(b",[")?;
    optional_string(w, meta.provider.as_deref())?;
    w.write_all(b",")?;
    scalar(w, &config)?;
    w.write_all(b",")?;
    scalar(w, &meta.stream_epoch)?;
    w.write_all(b",")?;
    optional_string(w, meta.sentence_id.as_deref())?;
    w.write_all(b",")?;
    scalar(w, &meta.local_correlation_id)?;
    w.write_all(b",")?;
    scalar(w, &meta.provider_timestamp_ms)?;
    w.write_all(b",")?;
    scalar(w, &meta.start_ms)?;
    w.write_all(b",")?;
    scalar(w, &meta.end_ms)?;
    w.write_all(b"]]")
}
#[allow(clippy::too_many_arguments)]
fn capture(
    tx: &Transaction<'_>,
    scope: &SummarySnapshotReadScope,
    proof: &SummaryJobProof,
) -> AppResult<FinalSnapshot> {
    scope.check()?;
    let session_id = proof.session_id();
    // First indexed session read pins the Deferred snapshot, followed by a
    // fresh trusted-ID proof within that same snapshot.
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1)",
        [session_id.to_string()],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(storage());
    }
    proof.running_non_deleting_in_tx(tx)?;
    scope.check()?;
    let last_rowid: Option<i64> = tx.query_row(
        "SELECT MAX(rowid) FROM translation_segments WHERE session_id=?1 AND is_final=1",
        [session_id.to_string()],
        |r| r.get(0),
    )?;
    let mut writer = CountHash {
        bytes: 0,
        hash: Sha256::new(),
        scope,
        failure: None,
    };
    let result = (|| -> io::Result<()> {
        writer.write_all(b"{\"format\":\"voxbridge.summary-input.v1\",\"session_id\":")?;
        quoted(&mut writer, &session_id.to_string())?;
        writer.write_all(b",\"rows\":[")
    })();
    writer.io(result)?;
    let mut owned = Vec::new();
    let mut metadata_bytes = 0usize;
    if let Some(high) = last_rowid {
        let mut q=tx.prepare("SELECT rowid,id,session_id,direction,sequence,occurred_at,language,text,is_final,details_json,occurred_at_ms FROM translation_segments WHERE session_id=?1 AND is_final=1 AND rowid<=?2 ORDER BY rowid LIMIT 100001")?;
        let mut rows = q.query(params![session_id.to_string(), high])?;
        while let Some(row) = rows.next()? {
            scope.check()?;
            if owned.len() == 100000 {
                return Err(error(Code::ContextTooLarge));
            }
            let rowid: i64 = row.get(0)?;
            let id = uuid(text(row, 1, 36)?)?;
            if uuid(text(row, 2, 36)?)? != session_id || row.get::<_, i64>(8)? != 1 || rowid > high
            {
                return Err(storage());
            }
            let direction = input_text(row, 3)?;
            let sequence: i64 = row.get(4)?;
            let seconds: i64 = row.get(5)?;
            let language = input_text(row, 6)?;
            let raw_text = input_text(row, 7)?;
            let millis: Option<i64> = row.get(10)?;
            let metadata = match row.get_ref(9)? {
                ValueRef::Null => Metadata::default(),
                ValueRef::Text(raw) => {
                    scope.check()?;
                    metadata_bytes = metadata_bytes
                        .checked_add(raw.len())
                        .filter(|n| *n <= METADATA_BYTES)
                        .ok_or_else(|| error(Code::ResourceLimit))?;
                    let raw = std::str::from_utf8(raw).map_err(|_| storage())?;
                    serde_json::from_str::<Metadata<'_>>(raw).map_err(|_| storage())?
                }
                _ => return Err(storage()),
            };
            scope.check()?;
            let config = metadata
                .provider_config_id
                .as_deref()
                .map(uuid)
                .transpose()?;
            let result = (|| -> io::Result<()> {
                if !owned.is_empty() {
                    writer.write_all(b",")?;
                }
                write_row(
                    &mut writer,
                    rowid,
                    id,
                    direction,
                    sequence,
                    seconds,
                    millis,
                    language,
                    raw_text,
                    &metadata,
                    config,
                )
            })();
            writer.io(result)?;
            // All borrowed lengths, type/UTF-8 and escaped charges passed first.
            owned.push(SnapshotRow {
                rowid,
                id,
                direction: direction.to_owned(),
                sequence,
                occurred_at_seconds: seconds,
                occurred_at_ms: millis,
                language: language.to_owned(),
                text: raw_text.to_owned(),
                kind: metadata.kind,
                identity: SnapshotIdentity {
                    provider: metadata.provider.map(Cow::into_owned),
                    provider_config_id: config,
                    stream_epoch: metadata.stream_epoch,
                    sentence_id: metadata.sentence_id.map(Cow::into_owned),
                    local_correlation_id: metadata.local_correlation_id,
                    provider_timestamp_ms: metadata.provider_timestamp_ms,
                    start_ms: metadata.start_ms,
                    end_ms: metadata.end_ms,
                },
            });
        }
    }
    let result = writer.write_all(b"]}");
    writer.io(result)?;
    scope.check()?;
    Ok(FinalSnapshot {
        session_id,
        last_rowid,
        input_final_count: owned.len() as u32,
        rows: owned,
        normalized_bytes: writer.bytes,
        input_hash: format!("{:x}", writer.hash.finalize()),
    })
}
