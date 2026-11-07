use crate::{
    app::AppState,
    intelligence::types::JobView,
    knowledge::types::{DocumentDetail, DocumentPage, KnowledgeResults, KnowledgeSearch},
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
#[tauri::command]
pub async fn search_knowledge(
    state: State<'_, AppState>,
    request: KnowledgeSearch,
) -> Result<KnowledgeResults, String> {
    state
        .search_knowledge(request)
        .await
        .map_err(super::command_error)
}
#[tauri::command]
pub async fn reindex_document(
    state: State<'_, AppState>,
    document_id: Uuid,
) -> Result<JobView, String> {
    state
        .reindex_document(document_id)
        .await
        .map_err(super::command_error)
}
#[tauri::command]
pub async fn delete_document(
    state: State<'_, AppState>,
    document_id: Uuid,
) -> Result<JobView, String> {
    state
        .delete_document(document_id)
        .await
        .map_err(super::command_error)
}
pub(crate) const COMMANDS: &[&str] = &[
    "import_document",
    "replace_document",
    "list_documents",
    "get_document",
    "search_knowledge",
    "reindex_document",
    "delete_document",
];
pub(crate) fn handle<R: tauri::Runtime>(invoke: tauri::ipc::Invoke<R>) -> bool {
    let handler: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
        import_document,
        replace_document,
        list_documents,
        get_document,
        search_knowledge,
        reindex_document,
        delete_document
    ];
    handler(invoke)
}
#[cfg(test)]
mod tests;
