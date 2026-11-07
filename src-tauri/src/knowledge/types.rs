use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Pdf,
    Docx,
    Text,
    Markdown,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TextLocation {
    pub page: Option<u32>,
    pub paragraph: u32,
    pub heading: Option<String>,
    pub start_char: u32,
    pub end_char: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TextBlock {
    pub text: String,
    pub location: TextLocation,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Parsing,
    Parsed,
    Indexing,
    Ready,
    RepairRequired,
    Failed,
    Deleting,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentSummary {
    pub id: uuid::Uuid,
    pub title: Option<String>,
    pub kind: DocumentKind,
    pub version: u32,
    pub origin: crate::intelligence::types::RecordOrigin,
    pub sha256: String,
    pub status: DocumentStatus,
    pub chunk_count: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: Option<i64>,
    pub error: Option<crate::intelligence::types::IntelligenceErrorCode>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentPage {
    pub rows: Vec<DocumentSummary>,
    pub next_cursor: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocumentDetail {
    pub summary: DocumentSummary,
    pub original_path: Option<std::path::PathBuf>,
    pub managed_path: Option<std::path::PathBuf>,
    pub legacy_uri: Option<String>,
    pub index_identity: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSearch {
    pub query: String,
    #[serde(default)]
    pub selected_segment_ids: Vec<uuid::Uuid>,
    pub session_id: Option<uuid::Uuid>,
    #[serde(default = "search_limit")]
    pub limit: u32,
}
fn search_limit() -> u32 {
    5
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeHit {
    pub document_id: uuid::Uuid,
    pub document_version: u32,
    pub chunk_id: uuid::Uuid,
    pub title: String,
    pub location: TextLocation,
    pub text: String,
    pub score: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeResults {
    pub hits: Vec<KnowledgeHit>,
    pub identity: Option<String>,
    pub unavailable_documents: u32,
}
