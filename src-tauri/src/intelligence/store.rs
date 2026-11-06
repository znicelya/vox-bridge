//! Transactional durable admission shared by the scheduler and summary version writer.
use super::{config::validate_profile, types::*};
use crate::{
    error::{AppError, AppResult},
    storage::LocalStore,
};
use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

pub(crate) const MAX_UNFINISHED: usize = 32;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
const JOB_COLUMNS: &str = "id,kind,status,session_id,object_id,completed_units,total_units,error_code,created_at_ms,updated_at_ms";
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Reserved for Task 3B service and scheduler composition"
    )
)]
pub(crate) struct JobDraft {
    pub id: Uuid,
    pub kind: JobKind,
    pub session_id: Option<Uuid>,
    pub object_id: Option<Uuid>,
    pub admission: JobAdmission,
    pub payload: serde_json::Value,
    pub initial_failure: Option<IntelligenceErrorCode>,
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Reserved for Task 3B service and scheduler composition"
    )
)]
pub(crate) enum JobAdmissionResult {
    Created(JobView),
    Existing(JobView),
}
impl JobAdmissionResult {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Reserved for Task 3B service and scheduler composition"
        )
    )]
    pub(crate) fn view(&self) -> &JobView {
        match self {
            Self::Created(v) | Self::Existing(v) => v,
        }
    }
}
/// No commit, wake or result insertion here. Caller acts on Created/Existing and
/// commits the job together with any result-version row in its own transaction.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Reserved for Task 3B service and scheduler composition"
    )
)]
pub(crate) fn admit_job(tx: &Transaction<'_>, draft: JobDraft) -> AppResult<JobAdmissionResult> {
    admit(tx, draft).map_err(safe_store_error)
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Reserved for Task 3B service and scheduler composition"
    )
)]
fn admit(tx: &Transaction<'_>, draft: JobDraft) -> AppResult<JobAdmissionResult> {
    let (exclusion, permanent) = match &draft.admission {
        JobAdmission::AutomaticSummary { session_id } => {
            if draft.kind != JobKind::Summary || draft.session_id != Some(*session_id) {
                return Err(configuration());
            }
            (
                format!("summary:{session_id}"),
                Some(format!("auto-summary:{session_id}")),
            )
        }
        JobAdmission::Manual { exclusion_key } => {
            if draft.initial_failure.is_some() {
                return Err(configuration());
            }
            let expected = match draft.kind {
                JobKind::Summary => {
                    format!("summary:{}", draft.session_id.ok_or_else(configuration)?)
                }
                JobKind::Import | JobKind::Reindex | JobKind::DeleteDocument => {
                    format!("document:{}", draft.object_id.ok_or_else(configuration)?)
                }
                JobKind::Research => {
                    format!("research:{}", draft.object_id.ok_or_else(configuration)?)
                }
                JobKind::ResearchOverview => format!(
                    "research-overview:{}",
                    draft.object_id.ok_or_else(configuration)?
                ),
            };
            if exclusion_key.is_empty()
                || exclusion_key.len() > 160
                || !exclusion_key.is_ascii()
                || exclusion_key != &expected
            {
                return Err(configuration());
            }
            (expected, None)
        }
    };
    if draft.initial_failure.is_some_and(|code| {
        !matches!(
            code,
            IntelligenceErrorCode::Configuration
                | IntelligenceErrorCode::Busy
                | IntelligenceErrorCode::NoFinalText
                | IntelligenceErrorCode::Interrupted
        )
    }) {
        return Err(configuration());
    }
    // Permanent identity precedes exclusion/capacity and payload work, including
    // terminal markers and a queue that has filled since the first callback.
    if let Some(key) = &permanent {
        let existing = tx
            .query_row(
                &format!("SELECT {JOB_COLUMNS} FROM intelligence_jobs WHERE permanent_key=?1"),
                [key],
                read_job,
            )
            .optional()?;
        if let Some(existing) = existing {
            return Ok(JobAdmissionResult::Existing(existing));
        }
    }
    let payload_json = serde_json::to_string(&draft.payload).map_err(|_| configuration())?;
    decode_payload(&payload_json)?;
    if draft.initial_failure.is_none() {
        let collision:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM intelligence_jobs WHERE exclusion_key=?1 AND status IN ('queued','running'))",[&exclusion],|r|r.get(0))?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM intelligence_jobs WHERE status IN ('queued','running')",
            [],
            |r| r.get(0),
        )?;
        if collision || count >= MAX_UNFINISHED as i64 {
            return Err(AppError::Intelligence(IntelligenceErrorCode::Busy));
        }
    }
    let status = if draft.initial_failure.is_some() {
        JobStatus::Failed
    } else {
        JobStatus::Queued
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .ok_or_else(storage)?;
    let view = JobView {
        id: draft.id,
        kind: draft.kind,
        status,
        session_id: draft.session_id,
        object_id: draft.object_id,
        completed_units: 0,
        total_units: None,
        error: draft.initial_failure,
        created_at_ms: now,
        updated_at_ms: now,
    };
    tx.execute("INSERT INTO intelligence_jobs (id,kind,status,session_id,object_id,exclusion_key,permanent_key,payload_json,error_code,created_at_ms,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",params![view.id.to_string(),enum_name(view.kind)?,enum_name(status)?,view.session_id.map(|id|id.to_string()),view.object_id.map(|id|id.to_string()),exclusion,permanent,payload_json,view.error.map(IntelligenceErrorCode::as_str),now])?;
    Ok(JobAdmissionResult::Created(view))
}
impl LocalStore {
    pub fn get_job(&self, id: Uuid) -> AppResult<Option<JobView>> {
        self.transaction(|tx| {
            Ok(tx
                .query_row(
                    &format!("SELECT {JOB_COLUMNS} FROM intelligence_jobs WHERE id=?1"),
                    [id.to_string()],
                    read_job,
                )
                .optional()?)
        })
        .map_err(safe_store_error)
    }
    pub fn list_jobs(&self, after: Option<i64>, limit: u32) -> AppResult<JobPage> {
        if !(1..=200).contains(&limit) || after.is_some_and(|cursor| cursor < 0) {
            return Err(configuration());
        }
        self.transaction(|tx| {
            let mut statement=tx.prepare(&format!("SELECT rowid,{JOB_COLUMNS} FROM intelligence_jobs WHERE rowid>?1 ORDER BY rowid LIMIT ?2"))?;
            let mut rows=statement.query(params![after.unwrap_or(0),limit+1])?;
            let mut values=Vec::new();let mut cursor=None;
            while let Some(row)=rows.next()? {
                if values.len()==limit as usize {return Ok(JobPage {rows:values,next_cursor:cursor});}
                cursor=Some(row.get(0)?);values.push(read_job_at(row,1)?);
            }
            Ok(JobPage {rows:values,next_cursor:None})
        }).map_err(safe_store_error)
    }
}
/// Bounded scan of <=32 unfinished payloads (<=64KiB each), never history.
/// Corrupt/oversize rows fail closed, so a profile cannot be deleted on uncertainty.
pub(crate) fn profile_in_use(tx: &Transaction<'_>, id: Uuid) -> AppResult<bool> {
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM intelligence_jobs WHERE status IN ('queued','running')",
        [],
        |r| r.get(0),
    )?;
    if count > MAX_UNFINISHED as i64 {
        return Err(storage());
    }
    let mut statement = tx.prepare(
        "SELECT payload_json FROM intelligence_jobs WHERE status IN ('queued','running') LIMIT 32",
    )?;
    for row in statement.query_map([], |r| r.get::<_, String>(0))? {
        if decode_payload(&row?)?.profiles.iter().any(|p| p.id == id) {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(crate) fn probe(tx: &Transaction<'_>, id: Uuid) -> AppResult<Option<ConnectionProbe>> {
    let raw: Option<String> = tx
        .query_row(
            "SELECT result_json FROM service_probes WHERE profile_id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|value| serde_json::from_str(&value).map_err(|_| storage()))
        .transpose()
}
pub(crate) fn decode_payload(raw: &str) -> AppResult<JobPayload> {
    if raw.len() > MAX_PAYLOAD_BYTES {
        return Err(configuration());
    }
    let payload: JobPayload = serde_json::from_str(raw).map_err(|_| configuration())?;
    if payload.version != 1 || payload.profiles.len() > 3 {
        return Err(configuration());
    }
    let mut ids = std::collections::HashSet::new();
    for p in &payload.profiles {
        validate_profile(p)?;
        if !ids.insert(p.id) {
            return Err(configuration());
        }
    }
    Ok(payload)
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Reserved for Task 3B service and scheduler composition"
    )
)]
fn enum_name(value: impl serde::Serialize) -> AppResult<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(storage)
}
fn read_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobView> {
    read_job_at(row, 0)
}
fn read_job_at(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<JobView> {
    let uuid =
        |index| Uuid::parse_str(&row.get::<_, String>(index)?).map_err(|_| conversion(index));
    let optional_uuid = |index| {
        row.get::<_, Option<String>>(index)?
            .map(|v| Uuid::parse_str(&v).map_err(|_| conversion(index)))
            .transpose()
    };
    Ok(JobView {
        id: uuid(offset)?,
        kind: read_enum(row, offset + 1)?,
        status: read_enum(row, offset + 2)?,
        session_id: optional_uuid(offset + 3)?,
        object_id: optional_uuid(offset + 4)?,
        completed_units: row.get(offset + 5)?,
        total_units: row.get(offset + 6)?,
        error: row
            .get::<_, Option<String>>(offset + 7)?
            .map(|s| {
                serde_json::from_value(serde_json::Value::String(s))
                    .map_err(|_| conversion(offset + 7))
            })
            .transpose()?,
        created_at_ms: row.get(offset + 8)?,
        updated_at_ms: row.get(offset + 9)?,
    })
}
fn read_enum<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(row.get(index)?))
        .map_err(|_| conversion(index))
}
fn conversion(index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other("invalid intelligence row")),
    )
}
fn configuration() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::Configuration)
}
fn storage() -> AppError {
    AppError::Intelligence(IntelligenceErrorCode::Storage)
}
pub(crate) fn safe_store_error(error: AppError) -> AppError {
    match error {
        AppError::Intelligence(_) => error,
        _ => storage(),
    }
}
#[cfg(test)]
mod tests;
