use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::{
    app::AppState, domain::session::SessionConfig, error::AppError, export::ExportFormat,
    session::SessionRecord,
};

fn command_error(error: AppError) -> String {
    format!("[{}] {}", error.code(), error)
}

#[tauri::command]
pub fn export_session(
    state: State<'_, AppState>,
    session_id: Uuid,
    format: ExportFormat,
    destination: std::path::PathBuf,
) -> Result<std::path::PathBuf, String> {
    crate::export::export_session(state.store(), session_id, format, &destination)
        .map_err(command_error)
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    config: SessionConfig,
) -> Result<SessionRecord, String> {
    let handle = state
        .orchestrator()
        .start(config)
        .await
        .map_err(command_error)?;
    state
        .orchestrator()
        .get(handle.id())
        .await
        .map_err(command_error)?
        .ok_or_else(|| "[INTERNAL_ERROR] created session disappeared".into())
}

#[tauri::command]
pub async fn stop_session(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<SessionRecord, String> {
    state
        .orchestrator()
        .stop_session(session_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<Vec<SessionRecord>, String> {
    state
        .orchestrator()
        .list(limit.unwrap_or(50).min(200), cursor)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn get_session(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<Option<SessionRecord>, String> {
    state
        .orchestrator()
        .get(session_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: Uuid) -> Result<(), String> {
    state
        .orchestrator()
        .delete_session(session_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn subscribe_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<String, String> {
    let event_name = format!("voxbridge://session/{session_id}");
    let mut receiver = state
        .orchestrator()
        .subscribe(session_id)
        .await
        .map_err(command_error)?;
    let app = app.clone();
    let name = event_name.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = receiver.recv().await {
            if app.emit(&name, event).is_err() {
                break;
            }
        }
    });
    Ok(event_name)
}
