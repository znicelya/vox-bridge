pub(crate) mod driver;
pub(crate) mod intelligence;
pub(crate) mod knowledge;
pub(crate) mod subscriptions;
pub(crate) mod summary;
use crate::{
    app::{
        AppState, LaunchValidation, ProviderConfigView, SaveProviderRequest, SubscriptionTicket,
    },
    audio::platform::{DriverStatus, inventory::AudioInventory},
    domain::{
        launch::SessionLaunch,
        session::SessionDirection,
        transcript::{DiagnosticPage, TranscriptPage},
    },
    error::{AppError, AppResult},
    export::ExportFormat,
    providers::ProviderCapabilities,
    session::SessionRecord,
};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
pub(crate) fn command_error(error: AppError) -> String {
    match error {
        AppError::Intelligence(code) => format!("[INTELLIGENCE_ERROR][{}] {}", code.as_str(), code),
        other => format!("[{}] {}", other.code(), other),
    }
}

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
pub async fn list_audio_endpoints(state: State<'_, AppState>) -> Result<AudioInventory, String> {
    state.list_audio_endpoints().await.map_err(command_error)
}

#[tauri::command]
pub async fn audio_driver_status(state: State<'_, AppState>) -> Result<DriverStatus, String> {
    state.audio_driver_status().await.map_err(command_error)
}

#[tauri::command]
pub async fn list_provider_configs(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderConfigView>, String> {
    state.list_provider_configs().await.map_err(command_error)
}

#[tauri::command]
pub async fn save_provider_config(
    state: State<'_, AppState>,
    request: SaveProviderRequest,
) -> Result<ProviderConfigView, String> {
    state
        .save_provider_config(request)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn delete_provider_config(
    state: State<'_, AppState>,
    config_id: Uuid,
) -> Result<(), String> {
    state
        .delete_provider_config(config_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn provider_capabilities(
    state: State<'_, AppState>,
    config_id: Uuid,
) -> Result<ProviderCapabilities, String> {
    state
        .provider_capabilities(config_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn validate_session_launch(
    state: State<'_, AppState>,
    launch: SessionLaunch,
) -> Result<LaunchValidation, String> {
    state
        .validate_session_launch(launch)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    launch: SessionLaunch,
) -> Result<SessionRecord, String> {
    state.create_session(launch).await.map_err(command_error)
}

#[tauri::command]
pub async fn stop_session(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<SessionRecord, String> {
    state.stop_session(session_id).await.map_err(command_error)
}

#[tauri::command]
pub async fn resume_session_direction(
    state: State<'_, AppState>,
    session_id: Uuid,
    direction: SessionDirection,
) -> Result<(), String> {
    state
        .resume_session_direction(session_id, direction)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn get_session(
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<Option<SessionRecord>, String> {
    state.get_session(session_id).await.map_err(command_error)
}

#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> Result<Vec<SessionRecord>, String> {
    state
        .list_sessions(limit, cursor)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: Uuid) -> Result<(), String> {
    state
        .delete_session(session_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn unsubscribe_session(
    state: State<'_, AppState>,
    subscription_id: Uuid,
) -> Result<(), String> {
    state
        .unsubscribe_session(subscription_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn export_session(
    state: State<'_, AppState>,
    session_id: Uuid,
    format: ExportFormat,
    destination: std::path::PathBuf,
) -> Result<std::path::PathBuf, String> {
    state
        .export_session(session_id, format, destination)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub fn get_session_transcript(
    state: State<'_, AppState>,
    session_id: Uuid,
    after: Option<i64>,
    limit: Option<u32>,
) -> Result<TranscriptPage, String> {
    state
        .get_session_transcript(session_id, after, limit)
        .map_err(command_error)
}

#[tauri::command]
pub fn get_session_diagnostics(
    state: State<'_, AppState>,
    session_id: Uuid,
    after: Option<i64>,
    limit: Option<u32>,
) -> Result<DiagnosticPage, String> {
    state
        .get_session_diagnostics(session_id, after, limit)
        .map_err(command_error)
}

struct TauriEmitter(AppHandle);
#[async_trait::async_trait]
impl subscriptions::FrontendEmitter for TauriEmitter {
    async fn emit(&self, event_name: &str, event: &crate::events::SessionEvent) -> AppResult<()> {
        self.0
            .emit(event_name, event)
            .map_err(|_| AppError::Internal("frontend emit failed".into()))
    }
}
#[tauri::command]
pub async fn subscribe_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Uuid,
    subscriber_id: String,
) -> Result<SubscriptionTicket, String> {
    state
        .subscribe_session(
            session_id,
            subscriber_id,
            std::sync::Arc::new(TauriEmitter(app)),
        )
        .await
        .map_err(command_error)
}
#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "commands/wire_contract_tests.rs"]
mod wire_contract_tests;
