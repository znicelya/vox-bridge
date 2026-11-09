#[cfg(test)]
mod export_tests;

pub mod json;
pub mod markdown;
pub mod plain_text;

use crate::{
    error::{AppError, AppResult},
    storage::{ExportData, LocalStore},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Markdown,
    Json,
    PlainText,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::PlainText => "txt",
        }
    }
}

pub fn export_session(
    store: &LocalStore,
    session_id: Uuid,
    format: ExportFormat,
    destination: &Path,
) -> AppResult<PathBuf> {
    if destination.as_os_str().is_empty() || destination.is_dir() {
        return Err(AppError::InvalidRequest(
            "destination must be a file path".into(),
        ));
    }
    let extension = destination
        .extension()
        .and_then(|v| v.to_str())
        .map(str::to_ascii_lowercase);
    if extension.as_deref() != Some(format.extension()) {
        return Err(AppError::InvalidRequest(format!(
            "destination extension must be .{}",
            format.extension()
        )));
    }
    if fs::metadata(destination).is_ok_and(|metadata| metadata.permissions().readonly()) {
        return Err(AppError::InvalidRequest(
            "destination is not writable".into(),
        ));
    }
    let data = store
        .get_export_data(session_id)?
        .ok_or(AppError::SessionNotFound(session_id))?;
    let output = match format {
        ExportFormat::Markdown => markdown::render_markdown(&data)?,
        ExportFormat::Json => json::render_json(&data)?,
        ExportFormat::PlainText => plain_text::render_plain_text(&data)?,
    };
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() || !parent.is_dir() {
        return Err(AppError::InvalidRequest(
            "destination directory does not exist".into(),
        ));
    }
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("export"),
        Uuid::new_v4()
    ));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        drop(file);
        replace_destination(&temp, destination)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result.map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            AppError::InvalidRequest("destination is not writable".into())
        } else {
            AppError::Internal(format!("failed to write export: {error}"))
        }
    })?;
    Ok(destination.to_path_buf())
}

#[cfg(not(windows))]
fn replace_destination(temp: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temp, destination)
}

#[cfg(windows)]
fn replace_destination(temp: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn timestamp(seconds: i64) -> String {
    let seconds = seconds.rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

pub(crate) fn segment_role(
    data: &ExportData,
    segment: &crate::storage::StoredTranscriptSegment,
) -> &'static str {
    use crate::domain::transcript::TranscriptKind;
    if let Some(details) = &segment.details {
        return match details.kind {
            TranscriptKind::Source => "source",
            TranscriptKind::Translation => "translation",
        };
    }
    let (source, target) = match segment.direction.as_str() {
        "source_to_target" => (&data.session.source_language, &data.session.target_language),
        "target_to_source" => (&data.session.target_language, &data.session.source_language),
        _ => return "unknown",
    };
    if source == target {
        "unknown"
    } else if &segment.language == source {
        "source"
    } else if &segment.language == target {
        "translation"
    } else {
        "unknown"
    }
}

#[cfg(test)]
pub(crate) fn safe_summary_json(raw: &str) -> Option<serde_json::Value> {
    crate::summary::content::legacy_json(raw).ok()
}

pub(crate) fn size_error() -> AppError {
    AppError::Intelligence(crate::intelligence::types::IntelligenceErrorCode::ResponseTooLarge)
}
pub(crate) struct ArtifactWriter {
    bytes: Vec<u8>,
}
impl ArtifactWriter {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    pub(crate) fn finish(self) -> AppResult<String> {
        String::from_utf8(self.bytes)
            .map_err(|_| AppError::Internal("invalid export encoding".into()))
    }
}
impl Write for ArtifactWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > 128 * 1024 * 1024)
        {
            return Err(std::io::Error::other("export artifact size limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn validate_summary_projection(data: &ExportData) -> AppResult<()> {
    use crate::summary::{
        content::{CONTENT_BYTES, json_size},
        types::SummaryOrigin,
    };
    let mut bytes = 2usize;
    for record in &data.summaries {
        let limit = match record.origin {
            SummaryOrigin::Native => CONTENT_BYTES + 64 * 1024,
            SummaryOrigin::Legacy => 8 * 1024 * 1024,
        };
        let size = json_size(record, limit).map_err(|_| size_error())?;
        bytes = bytes
            .checked_add(size + 1)
            .filter(|n| *n <= 32 * 1024 * 1024)
            .ok_or_else(size_error)?;
    }
    Ok(())
}
pub(crate) fn write_summary(
    output: &mut impl Write,
    summary: &crate::summary::types::SummaryExportRecord,
    markdown: bool,
) -> std::io::Result<()> {
    use crate::summary::types::{SummaryExportContent, SummaryOrigin};
    writeln!(
        output,
        "{}Version {}",
        if markdown { "### " } else { "" },
        summary.version
    )?;
    writeln!(
        output,
        "Summary ID: {}\nSession ID: {}",
        summary.id, summary.session_id
    )?;
    writeln!(
        output,
        "Status: {}\nOrigin: {}\nCreated at seconds: {}",
        summary.status,
        match summary.origin {
            SummaryOrigin::Native => "native",
            SummaryOrigin::Legacy => "legacy",
        },
        summary.created_at
    )?;
    if let Some(ms) = summary.created_at_ms {
        writeln!(output, "Created at milliseconds: {ms}")?;
    }
    if let Some(deleting) = summary.deleting {
        writeln!(output, "Deleting: {deleting}")?;
    }
    if let Some(profile) = summary.profile_id {
        writeln!(output, "Profile: {profile}")?;
    }
    if let Some(model) = &summary.model {
        writeln!(output, "Model: {model}")?;
    }
    if let Some(error) = summary.error {
        writeln!(output, "Error: {}", error.as_str())?;
    }
    if let Some(count) = summary.input_final_count {
        writeln!(output, "Final input rows: {count}")?;
    }
    if let Some(hash) = &summary.input_hash {
        writeln!(output, "Input hash: {hash}")?;
    }
    if let Some(hash) = &summary.raw_response_hash {
        writeln!(output, "Response hash: {hash}")?;
    }
    match &summary.content {
        Some(SummaryExportContent::Native(content)) => {
            writeln!(output, "\nSummary\n{}", content.summary)?;
            for (label, values) in [
                ("Participants", &content.participants),
                ("Companies", &content.companies),
                ("Requirements", &content.requirements),
                ("Risks", &content.risks),
            ] {
                writeln!(output, "\n{label}")?;
                for value in values {
                    writeln!(output, "- {value}")?;
                }
            }
            writeln!(output, "\nAction items")?;
            for action in &content.action_items {
                writeln!(
                    output,
                    "- {}\n  Owner: {}\n  Due: {}",
                    action.text,
                    action.owner.as_deref().unwrap_or("—"),
                    action.due.as_deref().unwrap_or("—")
                )?;
            }
            writeln!(output, "\nTerms")?;
            for term in &content.terms {
                writeln!(output, "- {}: {}", term.term, term.meaning)?;
            }
        }
        Some(SummaryExportContent::Legacy(content)) => {
            serde_json::to_writer_pretty(&mut *output, content).map_err(std::io::Error::other)?;
            writeln!(output)?;
        }
        None => writeln!(output, "Content: unavailable")?,
    }
    writeln!(output)
}
