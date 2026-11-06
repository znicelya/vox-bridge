//! Public safe wire values. Credentials remain input-only or opaque transport values.
use crate::app::CredentialChange;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Llm,
    Embedding,
    Tavily,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordOrigin {
    Native,
    Legacy,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelProfile {
    pub id: Uuid,
    pub kind: ServiceKind,
    pub name: String,
    pub base_url: String,
    pub model: Option<String>,
    pub timeout_ms: u32,
    pub max_context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub dimensions: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelProfileView {
    pub profile: ModelProfile,
    pub credential_configured: bool,
    pub last_probe: Option<ConnectionProbe>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveModelProfile {
    pub profile: ModelProfile,
    pub credential: CredentialChange,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct IntelligenceSettings {
    pub summary_profile_id: Option<Uuid>,
    pub embedding_profile_id: Option<Uuid>,
    pub tavily_profile_id: Option<Uuid>,
    pub automatic_summary: bool,
}
impl Default for IntelligenceSettings {
    fn default() -> Self {
        Self {
            summary_profile_id: None,
            embedding_profile_id: None,
            tavily_profile_id: None,
            automatic_summary: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManagedDataLocation {
    pub data_dir: PathBuf,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Reindex,
    DeleteDocument,
    Summary,
    Research,
    ResearchOverview,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntelligenceErrorCode {
    Configuration,
    Busy,
    Cancelled,
    Interrupted,
    Timeout,
    Network,
    Authentication,
    RateLimited,
    ResponseTooLarge,
    InvalidResponse,
    ContextTooLarge,
    UnsupportedDocument,
    ParseFailed,
    ResourceLimit,
    IndexRepairRequired,
    NoFinalText,
    Storage,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JobView {
    pub id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    pub session_id: Option<Uuid>,
    pub object_id: Option<Uuid>,
    pub completed_units: u32,
    pub total_units: Option<u32>,
    pub error: Option<IntelligenceErrorCode>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JobPage {
    pub rows: Vec<JobView>,
    pub next_cursor: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConnectionProbe {
    pub checked_at_ms: i64,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<IntelligenceErrorCode>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntelligenceEvent {
    JobChanged { job: JobView },
    ResyncNeeded { missed_events: u64 },
}

// Backend-only policy, deliberately absent from frontend input DTOs.
pub enum JobAdmission {
    Manual { exclusion_key: String },
    AutomaticSummary { session_id: Uuid },
}

/// Versioned internal payload. Producers supply IDs/validated input, never secrets.
/// At most three public service snapshots and 64 KiB of total serialized data.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct JobPayload {
    pub version: u32,
    pub profiles: Vec<ModelProfile>,
    pub input: serde_json::Value,
}

impl IntelligenceErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Busy => "busy",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::Timeout => "timeout",
            Self::Network => "network",
            Self::Authentication => "authentication",
            Self::RateLimited => "rate_limited",
            Self::ResponseTooLarge => "response_too_large",
            Self::InvalidResponse => "invalid_response",
            Self::ContextTooLarge => "context_too_large",
            Self::UnsupportedDocument => "unsupported_document",
            Self::ParseFailed => "parse_failed",
            Self::ResourceLimit => "resource_limit",
            Self::IndexRepairRequired => "index_repair_required",
            Self::NoFinalText => "no_final_text",
            Self::Storage => "storage",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Configuration => "Service configuration is invalid or incomplete.",
            Self::Busy => "Intelligence work is busy or the profile is in use.",
            Self::Cancelled => "Intelligence work was cancelled.",
            Self::Interrupted => "Intelligence work was interrupted.",
            Self::Timeout => "Service request timed out.",
            Self::Network => "Service connection failed.",
            Self::Authentication => "Service authentication failed.",
            Self::RateLimited => "Service rate limit reached.",
            Self::ResponseTooLarge => "Service response exceeded the size limit.",
            Self::InvalidResponse => "Service response was invalid.",
            Self::ContextTooLarge => "Input exceeded the model context limit.",
            Self::UnsupportedDocument => "Document format is unsupported.",
            Self::ParseFailed => "Document could not be parsed.",
            Self::ResourceLimit => "Intelligence resource limit reached.",
            Self::IndexRepairRequired => "Document index requires repair.",
            Self::NoFinalText => "No final transcript text is available.",
            Self::Storage => "Intelligence data could not be stored or read.",
        }
    }
}
impl std::fmt::Display for IntelligenceErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}
