use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: Uuid,
    pub started_at: i64,
    pub mode: String,
    pub direction: String,
    pub source_language: String,
    pub target_language: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptSegment {
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
