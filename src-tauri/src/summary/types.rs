use super::content::SummaryContent;
use crate::intelligence::types::{IntelligenceErrorCode, JobStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum SummaryView {
    Native(NativeSummaryView),
    Legacy(LegacySummaryView),
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NativeSummaryView {
    pub id: Uuid,
    pub session_id: Uuid,
    pub version: u32,
    pub status: JobStatus,
    pub created_at_ms: i64,
    pub content: Option<SummaryContent>,
    pub deleting: bool,
    pub profile_id: Option<Uuid>,
    pub model: Option<String>,
    pub error: Option<IntelligenceErrorCode>,
    pub input_final_count: Option<u32>,
    pub input_hash: Option<String>,
    pub raw_response_hash: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LegacySummaryView {
    pub id: Uuid,
    pub session_id: Uuid,
    pub version: String,
    pub status: String,
    pub created_at_seconds: String,
    pub created_at_ms: Option<i64>,
    pub content: Option<serde_json::Value>,
    pub projection_error: Option<IntelligenceErrorCode>,
    pub profile_id: Option<Uuid>,
    pub model: Option<String>,
    pub error: Option<IntelligenceErrorCode>,
    pub input_final_count: Option<u32>,
    pub input_hash: Option<String>,
    pub raw_response_hash: Option<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SummaryCursor(String);
impl SummaryCursor {
    pub(crate) fn from_version(value: i64) -> Self {
        Self(value.to_string())
    }
    pub(crate) fn value(&self) -> i64 {
        self.0.parse().expect("validated summary cursor")
    }
}
impl<'de> Deserialize<'de> for SummaryCursor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Cursor;
        impl<'de> serde::de::Visitor<'de> for Cursor {
            type Value = SummaryCursor;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("canonical signed decimal summary cursor")
            }
            fn visit_str<E: serde::de::Error>(self, raw: &str) -> Result<Self::Value, E> {
                if raw.len() > 20 {
                    return Err(E::custom("invalid summary cursor"));
                }
                let value = raw
                    .parse::<i64>()
                    .map_err(|_| E::custom("invalid summary cursor"))?;
                if value.to_string() != raw {
                    return Err(E::custom("invalid summary cursor"));
                }
                Ok(SummaryCursor(raw.to_owned()))
            }
        }
        d.deserialize_str(Cursor)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SummaryPage {
    pub rows: Vec<SummaryView>,
    pub next_cursor: Option<SummaryCursor>,
}

/// The export projection is produced only by the checked LocalStore reader.
/// It contains no job payload/witness, credentials, audio or raw responses.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SummaryOrigin {
    Native,
    Legacy,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub(crate) enum SummaryExportContent {
    Native(SummaryContent),
    Legacy(serde_json::Value),
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SummaryExportRecord {
    pub(crate) id: Uuid,
    pub(crate) session_id: Uuid,
    pub(crate) version: i64,
    pub(crate) created_at: i64,
    pub(crate) status: String,
    pub(crate) origin: SummaryOrigin,
    pub(crate) created_at_ms: Option<i64>,
    pub(crate) deleting: Option<bool>,
    #[serde(rename = "structured_json")]
    pub(crate) content: Option<SummaryExportContent>,
    pub(crate) profile_id: Option<Uuid>,
    pub(crate) model: Option<String>,
    pub(crate) error: Option<IntelligenceErrorCode>,
    pub(crate) input_final_count: Option<u32>,
    pub(crate) input_hash: Option<String>,
    pub(crate) raw_response_hash: Option<String>,
}
