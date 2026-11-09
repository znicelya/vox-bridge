use super::command_error;
use crate::{
    app::AppState,
    intelligence::types::JobView,
    summary::types::{SummaryCursor, SummaryPage},
};
use tauri::State;
use uuid::Uuid;
#[tauri::command]
pub async fn generate_summary(
    state: State<'_, AppState>,
    session_id: Uuid,
    profile_id: Option<Uuid>,
) -> Result<JobView, String> {
    state
        .generate_summary(session_id, profile_id)
        .await
        .map_err(command_error)
}
#[tauri::command]
pub fn list_summaries(
    state: State<'_, AppState>,
    session_id: Uuid,
    after_version: Option<SummaryCursor>,
    limit: Option<u32>,
) -> Result<SummaryPage, String> {
    state
        .list_summaries(session_id, after_version, limit)
        .map_err(command_error)
}
#[tauri::command]
pub async fn delete_summary(state: State<'_, AppState>, summary_id: Uuid) -> Result<(), String> {
    state
        .delete_summary(summary_id)
        .await
        .map_err(command_error)
}
pub(crate) const COMMANDS: &[&str] = &["generate_summary", "list_summaries", "delete_summary"];
pub(crate) fn handle<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
    let handler: fn(tauri::ipc::Invoke<R>) -> bool =
        tauri::generate_handler![generate_summary, list_summaries, delete_summary];
    handler(invoke)
}
#[cfg(test)]
mod tests;
