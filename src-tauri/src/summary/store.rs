//! Atomic admission, bounded projections, and fenced publication on LocalStore.
use super::{
    content::{self, SummaryContent},
    types::*,
};
use crate::{
    error::{AppError, AppResult},
    intelligence::{
        store::{self, JobAdmissionResult, JobDraft},
        types::*,
    },
    storage::LocalStore,
};
use rusqlite::{OptionalExtension, Row, Transaction, params, types::ValueRef};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) struct SummaryJobRequest {
    pub(crate) session_id: Uuid,
    pub(crate) profile: Option<ModelProfile>,
    pub(crate) automatic: bool,
    pub(crate) initial_failure: Option<IntelligenceErrorCode>,
}
pub(crate) fn error(code: IntelligenceErrorCode) -> AppError {
    AppError::Intelligence(code)
}
pub(crate) fn storage() -> AppError {
    error(IntelligenceErrorCode::Storage)
}
pub(crate) fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
pub(crate) fn text<'a>(row: &'a Row<'_>, index: usize, max: usize) -> AppResult<&'a str> {
    match row.get_ref(index)? {
        ValueRef::Text(raw) if raw.len() <= max => std::str::from_utf8(raw).map_err(|_| storage()),
        ValueRef::Text(_) => Err(error(IntelligenceErrorCode::ResponseTooLarge)),
        _ => Err(storage()),
    }
}
pub(crate) fn optional_text<'a>(
    row: &'a Row<'_>,
    index: usize,
    max: usize,
) -> AppResult<Option<&'a str>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        _ => text(row, index, max).map(Some),
    }
}
pub(crate) fn uuid(raw: &str) -> AppResult<Uuid> {
    let id = Uuid::parse_str(raw).map_err(|_| storage())?;
    if id.to_string() != raw {
        return Err(storage());
    }
    Ok(id)
}
pub(crate) fn session_open(tx: &Transaction<'_>, id: Uuid) -> AppResult<()> {
    let deleting: Option<i64> = tx
        .query_row(
            "SELECT summary_session_deleting FROM session_lifecycle WHERE session_id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    match deleting {
        Some(0) => Ok(()),
        Some(1) => Err(error(IntelligenceErrorCode::Cancelled)),
        _ => Err(storage()),
    }
}
impl LocalStore {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Required direct-store admission companion in the frozen Task1/Task2 interface"
        )
    )]
    pub(crate) fn create_summary_job(&self, request: SummaryJobRequest) -> AppResult<JobView> {
        self.transaction(|tx| Self::create_summary_job_in_tx(tx, request).map(|r| r.view().clone()))
            .map_err(store::safe_store_error)
    }
    pub(crate) fn create_summary_job_in_tx(
        tx: &Transaction<'_>,
        request: SummaryJobRequest,
    ) -> AppResult<JobAdmissionResult> {
        let SummaryJobRequest {
            session_id,
            profile,
            automatic,
            mut initial_failure,
        } = request;
        let permanent = format!("auto-summary:{session_id}");
        if automatic {
            if let Some(existing) = store::permanent_job(tx, &permanent)? {
                return Ok(JobAdmissionResult::Existing(existing));
            }
        }
        session_open(tx, session_id)?;
        if automatic {
            let eligible:bool=tx.query_row("SELECT status IN ('completed','failed') FROM session_lifecycle WHERE session_id=?1",[session_id.to_string()],|r|r.get(0))?;
            if !eligible {
                return Err(error(IntelligenceErrorCode::Configuration));
            }
            if initial_failure.is_none() {
                let has_final:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM translation_segments WHERE session_id=?1 AND is_final=1)",[session_id.to_string()],|r|r.get(0))?;
                initial_failure = if !has_final {
                    Some(IntelligenceErrorCode::NoFinalText)
                } else if profile.is_none() {
                    Some(IntelligenceErrorCode::Configuration)
                } else {
                    None
                };
            }
        }
        // Read-only prospective allocation. The existing admit helper remains
        // the sole job/capacity/permanent policy; only Created advances this.
        let prior: Option<i64> = tx
            .query_row(
                "SELECT last_version FROM summary_version_counters WHERE session_id=?1",
                [session_id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        let prior = match prior {
            Some(v) => v,
            None => tx.query_row(
                "SELECT max(0,coalesce(max(version),0)) FROM session_summaries WHERE session_id=?1",
                [session_id.to_string()],
                |r| r.get(0),
            )?,
        };
        let version = u32::try_from(prior)
            .ok()
            .and_then(|v| v.checked_add(1))
            .ok_or_else(|| error(IntelligenceErrorCode::ResourceLimit))?;
        let summary_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let payload = serde_json::json!({"version":1,"profiles":profile.iter().cloned().collect::<Vec<_>>(),"input":{"summary_id":summary_id,"session_id":session_id}});
        let draft = |failure| JobDraft {
            id: job_id,
            kind: JobKind::Summary,
            session_id: Some(session_id),
            object_id: Some(summary_id),
            admission: if automatic {
                JobAdmission::AutomaticSummary { session_id }
            } else {
                JobAdmission::Manual {
                    exclusion_key: format!("summary:{session_id}"),
                }
            },
            payload: payload.clone(),
            initial_failure: failure,
        };
        let result = match store::admit_job(tx, draft(initial_failure)) {
            Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
                if automatic && initial_failure.is_none() =>
            {
                store::admit_job(tx, draft(Some(IntelligenceErrorCode::Busy)))?
            }
            other => other?,
        };
        if let JobAdmissionResult::Created(job) = &result {
            let witness = {
                let mut q = tx.prepare("SELECT payload_json FROM intelligence_jobs WHERE id=?1")?;
                let mut rows = q.query([job.id.to_string()])?;
                let row = rows.next()?.ok_or_else(storage)?;
                hash(text(row, 0, 64 * 1024)?.as_bytes())
            };
            tx.execute("INSERT INTO session_summaries(id,session_id,version,created_at,status,structured_json,profile_id,model,error_code,origin,job_id,created_at_ms,job_payload_hash) VALUES(?1,?2,?3,?4,?5,'null',?6,?7,?8,'native',?9,?10,?11)",params![summary_id.to_string(),session_id.to_string(),version,job.created_at_ms.div_euclid(1000),serde_json::to_value(job.status).unwrap().as_str(),profile.as_ref().map(|p|p.id.to_string()),profile.as_ref().and_then(|p|p.model.clone()),job.error.map(IntelligenceErrorCode::as_str),job.id.to_string(),job.created_at_ms,witness])?;
            tx.execute("INSERT INTO summary_version_counters(session_id,last_version) VALUES(?1,?2) ON CONFLICT(session_id) DO UPDATE SET last_version=excluded.last_version",params![session_id.to_string(),version])?;
        }
        Ok(result)
    }
    pub(crate) fn summary_page(
        &self,
        session: Uuid,
        after: Option<SummaryCursor>,
        limit: u32,
    ) -> AppResult<SummaryPage> {
        if !(1..=100).contains(&limit) {
            return Err(error(IntelligenceErrorCode::Configuration));
        }
        self.transaction(|tx| {
            let mut q=tx.prepare("SELECT id,session_id,version,created_at,status,structured_json,origin,profile_id,model,created_at_ms,error_code,input_final_count,input_hash,raw_response_hash,deleting,job_id,job_payload_hash FROM session_summaries WHERE session_id=?1 AND (?2 IS NULL OR version>?2) ORDER BY version LIMIT ?3")?;
            let mut rows=q.query(params![session.to_string(),after.as_ref().map(SummaryCursor::value),limit+1])?;
            // Reserve the complete page envelope and the longest signed-i64
            // cursor. Counting one separator per row is conservative by one
            // byte, so every returned continuation stays within the wire cap.
            let mut out=Vec::new(); let mut cursor=None;
            let mut bytes=b"{\"rows\":[],\"next_cursor\":\"\"}".len()+20;
            while let Some(row)=rows.next()? {
                if out.len()==limit as usize { return Ok(SummaryPage{rows:out,next_cursor:cursor}); }
                let view=project(row,tx)?;
                let row_limit=match &view {SummaryView::Native(_)=>content::CONTENT_BYTES+64*1024,SummaryView::Legacy(_)=>8*1024*1024};
                let size=content::json_size(&view,row_limit).map_err(error)?;
                if bytes.checked_add(size+1).is_none_or(|n|n>8*1024*1024) {
                    return Ok(SummaryPage{rows:out,next_cursor:cursor});
                }
                bytes+=size+1;
                cursor=Some(SummaryCursor::from_version(row.get(2)?)); out.push(view);
            }
            Ok(SummaryPage{rows:out,next_cursor:None})
        }).map_err(store::safe_store_error)
    }
    pub(crate) fn summary_export_records(
        &self,
        session: Uuid,
    ) -> AppResult<Vec<SummaryExportRecord>> {
        self.transaction(|tx| {
            // This explicit projection never queries the response table.
            let mut q=tx.prepare("SELECT id,session_id,version,created_at,status,structured_json,origin,profile_id,model,created_at_ms,error_code,input_final_count,input_hash,raw_response_hash,deleting,job_id,job_payload_hash FROM session_summaries WHERE session_id=?1 ORDER BY version LIMIT 200001")?;
            let mut rows=q.query([session.to_string()])?;let mut out=Vec::new();let mut bytes=2usize;
            while let Some(row)=rows.next()? {
                if out.len()==200000 {return Err(error(IntelligenceErrorCode::ResourceLimit));}
                let record=match project(row,tx)? {
                    SummaryView::Native(v)=>SummaryExportRecord{id:v.id,session_id:v.session_id,version:i64::from(v.version),created_at:row.get(3)?,status:serde_json::to_value(v.status).map_err(|_|storage())?.as_str().ok_or_else(storage)?.to_owned(),origin:SummaryOrigin::Native,created_at_ms:Some(v.created_at_ms),deleting:Some(v.deleting),content:v.content.map(SummaryExportContent::Native),profile_id:v.profile_id,model:v.model,error:v.error,input_final_count:v.input_final_count,input_hash:v.input_hash,raw_response_hash:v.raw_response_hash},
                    SummaryView::Legacy(v)=> {
                        if let Some(code)=v.projection_error {return Err(error(code));}
                        SummaryExportRecord{id:v.id,session_id:v.session_id,version:row.get(2)?,created_at:row.get(3)?,status:v.status,origin:SummaryOrigin::Legacy,created_at_ms:None,deleting:None,content:Some(SummaryExportContent::Legacy(v.content.ok_or_else(storage)?)),profile_id:None,model:None,error:None,input_final_count:None,input_hash:None,raw_response_hash:None}
                    },
                };
                let row_limit=match record.origin {SummaryOrigin::Native=>content::CONTENT_BYTES+64*1024,SummaryOrigin::Legacy=>8*1024*1024};
                let size=content::json_size(&record,row_limit).map_err(error)?;
                bytes=bytes.checked_add(size+1).filter(|n|*n<=32*1024*1024).ok_or_else(||error(IntelligenceErrorCode::ResponseTooLarge))?;
                out.push(record);
            }
            Ok(out)
        }).map_err(store::safe_store_error)
    }
}
fn enum_text<T: serde::de::DeserializeOwned>(raw: &str) -> AppResult<T> {
    serde_json::from_value(serde_json::Value::String(raw.into())).map_err(|_| storage())
}
fn safe_hash(row: &Row<'_>, column: usize) -> AppResult<Option<String>> {
    optional_text(row, column, 64)?
        .map(|raw| {
            if content::valid_hash(raw) {
                Ok(raw.to_owned())
            } else {
                Err(storage())
            }
        })
        .transpose()
}
fn project(row: &Row<'_>, tx: &Transaction<'_>) -> AppResult<SummaryView> {
    let id = uuid(text(row, 0, 36)?)?;
    let session_id = uuid(text(row, 1, 36)?)?;
    let version: i64 = row.get(2)?;
    let seconds: i64 = row.get(3)?;
    let status = text(row, 4, 64 * 1024)?;
    match text(row, 6, 6)? {
        "legacy" => {
            let (content, projection_error) = match text(row, 5, content::CONTENT_BYTES)
                .and_then(|s| content::legacy_json(s).map_err(error))
            {
                Ok(v) => (Some(v), None),
                Err(AppError::Intelligence(code)) => (None, Some(code)),
                Err(_) => (None, Some(IntelligenceErrorCode::InvalidResponse)),
            };
            Ok(SummaryView::Legacy(LegacySummaryView {
                id,
                session_id,
                version: version.to_string(),
                status: status.to_owned(),
                created_at_seconds: seconds.to_string(),
                created_at_ms: None,
                content,
                projection_error,
                profile_id: None,
                model: None,
                error: None,
                input_final_count: None,
                input_hash: None,
                raw_response_hash: None,
            }))
        }
        "native" => {
            let witness = text(row, 16, 64)?;
            if !content::valid_hash(witness) {
                return Err(storage());
            }
            let job_id = optional_text(row, 15, 36)?
                .map(uuid)
                .transpose()?
                .ok_or_else(storage)?;
            let anchored = super::job_proof::SummaryJobProof::anchored_in_tx(tx, job_id)?
                .ok_or_else(storage)?;
            if anchored.summary_id() != id || anchored.session_id() != session_id {
                return Err(storage());
            }
            let version = u32::try_from(version)
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(storage)?;
            let raw = text(row, 5, content::CONTENT_BYTES)?;
            let content = if raw == "null" {
                None
            } else {
                Some(SummaryContent::parse(raw).map_err(|_| storage())?)
            };
            let model = optional_text(row, 8, 800)?.map(str::to_owned);
            if model.as_ref().is_some_and(|m| m.chars().count() > 200) {
                return Err(storage());
            }
            let deleting: i64 = row.get(14)?;
            if !matches!(deleting, 0 | 1) {
                return Err(storage());
            }
            let count: Option<u32> = row.get(11)?;
            if count.is_some_and(|c| c > 100000) {
                return Err(storage());
            }
            Ok(SummaryView::Native(NativeSummaryView {
                id,
                session_id,
                version,
                status: enum_text(status)?,
                created_at_ms: row.get(9)?,
                content,
                deleting: deleting == 1,
                profile_id: optional_text(row, 7, 36)?.map(uuid).transpose()?,
                model,
                error: optional_text(row, 10, 32)?.map(enum_text).transpose()?,
                input_final_count: count,
                input_hash: safe_hash(row, 12)?,
                raw_response_hash: safe_hash(row, 13)?,
            }))
        }
        _ => Err(storage()),
    }
}

/// Called only after an actual completion returned. Raw and supplied map
/// tuples commit together before any strict content deserialization.
pub(crate) fn retain_response_in_tx(
    tx: &Transaction<'_>,
    proof: &super::job_proof::SummaryJobProof,
    manifest: &super::planner::Manifest,
    plan: &super::planner::RequestPlan,
    response: &crate::llm::LlmResponse,
) -> AppResult<()> {
    use super::planner::Phase;
    proof.running_non_deleting_in_tx(tx)?;
    if plan.ordinal >= 64 || response.content.len() > content::CONTENT_BYTES {
        return Err(error(IntelligenceErrorCode::ResponseTooLarge));
    }
    if !content::valid_hash(&response.response_hash)
        || hash(response.content.as_bytes()) != response.response_hash
    {
        return Err(error(IntelligenceErrorCode::InvalidResponse));
    }
    let (count, min, max): (u32, Option<u32>, Option<u32>) = tx.query_row(
        "SELECT count(*),min(ordinal),max(ordinal) FROM summary_responses WHERE summary_id=?1",
        [proof.summary_id().to_string()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if count != plan.ordinal || (count > 0 && (min != Some(0) || max != Some(count - 1))) {
        return Err(storage());
    }
    let previous: u64 = tx.query_row(
        "SELECT raw_response_bytes FROM session_summaries WHERE id=?1",
        [proof.summary_id().to_string()],
        |r| r.get(0),
    )?;
    let total = previous
        .checked_add(response.content.len() as u64)
        .filter(|n| *n <= 32 * 1024 * 1024)
        .ok_or_else(|| error(IntelligenceErrorCode::ResponseTooLarge))?;
    if tx.execute("UPDATE session_summaries SET raw_response_bytes=?2,raw_response_hash=?3 WHERE id=?1 AND job_id=?4 AND origin='native' AND status='running' AND deleting=0",
        params![proof.summary_id().to_string(),total,response.response_hash,proof.job_id().to_string()])?!=1 {return Err(storage());}
    tx.execute("INSERT INTO summary_responses(summary_id,ordinal,phase,raw_text,response_hash) VALUES(?1,?2,?3,?4,?5)",
        params![proof.summary_id().to_string(),plan.ordinal,plan.phase.as_str(),response.content,response.response_hash])?;
    if plan.phase == Phase::Map {
        if plan.coverage != plan.input_range
            || plan.coverage.is_empty()
            || plan.coverage.end > manifest.parts.len()
        {
            return Err(storage());
        }
        let mut insert=tx.prepare("INSERT INTO summary_coverage(summary_id,segment_id,part_index,part_count) VALUES(?1,?2,?3,?4)")?;
        for part in &manifest.parts[plan.coverage.clone()] {
            insert.execute(params![
                proof.summary_id().to_string(),
                part.key.segment_id.to_string(),
                part.key.part_index,
                part.key.part_count
            ])?;
        }
    }
    proof.require_in_tx(tx)
}

/// Compare the complete tuple stream, including every middle split part.
/// The sorted index contains only bounded manifest references, never raw text.
fn require_complete_coverage(
    tx: &Transaction<'_>,
    summary: Uuid,
    manifest: &super::planner::Manifest,
) -> AppResult<()> {
    if manifest.parts.is_empty() || manifest.parts.len() > 100000 {
        return Err(storage());
    }
    let mut expected: Vec<usize> = (0..manifest.parts.len()).collect();
    expected.sort_unstable_by(|a, b| {
        let (a, b) = (&manifest.parts[*a].key, &manifest.parts[*b].key);
        a.segment_id
            .as_bytes()
            .cmp(b.segment_id.as_bytes())
            .then(a.part_index.cmp(&b.part_index))
    });
    let mut q=tx.prepare("SELECT segment_id,part_index,part_count FROM summary_coverage WHERE summary_id=?1 ORDER BY segment_id,part_index LIMIT 100001")?;
    let mut rows = q.query([summary.to_string()])?;
    let mut prior: Option<&super::planner::PartKey> = None;
    for index in expected {
        let key = &manifest.parts[index].key;
        if key.part_count == 0 || key.part_count > 100000 || key.part_index >= key.part_count {
            return Err(storage());
        }
        match prior {
            Some(p) if p.segment_id == key.segment_id => {
                if p.part_count != key.part_count
                    || p.part_index.checked_add(1) != Some(key.part_index)
                {
                    return Err(storage());
                }
            }
            Some(p) => {
                if p.part_index + 1 != p.part_count || key.part_index != 0 {
                    return Err(storage());
                }
            }
            None => {
                if key.part_index != 0 {
                    return Err(storage());
                }
            }
        }
        let row = rows.next()?.ok_or_else(storage)?;
        if uuid(text(row, 0, 36)?)? != key.segment_id
            || row.get::<_, u32>(1)? != key.part_index
            || row.get::<_, u32>(2)? != key.part_count
        {
            return Err(storage());
        }
        prior = Some(key);
    }
    if prior.is_none_or(|p| p.part_index + 1 != p.part_count) || rows.next()?.is_some() {
        return Err(storage());
    }
    Ok(())
}
pub(crate) fn publish_root_in_tx(
    tx: &Transaction<'_>,
    proof: &super::job_proof::SummaryJobProof,
    manifest: &super::planner::Manifest,
    root: &super::planner::SummaryNode,
) -> AppResult<()> {
    proof.running_non_deleting_in_tx(tx)?;
    if root.coverage != (0..manifest.parts.len()) {
        return Err(storage());
    }
    require_complete_coverage(tx, proof.summary_id(), manifest)?;
    content::json_size(&root.content, content::CONTENT_BYTES).map_err(error)?;
    let raw = serde_json::to_string(&root.content).map_err(|_| storage())?;
    // Validate typed content again at the storage boundary, including fields
    // constructed by private callers rather than a deserializer.
    SummaryContent::parse(&raw).map_err(error)?;
    if tx.execute("UPDATE session_summaries SET structured_json=?2 WHERE id=?1 AND job_id=?3 AND origin='native' AND status='running' AND deleting=0",
        params![proof.summary_id().to_string(),raw,proof.job_id().to_string()])?!=1 {return Err(storage());}
    proof.require_in_tx(tx)
}
