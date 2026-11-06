use super::*;
use crate::intelligence::{
    store::{JobDraft, admit_job},
    types::{IntelligenceErrorCode as Code, JobAdmission, JobKind, JobStatus},
};
use crate::knowledge::{
    PARSER_ID,
    parser::DocumentParser,
    types::{DocumentKind, DocumentStatus, TextBlock},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::watch;

struct NeverReplay(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl DocumentParser for NeverReplay {
    async fn parse(
        &self,
        _: PathBuf,
        _: DocumentKind,
        _: watch::Receiver<bool>,
    ) -> Result<Vec<TextBlock>, Code> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(Code::ParseFailed)
    }
}

#[tokio::test]
async fn documents_startup_existing_copy_and_reap_intents_reconcile_without_replay_twice() {
    let dir = tempfile::tempdir().unwrap();
    let managed = dir.path().join("knowledge/documents");
    std::fs::create_dir_all(&managed).unwrap();
    let original = dir.path().join("original.txt");
    std::fs::write(&original, "selected source stays").unwrap();
    let local = LocalStore::open(dir.path()).unwrap();
    let mut admitted = Vec::new();
    let mut disposable = Vec::new();
    for (status, state) in [("queued", "copying"), ("running", "awaiting_reap")] {
        let document = Uuid::new_v4();
        let file = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let name = format!("{file}.txt");
        let final_path = managed.join(&name);
        let temp = managed.join(format!("{file}.tmp"));
        std::fs::write(&final_path, "interrupted candidate").unwrap();
        std::fs::write(&temp, "unpublished tail").unwrap();
        let source_json = super::super::store::encode_path(&original).unwrap();
        let managed_json = super::super::store::encode_path(&final_path).unwrap();
        local.transaction(|tx| {
            admit_job(tx,JobDraft{id:job_id,kind:JobKind::Import,session_id:None,object_id:Some(document),admission:JobAdmission::Manual{exclusion_key:format!("document:{document}")},payload:serde_json::json!({"version":1,"profiles":[],"input":{"document_id":document,"document_version":1,"parser_identity":PARSER_ID}}),initial_failure:None})?;
            tx.execute("UPDATE intelligence_jobs SET status=?1 WHERE id=?2",params![status,job_id.to_string()])?;
            tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at,kind,origin,version,original_path,managed_path,status,parser_identity,import_job_id) VALUES (?1,?2,'managed-fixture','fixture',37,'text','native',1,?3,?4,'parsing',?5,?6)",params![document.to_string(),"a".repeat(64),source_json,managed_json,PARSER_ID,job_id.to_string()])?;
            tx.execute("INSERT INTO knowledge_managed_files (file_id,relative_name,document_id,document_version,job_id,state,created_at_ms) VALUES (?1,?2,?3,1,?4,?5,37)",params![file.to_string(),name,document.to_string(),job_id.to_string(),state])?;
            tx.execute("INSERT INTO knowledge_document_imports (document_id,document_version,job_id,parser_identity,sha256,file_id,created_at_ms) VALUES (?1,1,?2,?3,?4,?5,37)",params![document.to_string(),job_id.to_string(),PARSER_ID,"a".repeat(64),file.to_string()])?;Ok(())
        }).unwrap();
        admitted.push((document, job_id));
        disposable.extend([final_path, temp]);
    }
    let orphan = Uuid::new_v4();
    let orphan_name = format!("{orphan}.txt");
    for name in [&orphan_name, &format!("{orphan}.tmp")] {
        let path = managed.join(name);
        std::fs::write(&path, "unadmitted candidate").unwrap();
        disposable.push(path);
    }
    let retained_id = Uuid::new_v4();
    let retained_file = Uuid::new_v4();
    let retained_name = format!("{retained_file}.txt");
    let retained = managed.join(&retained_name);
    std::fs::write(&retained, "retained canonical source").unwrap();
    let retained_json = super::super::store::encode_path(&retained).unwrap();
    local.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_managed_files (file_id,relative_name,state,created_at_ms) VALUES (?1,?2,'copying',37)",params![orphan.to_string(),orphan_name])?;
        tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at,kind,origin,managed_path,status) VALUES (?1,?2,'managed-retained','retained',37,'text','native',?3,'parsed')",params![retained_id.to_string(),"b".repeat(64),retained_json])?;
        tx.execute("INSERT INTO knowledge_managed_files (file_id,relative_name,document_id,document_version,state,created_at_ms) VALUES (?1,?2,?3,1,'retained',37)",params![retained_file.to_string(),retained_name,retained_id.to_string()])?;Ok(())
    }).unwrap();
    let foreign = managed.join(format!("{}.txt", Uuid::new_v4()));
    std::fs::write(&foreign, "foreign UUID has no intent").unwrap();
    drop(local);

    let calls = Arc::new(AtomicUsize::new(0));
    let parser = Arc::new(NeverReplay(calls.clone()));
    let app = crate::app::document_test_support::with_parser(dir.path(), parser.clone());
    let mut snapshots = Vec::new();
    for (document, job_id) in &admitted {
        let detail = app.get_document(*document).unwrap().unwrap();
        let job = app.get_job(*job_id).unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Interrupted);
        assert_eq!(job.error, Some(Code::Interrupted));
        assert_eq!(detail.summary.status, DocumentStatus::Failed);
        assert_eq!(detail.summary.error, Some(Code::Interrupted));
        assert_eq!(detail.managed_path, None);
        assert_eq!(detail.original_path, Some(original.clone()));
        snapshots.push((detail, job));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(disposable.iter().all(|p| !p.exists()));
    assert_eq!(
        std::fs::read_to_string(&original).unwrap(),
        "selected source stays"
    );
    assert_eq!(
        std::fs::read_to_string(&foreign).unwrap(),
        "foreign UUID has no intent"
    );
    assert_eq!(
        std::fs::read_to_string(&retained).unwrap(),
        "retained canonical source"
    );
    let saved = app.get_document(retained_id).unwrap().unwrap();
    assert_eq!(saved.summary.status, DocumentStatus::Parsed);
    assert_eq!(saved.managed_path, Some(retained.clone()));
    assert_eq!(
        app.store()
            .transaction(|tx| Ok(tx.query_row(
                "SELECT count(*) FROM knowledge_managed_files",
                [],
                |r| r.get::<_, u32>(0)
            )?))
            .unwrap(),
        1
    );
    app.close_all().await.unwrap();
    drop(app);
    let reopened = crate::app::document_test_support::with_parser(dir.path(), parser);
    for (detail, job) in snapshots {
        assert_eq!(
            reopened.get_document(detail.summary.id).unwrap(),
            Some(detail)
        );
        assert_eq!(reopened.get_job(job.id).unwrap(), Some(job));
    }
    assert_eq!(reopened.get_document(retained_id).unwrap(), Some(saved));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read_dir(managed).unwrap().count(), 2);
    reopened.close_all().await.unwrap();
}
