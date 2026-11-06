use super::*;
use crate::intelligence::{
    test_support::IntelligenceHarness,
    types::{IntelligenceErrorCode as Code, JobKind, JobStatus},
};
use crate::knowledge::{
    documents::DocumentService,
    parser::DocumentParser,
    types::{DocumentKind, DocumentStatus, TextBlock, TextLocation},
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::watch;

struct TextParser;
#[async_trait::async_trait]
impl DocumentParser for TextParser {
    async fn parse(
        &self,
        path: PathBuf,
        _kind: DocumentKind,
        _cancel: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code> {
        let text = std::fs::read_to_string(path).map_err(|_| Code::ParseFailed)?;
        if text == "fail" {
            return Err(Code::ParseFailed);
        }
        let end = text.chars().count() as u32;
        Ok(vec![TextBlock {
            text,
            location: TextLocation {
                page: None,
                paragraph: 1,
                heading: Some("完整 heading".into()),
                start_char: 0,
                end_char: end,
            },
        }])
    }
}
fn service(h: &IntelligenceHarness) -> DocumentService {
    DocumentService::new(
        h.store.clone(),
        h.config.clone(),
        h.jobs.clone(),
        Arc::new(TextParser),
        h.temp_dir.path().into(),
    )
}
async fn wait(h: &IntelligenceHarness, id: Uuid, want: JobStatus) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if h.store.get_job(id).unwrap().unwrap().status == want {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn source(h: &IntelligenceHarness, name: &str, text: &str) -> PathBuf {
    let path = h.temp_dir.path().join(name);
    std::fs::write(&path, text).unwrap();
    path
}
fn files(h: &IntelligenceHarness) -> u32 {
    h.store
        .transaction(|tx| {
            Ok(
                tx.query_row("SELECT count(*) FROM knowledge_managed_files", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap()
}

struct CollidingCopier;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn documents_atomic_duplicate_imports_share_job_and_parser_identity_invalidates_dedup() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    let permit = h.jobs.parser_gate().acquire_owned().await.unwrap();
    let path = source(&h, "same.txt", "same effective content");
    let (first, second) = tokio::join!(docs.import(path.clone()), docs.import(path.clone()));
    let first = first.unwrap();
    assert_eq!(second.unwrap().id, first.id);
    assert_eq!(docs.page(None, 64).unwrap().rows.len(), 1);
    assert_eq!(
        files(&h),
        1,
        "duplicate candidate must be removed after atomic admission"
    );
    drop(permit);
    wait(&h, first.id, JobStatus::Succeeded).await;
    let id = first.object_id.unwrap();
    h.store.transaction(|tx| {
        tx.execute("UPDATE documents SET parser_identity='prior-parser' WHERE id=?1", [id.to_string()])?;
        tx.execute("UPDATE knowledge_document_imports SET parser_identity='prior-parser' WHERE document_id=?1", [id.to_string()])?;
        Ok(())
    }).unwrap();
    let current = docs.import(path).await.unwrap();
    assert_ne!(
        current.id, first.id,
        "SHA alone cannot deduplicate content parsed by a prior parser"
    );
    assert_ne!(current.object_id, first.object_id);
    wait(&h, current.id, JobStatus::Succeeded).await;
    assert_eq!(docs.page(None, 64).unwrap().rows.len(), 2);
    assert_eq!(files(&h), 2);
    h.jobs.shutdown().await.unwrap();
}

impl crate::knowledge::copying::FileCopier for CollidingCopier {
    fn copy(
        &self,
        request: crate::knowledge::copying::CopyRequest,
    ) -> crate::error::AppResult<crate::knowledge::copying::CopiedFile> {
        let name = format!("{}.pdf", request.file_id);
        request.store.transaction(|tx|{tx.execute("INSERT INTO knowledge_managed_files (file_id,relative_name,state,created_at_ms) VALUES (?1,?2,'copying',37)",rusqlite::params![request.file_id.to_string(),name])?;Ok(())}).unwrap();
        crate::knowledge::copying::ManagedCopier.copy(request)
    }
}
#[tokio::test]
async fn documents_copy_intent_service_does_not_claim_a_failed_copiers_unowned_row() {
    let h = IntelligenceHarness::new();
    let mut docs = service(&h);
    docs.copier = Arc::new(CollidingCopier);
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    assert!(matches!(
        docs.import(source(&h, "new.txt", "new source")).await,
        Err(crate::error::AppError::Intelligence(Code::Storage))
    ));
    let rows: Vec<(String, i64)> = h
        .store
        .transaction(|tx| {
            let mut statement =
                tx.prepare("SELECT state,created_at_ms FROM knowledge_managed_files")?;
            let rows = statement
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(
        rows,
        vec![("copying".into(), 37)],
        "service must not clean a candidate that its copier never returned"
    );
    assert!(h.store.list_jobs(None, 200).unwrap().rows.is_empty());
    h.jobs.shutdown().await.unwrap();
}

#[tokio::test]
async fn documents_admission_commits_local_chunks_and_deduplicates_same_parser_content() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    h.jobs.register(JobKind::Import, docs.handler()).unwrap();
    h.jobs.start().unwrap();
    let path = source(&h, "manual.txt", "额定电压 24 V");
    let job = docs
        .import(path.clone())
        .await
        .expect("valid import must admit");
    let duplicate = docs.import(path.clone()).await.unwrap();
    assert_eq!(duplicate.id, job.id);
    wait(&h, job.id, JobStatus::Succeeded).await;
    let id = job.object_id.unwrap();
    let detail = docs.get(id).unwrap().unwrap();
    assert_eq!(detail.summary.status, DocumentStatus::Parsed);
    assert_eq!(detail.summary.version, 1);
    assert_eq!(detail.summary.chunk_count, 1);
    assert!(detail.summary.updated_at_ms.is_some());
    assert_eq!(detail.index_identity, None);
    assert_eq!(docs.import(path).await.unwrap().id, job.id);
    assert_eq!(files(&h), 1);
    h.store.transaction(|tx|{let (text,location,hash,version,state):(String,String,String,u32,String)=tx.query_row("SELECT text,location_json,content_hash,document_version,index_state FROM document_chunks WHERE document_id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;assert_eq!(text,"额定电压 24 V");assert_eq!(serde_json::from_str::<TextLocation>(&location).unwrap().heading.as_deref(),Some("完整 heading"));assert_eq!(hash.len(),64);assert_eq!(version,1);assert_eq!(state,"parsed");Ok(())}).unwrap();
    let saved = detail.summary.updated_at_ms;
    assert_eq!(docs.page(None, 64).unwrap().rows[0].updated_at_ms, saved);
    h.jobs.shutdown().await.unwrap();
}

#[tokio::test]
async fn documents_admission_busy_preserves_versions_and_duplicate_survives_full_queue() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    h.jobs.register(JobKind::Import, docs.handler()).unwrap();
    h.jobs.start().unwrap();
    let permit = h.jobs.parser_gate().acquire_owned().await.unwrap();
    let mut jobs = Vec::new();
    for i in 0..32 {
        jobs.push(
            docs.import(source(&h, &format!("{i}.txt"), &format!("unique text {i}")))
                .await
                .unwrap(),
        );
    }
    let before = docs.get(jobs[0].object_id.unwrap()).unwrap().unwrap();
    assert_eq!(
        docs.import(h.temp_dir.path().join("0.txt"))
            .await
            .unwrap()
            .id,
        jobs[0].id
    );
    assert!(matches!(
        docs.replace(
            before.summary.id,
            source(&h, "replacement.txt", "new content")
        )
        .await,
        Err(crate::error::AppError::Intelligence(Code::Busy))
    ));
    assert_eq!(docs.get(before.summary.id).unwrap(), Some(before));
    assert_eq!(docs.page(None, 64).unwrap().rows.len(), 32);
    assert_eq!(files(&h), 32);
    h.jobs.shutdown().await.unwrap();
    drop(permit);
}

#[tokio::test]
async fn documents_admission_replacement_keeps_requested_uuid_and_explicit_failed_retry_advances() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    h.jobs.register(JobKind::Import, docs.handler()).unwrap();
    h.jobs.start().unwrap();
    let first = docs
        .import(source(&h, "first.txt", "first source"))
        .await
        .unwrap();
    wait(&h, first.id, JobStatus::Succeeded).await;
    let other = docs
        .import(source(&h, "other.txt", "other source"))
        .await
        .unwrap();
    wait(&h, other.id, JobStatus::Succeeded).await;
    let id = first.object_id.unwrap();
    let job = docs
        .replace(id, h.temp_dir.path().join("other.txt"))
        .await
        .unwrap();
    assert_eq!(job.object_id, Some(id));
    assert_ne!(job.id, other.id);
    wait(&h, job.id, JobStatus::Succeeded).await;
    assert_eq!(docs.get(id).unwrap().unwrap().summary.version, 2);
    assert_eq!(
        docs.replace(id, h.temp_dir.path().join("other.txt"))
            .await
            .unwrap()
            .id,
        job.id
    );
    let failed = docs
        .replace(id, source(&h, "failure.txt", "fail"))
        .await
        .unwrap();
    wait(&h, failed.id, JobStatus::Failed).await;
    let detail = docs.get(id).unwrap().unwrap();
    assert_eq!(detail.summary.version, 3);
    assert_eq!(detail.summary.status, DocumentStatus::Failed);
    assert_eq!(detail.managed_path, None);
    assert!(h.store.transaction(|tx|Ok(tx.query_row("SELECT EXISTS(SELECT 1 FROM document_chunks WHERE document_id=?1 AND text='other source' AND index_state!='ready')",[id.to_string()],|r|r.get::<_,bool>(0))?)).unwrap());
    let retry = docs
        .replace(id, h.temp_dir.path().join("failure.txt"))
        .await
        .unwrap();
    assert_eq!(retry.object_id, Some(id));
    assert_ne!(retry.id, failed.id);
    assert_eq!(docs.get(id).unwrap().unwrap().summary.version, 4);
    wait(&h, retry.id, JobStatus::Failed).await;
    assert!(
        files(&h) >= 3,
        "earlier successful source associations survive replacement failure"
    );
    h.jobs.shutdown().await.unwrap();
}

#[tokio::test]
async fn documents_completion_same_content_noop_at_maximum_document_version() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    let path = source(&h, "source.txt", "same content");
    let job = docs.import(path.clone()).await.unwrap();
    wait(&h, job.id, JobStatus::Succeeded).await;
    let id = job.object_id.unwrap();
    h.store.transaction(|tx|{tx.execute("UPDATE documents SET version=4294967295 WHERE id=?1",[id.to_string()])?;tx.execute("UPDATE knowledge_document_imports SET document_version=4294967295 WHERE document_id=?1",[id.to_string()])?;tx.execute("UPDATE document_chunks SET document_version=4294967295 WHERE document_id=?1",[id.to_string()])?;Ok(())}).unwrap();
    assert_eq!(
        docs.replace(id, path).await.unwrap().id,
        job.id,
        "no-op must precede an unnecessary version increment"
    );
    assert_eq!(docs.get(id).unwrap().unwrap().summary.version, u32::MAX);
    assert_eq!(files(&h), 1);
    h.jobs.shutdown().await.unwrap();
}

#[tokio::test]
async fn documents_completion_admission_rollback_publishes_no_job_and_preserves_old_metadata() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    let old = docs
        .import(source(&h, "source.txt", "old text"))
        .await
        .unwrap();
    wait(&h, old.id, JobStatus::Succeeded).await;
    let id = old.object_id.unwrap();
    let before = docs.get(id).unwrap().unwrap();
    h.store.transaction(|tx|{tx.execute_batch("CREATE TRIGGER reject_version BEFORE UPDATE OF version ON documents BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;")?;Ok(())}).unwrap();
    let mut events = h.jobs.subscribe();
    assert!(matches!(
        docs.replace(id, source(&h, "new.txt", "new text")).await,
        Err(crate::error::AppError::Intelligence(Code::Storage))
    ));
    assert_eq!(docs.get(id).unwrap(), Some(before));
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 1);
    assert_eq!(files(&h), 1);
    assert!(events.try_recv().is_err());
    h.store
        .transaction(|tx| {
            let text: String = tx.query_row(
                "SELECT text FROM document_chunks WHERE document_id=?1",
                [id.to_string()],
                |r| r.get(0),
            )?;
            assert_eq!(text, "old text");
            Ok(())
        })
        .unwrap();
    h.jobs.shutdown().await.unwrap();
}

#[tokio::test]
async fn documents_completion_failed_chunk_commit_keeps_old_text_and_truthful_progress() {
    let h = IntelligenceHarness::new();
    let docs = service(&h);
    docs.register_import_handler().unwrap();
    h.jobs.start().unwrap();
    let old = docs
        .import(source(&h, "source.txt", "old text"))
        .await
        .unwrap();
    wait(&h, old.id, JobStatus::Succeeded).await;
    let id = old.object_id.unwrap();
    h.store.transaction(|tx|{tx.execute_batch("CREATE TRIGGER reject_new_chunk BEFORE INSERT ON document_chunks WHEN NEW.text='new text' BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;")?;Ok(())}).unwrap();
    let new = docs
        .replace(id, source(&h, "new.txt", "new text"))
        .await
        .unwrap();
    wait(&h, new.id, JobStatus::Failed).await;
    let job = h.store.get_job(new.id).unwrap().unwrap();
    assert_eq!(job.completed_units, 0);
    assert_eq!(job.total_units, Some(1));
    let detail = docs.get(id).unwrap().unwrap();
    assert_eq!(detail.summary.status, DocumentStatus::Failed);
    assert_eq!(detail.summary.version, 2);
    assert_eq!(detail.managed_path, None);
    h.store
        .transaction(|tx| {
            let (text, state): (String, String) = tx.query_row(
                "SELECT text,index_state FROM document_chunks WHERE document_id=?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            assert_eq!(text, "old text");
            assert_eq!(state, "stale");
            Ok(())
        })
        .unwrap();
    h.jobs.shutdown().await.unwrap();
    assert_eq!(files(&h), 1);
}
