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

pub(crate) fn safe_summary_json(raw: &str) -> Option<serde_json::Value> {
    fn sanitize(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut safe = serde_json::Map::new();
                for (key, value) in object {
                    let normalized: String = key
                        .chars()
                        .filter(|character| character.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect();
                    let sensitive = normalized.contains("secret")
                        || normalized.contains("token")
                        || normalized.contains("password")
                        || normalized.contains("passphrase")
                        || normalized.contains("passcode")
                        || normalized == "passwd"
                        || normalized == "pwd"
                        || normalized == "key"
                        || normalized.contains("apikey")
                        || normalized.contains("accesskey")
                        || normalized.contains("clientkey")
                        || normalized.contains("privatekey")
                        || normalized.contains("signingkey")
                        || normalized.contains("encryptionkey")
                        || normalized.contains("credential")
                        || normalized.contains("cookie")
                        || normalized.contains("sessionid")
                        || normalized.contains("authorization")
                        || normalized == "auth"
                        || normalized.contains("bearer")
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
