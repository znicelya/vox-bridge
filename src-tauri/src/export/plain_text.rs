use super::{safe_summary_json, segment_role, timestamp};
use crate::storage::ExportData;

pub fn render_plain_text(data: &ExportData) -> String {
    let mut output = String::new();
    for segment in &data.segments {
        if segment.is_final {
            output.push_str(&format!(
                "[{}] [{}] {} ({}): {}\n",
                timestamp(segment.occurred_at),
                segment.direction,
                segment_role(data, &segment.language),
                segment.language,
                segment.text
            ));
        }
    }
    if !data.summaries.is_empty() {
        output.push_str("\nSummaries\n");
        for summary in &data.summaries {
            let content = safe_summary_json(&summary.structured_json)
                .map(|value| serde_json::to_string(&value).unwrap_or_default())
                .unwrap_or_default();
            output.push_str(&format!("Version {}: {}\n", summary.version, content));
        }
    }
    output
}
