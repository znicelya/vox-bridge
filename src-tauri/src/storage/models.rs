use crate::domain::transcript::{RuntimeDiagnostic, TranscriptDetails};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSession {
    pub id: Uuid,
    pub started_at: i64,
    pub mode: String,
    pub direction: String,
    pub source_language: String,
    pub target_language: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredTranscriptSegment {
    pub id: Uuid,
    pub session_id: Uuid,
    pub direction: String,
    pub sequence: i64,
    pub occurred_at: i64,
    pub language: String,
    pub text: String,
    pub is_final: bool,
    pub details: Option<TranscriptDetails>,
    pub occurred_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSessionLifecycle {
    pub session_id: Uuid,
    pub status: String,
    pub ended_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: Uuid,
    pub started_at: i64,
    pub mode: String,
    pub direction: String,
    pub source_language: String,
    pub target_language: String,
}

impl SessionSummary {
    pub fn cursor(&self) -> String {
        format!("{}:{}", self.started_at, self.id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryVersion {
    pub id: Uuid,
    pub session_id: Uuid,
    pub version: i64,
    pub created_at: i64,
    pub status: String,
    pub structured_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportData {
    pub session: StoredSession,
    pub lifecycle: StoredSessionLifecycle,
    pub segments: Vec<StoredTranscriptSegment>,
    pub summaries: Vec<crate::summary::types::SummaryExportRecord>,
    pub diagnostics: Vec<RuntimeDiagnostic>,
    pub context: Option<serde_json::Value>,
}
