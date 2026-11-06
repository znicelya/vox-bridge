pub(crate) mod subscriptions;

use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::{
    app::AppState, domain::launch::SessionLaunch, error::AppError, export::ExportFormat,
    session::SessionRecord,
};

fn command_error(error: AppError) -> String {
    format!("[{}] {}", error.code(), error)
}

/// Development-only synthetic diagnostics; never opens audio hardware or a provider.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn dev_audio_loopback(
    frames: Option<u32>,
) -> Result<crate::audio::loopback::SyntheticReport, String> {
    crate::audio::loopback::run_synthetic(frames.unwrap_or(50))
        .await
        .map_err(command_error)
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
    launch: SessionLaunch,
) -> Result<SessionRecord, String> {
    let config_id = launch.provider_config_id;
    let saved = state
        .config_io()
        .run(move |transaction| transaction.load_provider_config(config_id))
        .await
        .map_err(command_error)?;
    state
        .orchestrator()
        .start(launch, saved)
        .await
        .map_err(command_error)
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
