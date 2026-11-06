use crate::{
    app::AppState,
    intelligence::types::JobView,
    knowledge::types::{DocumentDetail, DocumentPage},
};
use std::path::PathBuf;
use tauri::State;
use uuid::Uuid;
#[tauri::command]
pub async fn import_document(state: State<'_, AppState>, path: PathBuf) -> Result<JobView, String> {
    state
        .import_document(path)
        .await
        .map_err(super::command_error)
}
#[tauri::command]
pub async fn replace_document(
    state: State<'_, AppState>,
    document_id: Uuid,
    path: PathBuf,
) -> Result<JobView, String> {
    state
        .replace_document(document_id, path)
        .await
        .map_err(super::command_error)
}
#[tauri::command]
pub fn list_documents(
    state: State<'_, AppState>,
    after: Option<i64>,
    limit: u32,
) -> Result<DocumentPage, String> {
    state
        .list_documents(after, limit)
        .map_err(super::command_error)
}
#[tauri::command]
pub fn get_document(
    state: State<'_, AppState>,
    document_id: Uuid,
) -> Result<Option<DocumentDetail>, String> {
    state
        .get_document(document_id)
        .map_err(super::command_error)
}
pub(crate) const COMMANDS: &[&str] = &[
    "import_document",
    "replace_document",
    "list_documents",
    "get_document",
];
pub(crate) fn handle<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
    let handler: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
        import_document,
        replace_document,
        list_documents,
        get_document
    ];
    handler(invoke)
}
#[cfg(test)]
mod tests;
