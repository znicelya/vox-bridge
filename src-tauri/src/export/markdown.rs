use super::{
    ArtifactWriter, segment_role, size_error, timestamp, validate_summary_projection, write_summary,
};
use crate::{error::AppResult, storage::ExportData};
use std::io::Write;
pub fn render_markdown(data: &ExportData) -> AppResult<String> {
    validate_summary_projection(data)?;
    let mut output = ArtifactWriter::new();
    let write = (|| -> std::io::Result<()> {
        write!(
            output,
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
        )?;
        for segment in data.segments.iter().filter(|s| s.is_final) {
            writeln!(
                output,
                "- {} {} **{}** ({}): {}",
                timestamp(segment.occurred_at),
                segment.direction,
                segment_role(data, segment),
                segment.language,
                segment.text
            )?;
        }
        if !data.summaries.is_empty() {
            write!(output, "\n## Summaries\n\n")?;
            for summary in &data.summaries {
                write_summary(&mut output, summary, true)?;
            }
        }
        Ok(())
    })();
    write.map_err(|_| size_error())?;
    output.finish()
}
