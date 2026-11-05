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
    if let Ok(metadata) = fs::metadata(destination) {
        if metadata.permissions().readonly() {
            return Err(AppError::InvalidRequest(
                "destination is not writable".into(),
            ));
        }
    }
    let data = store
        .get_export_data(session_id)?
        .ok_or(AppError::SessionNotFound(session_id))?;
    let output = match format {
        ExportFormat::Markdown => markdown::render_markdown(&data),
        ExportFormat::Json => json::render_json(&data)?,
        ExportFormat::PlainText => plain_text::render_plain_text(&data),
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
        fs::rename(&temp, destination)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result.map_err(|error| AppError::Internal(format!("failed to write export: {error}")))?;
    Ok(destination.to_path_buf())
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

pub(crate) fn segment_role(data: &ExportData, language: &str) -> &'static str {
    if language == data.session.target_language {
        "translation"
    } else {
        "source"
    }
}

pub(crate) fn safe_summary_json(raw: &str) -> Option<serde_json::Value> {
    fn sanitize(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut safe = serde_json::Map::new();
                for (key, value) in object {
                    let normalized = key.to_ascii_lowercase().replace(['_', '-', '.'], "");
                    let sensitive = normalized.contains("secret")
                        || normalized.contains("token")
                        || normalized == "apikey"
                        || (normalized.starts_with("audio")
                            && ["path", "uri", "file", "recording"]
                                .iter()
                                .any(|part| normalized.ends_with(part)));
                    if !sensitive {
                        safe.insert(key, sanitize(value));
                    }
                }
                serde_json::Value::Object(safe)
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sanitize).collect())
            }
            other => other,
        }
    }

    serde_json::from_str(raw).ok().map(sanitize)
}
