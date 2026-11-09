//! Fresh Native authority. Payload/object IDs never choose the target row.
use super::{
    content,
    store::{self, optional_text, storage, text, uuid},
};
use crate::{
    error::AppResult,
    intelligence::{config::validate_profile, store::decode_payload, types::*},
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct SummaryJobProof {
    trusted_job_id: Uuid,
    summary_id: Uuid,
    session_id: Uuid,
    version: u32,
    profile: Option<ModelProfile>,
    expected_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryInput {
    #[serde(deserialize_with = "canonical_uuid")]
    summary_id: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    session_id: Uuid,
}
fn canonical_uuid<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Uuid, D::Error> {
    struct Id;
    impl<'de> serde::de::Visitor<'de> for Id {
        type Value = Uuid;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("canonical UUID")
        }
        fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<Uuid, E> {
            if raw.len() != 36 {
                return Err(E::custom("invalid UUID"));
            }
            let id = Uuid::parse_str(raw).map_err(|_| E::custom("invalid UUID"))?;
            if id.to_string() != raw {
                return Err(E::custom("invalid UUID"));
            }
            Ok(id)
        }
    }
    d.deserialize_str(Id)
}
struct Domain {
    proof: SummaryJobProof,
    payload: JobPayload,
    status: JobStatus,
    error: Option<IntelligenceErrorCode>,
}
fn read(tx: &Transaction<'_>, trusted_job_id: Uuid) -> AppResult<Option<Domain>> {
    // First and unique anchor: independently admitted Native row by trusted ID.
    let (summary_id, session_id, version, profile_id, model, witness) = {
        let mut q=tx.prepare("SELECT id,session_id,version,profile_id,model,job_payload_hash,created_at_ms,origin FROM session_summaries WHERE job_id=?1 LIMIT 2")?;
        let mut rows = q.query([trusted_job_id.to_string()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        match text(row, 7, 6)? {
            "legacy" => return Ok(None),
            "native" => {}
            _ => return Err(storage()),
        }
        let summary_id = uuid(text(row, 0, 36)?)?;
        let session_id = uuid(text(row, 1, 36)?)?;
        let version: u32 = row.get(2)?;
        if version == 0 {
            return Err(storage());
        }
        let profile_id = optional_text(row, 3, 36)?.map(uuid).transpose()?;
        let model = optional_text(row, 4, 800)?.map(str::to_owned);
        if model.as_ref().is_some_and(|m| m.chars().count() > 200) {
            return Err(storage());
        }
        let witness = text(row, 5, 64)?.to_owned();
        if !content::valid_hash(&witness) {
            return Err(storage());
        }
        let _: i64 = row.get(6)?;
        if rows.next()?.is_some() {
            return Err(storage());
        }
        (summary_id, session_id, version, profile_id, model, witness)
    };
    let mut q=tx.prepare("SELECT kind,object_id,session_id,exclusion_key,permanent_key,payload_json,status,error_code FROM intelligence_jobs WHERE id=?1")?;
    let mut rows = q.query([trusted_job_id.to_string()])?;
    let row = rows.next()?.ok_or_else(storage)?;
    if text(row, 0, 32)? != "summary" {
        return Err(storage());
    }
    if uuid(text(row, 1, 36)?)? != summary_id
        || uuid(text(row, 2, 36)?)? != session_id
        || text(row, 3, 160)? != format!("summary:{session_id}")
        || optional_text(row, 4, 160)?.is_some_and(|p| p != format!("auto-summary:{session_id}"))
    {
        return Err(storage());
    }
    let raw = text(row, 5, 64 * 1024)?;
    let digest = store::hash(raw.as_bytes());
    if digest != witness {
        return Err(storage());
    }
    let payload = decode_payload(raw).map_err(|_| storage())?;
    if payload.profiles.len() > 1 {
        return Err(storage());
    }
    let input: SummaryInput =
        serde_json::from_value(payload.input.clone()).map_err(|_| storage())?;
    if input.summary_id != summary_id || input.session_id != session_id {
        return Err(storage());
    }
    let profile = payload.profiles.first().cloned();
    match &profile {
        Some(p) => {
            validate_profile(p).map_err(|_| storage())?;
            if p.kind != ServiceKind::Llm || profile_id != Some(p.id) || model != p.model {
                return Err(storage());
            }
        }
        None => {
            if profile_id.is_some() || model.is_some() {
                return Err(storage());
            }
        }
    }
    let status: JobStatus =
        serde_json::from_value(serde_json::json!(text(row, 6, 32)?)).map_err(|_| storage())?;
    let error = optional_text(row, 7, 32)?
        .map(|raw| serde_json::from_value(serde_json::json!(raw)).map_err(|_| storage()))
        .transpose()?;
    Ok(Some(Domain {
        proof: SummaryJobProof {
            trusted_job_id,
            summary_id,
            session_id,
            version,
            profile,
            expected_digest: digest,
        },
        payload,
        status,
        error,
    }))
}
impl SummaryJobProof {
    pub(crate) fn claim_in_tx(
        tx: &Transaction<'_>,
        trusted_job_id: Uuid,
        claimed_payload: &serde_json::Value,
    ) -> AppResult<Self> {
        let domain = read(tx, trusted_job_id)
            .map_err(|_| storage())?
            .ok_or_else(storage)?;
        content::json_size(claimed_payload, 64 * 1024).map_err(|_| storage())?;
        let claimed: JobPayload =
            serde_json::from_value(claimed_payload.clone()).map_err(|_| storage())?;
        if domain.payload != claimed {
            return Err(storage());
        }
        Ok(domain.proof)
    }
    pub(crate) fn anchored_in_tx(
        tx: &Transaction<'_>,
        trusted_job_id: Uuid,
    ) -> AppResult<Option<Self>> {
        read(tx, trusted_job_id)
            .map(|d| d.map(|d| d.proof))
            .map_err(|_| storage())
    }
    pub(crate) fn require_in_tx(&self, tx: &Transaction<'_>) -> AppResult<()> {
        let fresh = read(tx, self.trusted_job_id)
            .map_err(|_| storage())?
            .ok_or_else(storage)?
            .proof;
        if fresh.summary_id != self.summary_id
            || fresh.session_id != self.session_id
            || fresh.version != self.version
            || fresh.profile != self.profile
            || fresh.expected_digest != self.expected_digest
        {
            return Err(storage());
        }
        Ok(())
    }
    pub(crate) fn terminal_in_tx(
        tx: &Transaction<'_>,
        trusted_job_id: Uuid,
        status: JobStatus,
        error: Option<IntelligenceErrorCode>,
    ) -> AppResult<Option<Self>> {
        let Some(domain) = read(tx, trusted_job_id).map_err(|_| storage())? else {
            return Ok(None);
        };
        if matches!(status, JobStatus::Queued | JobStatus::Running)
            || domain.status != status
            || domain.error != error
        {
            return Err(storage());
        }
        Ok(Some(domain.proof))
    }
    pub(crate) fn job_id(&self) -> Uuid {
        self.trusted_job_id
    }
    pub(crate) fn summary_id(&self) -> Uuid {
        self.summary_id
    }
    pub(crate) fn session_id(&self) -> Uuid {
        self.session_id
    }
    pub(crate) fn profile(&self) -> Option<&ModelProfile> {
        self.profile.as_ref()
    }
    pub(crate) fn running_non_deleting_in_tx(&self, tx: &Transaction<'_>) -> AppResult<()> {
        self.require_in_tx(tx)?;
        let running:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM session_summaries s JOIN intelligence_jobs j ON j.id=s.job_id JOIN session_lifecycle l ON l.session_id=s.session_id WHERE s.id=?1 AND s.job_id=?2 AND s.origin='native' AND s.deleting=0 AND l.summary_session_deleting=0 AND j.status='running')",params![self.summary_id.to_string(),self.trusted_job_id.to_string()],|r|r.get(0))?;
        if !running {
            return Err(store::error(IntelligenceErrorCode::Cancelled));
        }
        Ok(())
    }
}
