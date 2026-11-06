use super::command_error;
use crate::{
    app::{AppState, SubscriptionTicket},
    error::AppResult,
    intelligence::{subscriptions::IntelligenceEmitter, types::*},
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
#[tauri::command]
pub async fn list_model_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ModelProfileView>, String> {
    state.list_model_profiles().await.map_err(command_error)
}
#[tauri::command]
pub async fn save_model_profile(
    state: State<'_, AppState>,
    request: SaveModelProfile,
) -> Result<ModelProfileView, String> {
    state
        .save_model_profile(request)
        .await
        .map_err(command_error)
}
#[tauri::command]
pub async fn delete_model_profile(
    state: State<'_, AppState>,
    profile_id: Uuid,
) -> Result<(), String> {
    state
        .delete_model_profile(profile_id)
        .await
        .map_err(command_error)
}
#[tauri::command]
pub async fn get_intelligence_settings(
    state: State<'_, AppState>,
) -> Result<IntelligenceSettings, String> {
    state
        .get_intelligence_settings()
        .await
        .map_err(command_error)
}
#[tauri::command]
pub fn get_managed_data_location(
    state: State<'_, AppState>,
) -> Result<ManagedDataLocation, String> {
    Ok(state.get_managed_data_location())
}
#[tauri::command]
pub async fn save_intelligence_settings(
    state: State<'_, AppState>,
    settings: IntelligenceSettings,
) -> Result<IntelligenceSettings, String> {
    state
        .save_intelligence_settings(settings)
        .await
        .map_err(command_error)
}
#[tauri::command]
pub async fn test_model_profile(
    state: State<'_, AppState>,
    profile_id: Uuid,
) -> Result<ConnectionProbe, String> {
    state
        .test_model_profile(profile_id)
        .await
        .map_err(command_error)
}
#[tauri::command]
pub async fn test_translation_provider(
    state: State<'_, AppState>,
    profile_id: Uuid,
) -> Result<ConnectionProbe, String> {
    state
        .test_translation_provider(profile_id)
        .await
        .map_err(command_error)
}
#[tauri::command]
pub fn get_job(state: State<'_, AppState>, job_id: Uuid) -> Result<Option<JobView>, String> {
    state.get_job(job_id).map_err(command_error)
}
#[tauri::command]
pub fn list_jobs(
    state: State<'_, AppState>,
    after: Option<i64>,
    limit: Option<u32>,
) -> Result<JobPage, String> {
    state.list_jobs(after, limit).map_err(command_error)
}
#[tauri::command]
pub async fn cancel_job(state: State<'_, AppState>, job_id: Uuid) -> Result<JobView, String> {
    state.cancel_job(job_id).await.map_err(command_error)
}
struct TauriEmitter<R: tauri::Runtime>(AppHandle<R>);
#[async_trait::async_trait]
impl<R: tauri::Runtime> IntelligenceEmitter for TauriEmitter<R> {
    async fn emit(&self, name: &str, event: &IntelligenceEvent) -> AppResult<()> {
        self.0
            .emit(name, event)
            .map_err(|_| crate::error::AppError::Intelligence(IntelligenceErrorCode::Interrupted))
    }
}
#[tauri::command]
pub async fn subscribe_intelligence<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    subscriber_id: String,
) -> Result<SubscriptionTicket, String> {
    state
        .subscribe_intelligence(subscriber_id, Arc::new(TauriEmitter(app)))
        .await
        .map_err(command_error)
}
#[tauri::command]
pub async fn unsubscribe_intelligence(
    state: State<'_, AppState>,
    subscription_id: Uuid,
) -> Result<(), String> {
    state
        .unsubscribe_intelligence(subscription_id)
        .await
        .map_err(command_error)
}

pub(crate) const COMMANDS: &[&str] = &[
    "list_model_profiles",
    "save_model_profile",
    "delete_model_profile",
    "get_intelligence_settings",
    "get_managed_data_location",
    "save_intelligence_settings",
    "test_model_profile",
    "test_translation_provider",
    "get_job",
    "list_jobs",
    "cancel_job",
    "subscribe_intelligence",
    "unsubscribe_intelligence",
];
pub(crate) fn handle<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
    let handler: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
        list_model_profiles,
        save_model_profile,
        delete_model_profile,
        get_intelligence_settings,
        get_managed_data_location,
        save_intelligence_settings,
        test_model_profile,
        test_translation_provider,
        get_job,
        list_jobs,
        cancel_job,
        subscribe_intelligence,
        unsubscribe_intelligence
    ];
    handler(invoke)
}
#[cfg(test)]
mod tests;
