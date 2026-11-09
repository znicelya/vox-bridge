use super::{
    ArtifactWriter, segment_role, size_error, timestamp, validate_summary_projection, write_summary,
};
use crate::{error::AppResult, storage::ExportData};
use std::io::Write;
pub fn render_plain_text(data: &ExportData) -> AppResult<String> {
    validate_summary_projection(data)?;
    let mut output = ArtifactWriter::new();
    let write = (|| -> std::io::Result<()> {
        for segment in data.segments.iter().filter(|s| s.is_final) {
            writeln!(
                output,
                "[{}] [{}] {} ({}): {}",
                timestamp(segment.occurred_at),
                segment.direction,
                segment_role(data, segment),
                segment.language,
                segment.text
            )?;
        }
        if !data.summaries.is_empty() {
            write!(output, "\nSummaries\n")?;
            for summary in &data.summaries {
                write_summary(&mut output, summary, false)?;
            }
        }
        Ok(())
    })();
    write.map_err(|_| size_error())?;
    output.finish()
}
