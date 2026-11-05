use super::{safe_summary_json, segment_role, timestamp};
use crate::storage::ExportData;

pub fn render_markdown(data: &ExportData) -> String {
    let mut output = format!(
        "# Session {}\n\n- Status: {}\n- Started at: `{}`\n- Ended at: `{}`\n- Direction: {}\n- Source language: {}\n- Target language: {}\n\n## Transcript\n\n",
        data.session.id,
        data.lifecycle.status,
        timestamp(data.session.started_at),
        data.lifecycle
            .ended_at
            .map(timestamp)
            .unwrap_or_else(|| "ongoing".into()),
        data.session.direction,
        data.session.source_language,
        data.session.target_language
    );
    for segment in &data.segments {
        output.push_str(&format!(
            "- {} {} **{}** ({}): {}\n",
            timestamp(segment.occurred_at),
            segment.direction,
            segment_role(data, segment),
            segment.language,
            segment.text
        ));
    }
    if !data.summaries.is_empty() {
        output.push_str("\n## Summaries\n\n");
        for summary in &data.summaries {
            output.push_str(&format!("### Version {}\n\n", summary.version));
            if let Some(value) = safe_summary_json(&summary.structured_json) {
                if let Some(title) = value.get("title").and_then(|v| v.as_str()) {
                    output.push_str(&format!("**{}**\n\n", title));
                }
                if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                    output.push_str(text);
                    output.push_str("\n\n");
                }
                if value.get("title").is_none() && value.get("text").is_none() {
                    output.push_str(&serde_json::to_string_pretty(&value).unwrap_or_default());
                    output.push_str("\n\n");
                }
            }
        }
    }
    output
}
