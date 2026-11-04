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
