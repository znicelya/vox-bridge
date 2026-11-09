use super::timestamp;
use crate::error::AppResult;
use crate::storage::ExportData;
use serde::Serialize;

#[derive(Serialize)]
struct JsonExport<'a> {
    session: JsonSession<'a>,
    segments: Vec<JsonSegment<'a>>,
    summaries: &'a [crate::summary::types::SummaryExportRecord],
    diagnostics: &'a [crate::domain::transcript::RuntimeDiagnostic],
    context: &'a Option<serde_json::Value>,
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
    role: &'static str,
    origin: &'static str,
    details: &'a Option<crate::domain::transcript::TranscriptDetails>,
    occurred_at_ms: Option<i64>,
}
pub fn render_json(data: &ExportData) -> AppResult<String> {
    super::validate_summary_projection(data)?;
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
                role: super::segment_role(data, s),
                origin: if s.details.is_some() {
                    "provider"
                } else {
                    "legacy"
                },
                details: &s.details,
                occurred_at_ms: s.occurred_at_ms,
            })
            .collect(),
        summaries: &data.summaries,
        diagnostics: &data.diagnostics,
        context: &data.context,
    };
    let mut output = super::ArtifactWriter::new();
    serde_json::to_writer_pretty(&mut output, &value).map_err(|_| super::size_error())?;
    output.finish()
}
