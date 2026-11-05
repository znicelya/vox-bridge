use super::timestamp;
use crate::error::AppResult;
use crate::storage::ExportData;
use serde::Serialize;

#[derive(Serialize)]
struct JsonExport<'a> {
    session: JsonSession<'a>,
    segments: Vec<JsonSegment<'a>>,
    summaries: Vec<JsonSummary<'a>>,
}
#[derive(Serialize)]
struct JsonSession<'a> {
    id: String,
    started_at: i64,
    ended_at: Option<i64>,
    status: &'a str,
    mode: &'a str,
    direction: &'a str,
    source_language: &'a str,
    target_language: &'a str,
}
#[derive(Serialize)]
struct JsonSegment<'a> {
    id: String,
    direction: &'a str,
    sequence: i64,
    occurred_at: i64,
    timestamp: String,
    language: &'a str,
    text: &'a str,
    final_segment: bool,
}
#[derive(Serialize)]
struct JsonSummary<'a> {
    id: String,
    version: i64,
    created_at: i64,
    status: &'a str,
    structured_json: Option<serde_json::Value>,
}

pub fn render_json(data: &ExportData) -> AppResult<String> {
    let value = JsonExport {
        session: JsonSession {
            id: data.session.id.to_string(),
            started_at: data.session.started_at,
            ended_at: data.lifecycle.ended_at,
            status: &data.lifecycle.status,
            mode: &data.session.mode,
            direction: &data.session.direction,
            source_language: &data.session.source_language,
            target_language: &data.session.target_language,
        },
        segments: data
            .segments
            .iter()
            .filter(|s| s.is_final)
            .map(|s| JsonSegment {
                id: s.id.to_string(),
                direction: &s.direction,
                sequence: s.sequence,
                occurred_at: s.occurred_at,
                timestamp: timestamp(s.occurred_at),
                language: &s.language,
                text: &s.text,
                final_segment: true,
            })
            .collect(),
        summaries: data
            .summaries
            .iter()
            .map(|s| JsonSummary {
                id: s.id.to_string(),
                version: s.version,
                created_at: s.created_at,
                status: &s.status,
                structured_json: super::safe_summary_json(&s.structured_json),
            })
            .collect(),
    };
    Ok(serde_json::to_string_pretty(&value)
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?)
}
