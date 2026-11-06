use super::*;
use crate::intelligence::types::JobView;
use crate::knowledge::types::{DocumentDetail, DocumentPage};
impl AppState {
    pub async fn import_document(&self, path: PathBuf) -> AppResult<JobView> {
        self.documents
            .import_owned(path, self.intelligence_lease()?)
            .await
    }
    pub async fn replace_document(&self, id: Uuid, path: PathBuf) -> AppResult<JobView> {
        self.documents
            .replace_owned(id, path, self.intelligence_lease()?)
            .await
    }
    pub fn list_documents(&self, after: Option<i64>, limit: u32) -> AppResult<DocumentPage> {
        self.documents.page(after, limit)
    }
    pub fn get_document(&self, id: Uuid) -> AppResult<Option<DocumentDetail>> {
        self.documents.get(id)
    }
}
#[cfg(test)]
mod tests;
