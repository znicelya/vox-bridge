use super::super::{PARSER_ID, copying::CopiedFile};
use super::*;
use crate::intelligence::{
    jobs::JobAdmissionFence,
    store::{self as jobs_store, JobDraft},
    types::{JobAdmission, JobStatus},
};
use rusqlite::{OptionalExtension, params};
pub(super) fn admit(
    local: &LocalStore,
    fence: &JobAdmissionFence,
    replacement: Option<Uuid>,
    file_id: Uuid,
    copy: &CopiedFile,
    cancel: &watch::Receiver<bool>,
) -> AppResult<(JobView, bool)> {
    local.transaction(|tx|fence.run(||{
        if *cancel.borrow(){return Err(error(Code::Cancelled));}
        let mut version=1_u32;let mut prior_version=None;let id;let existing:Option<String>;
        if let Some(document)=replacement {
            let current:Option<(u32,String,Option<String>,String,Option<String>)>=tx.query_row("SELECT version,sha256,parser_identity,status,import_job_id FROM documents WHERE id=?1",[document.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            let (old,hash,parser,status,job)=current.ok_or_else(||error(Code::Configuration))?;
            id=document;prior_version=Some(old);
            existing=if hash==copy.sha256&&parser.as_deref()==Some(PARSER_ID)&&matches!(status.as_str(),"parsing"|"parsed"|"indexing"|"ready") {job}else{None};
        }else {
            id=Uuid::new_v4();
            existing=tx.query_row("SELECT i.job_id FROM documents d JOIN knowledge_document_imports i ON i.document_id=d.id AND i.document_version=d.version WHERE d.sha256=?1 AND d.parser_identity=?2 AND i.sha256=d.sha256 AND i.parser_identity=d.parser_identity ORDER BY d.rowid LIMIT 1",params![copy.sha256,PARSER_ID],|r|r.get(0)).optional()?;
        }
        if let Some(existing)=existing {
            let existing=Uuid::parse_str(&existing).map_err(|_|error(Code::Storage))?;
            if let Some(job)=jobs_store::job_in_transaction(tx,existing)? {
                if replacement.is_none()||matches!(job.status,JobStatus::Queued|JobStatus::Running|JobStatus::Succeeded) {return Ok((job,false));}
            }
        }
        if let Some(old)=prior_version {version=old.checked_add(1).ok_or_else(||error(Code::ResourceLimit))?;}
        let payload=serde_json::json!({"version":1,"profiles":[],"input":{"document_id":id,"document_version":version,"parser_identity":PARSER_ID}});
        let job=jobs_store::admit_job(tx,JobDraft{id:Uuid::new_v4(),kind:JobKind::Import,session_id:None,object_id:Some(id),admission:JobAdmission::Manual{exclusion_key:format!("document:{id}")},payload,initial_failure:None})?.view().clone();
        let original=super::super::store::encode_path(&copy.original)?;let managed=super::super::store::encode_path(&copy.managed)?;
        let known_kind=serde_json::to_value(copy.kind).map_err(|_|error(Code::Storage))?;
        if replacement.is_some() {
            tx.execute("UPDATE documents SET sha256=?1,title=?2,kind=?3,origin='native',version=?4,original_path=?5,managed_path=?6,status='parsing',error_code=NULL,updated_at_ms=?7,index_identity=NULL,parser_identity=?8,import_job_id=?9 WHERE id=?10",params![copy.sha256,copy.title,known_kind.as_str(),version,original,managed,job.created_at_ms,PARSER_ID,job.id.to_string(),id.to_string()])?;
            tx.execute("UPDATE document_chunks SET index_state='stale' WHERE document_id=?1",[id.to_string()])?;
        }else {
            tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at,kind,origin,version,original_path,managed_path,status,updated_at_ms,parser_identity,import_job_id) VALUES (?1,?2,?3,?4,?5,?6,'native',?7,?8,?9,'parsing',?5,?10,?11)",params![id.to_string(),copy.sha256,format!("managed:{id}"),copy.title,job.created_at_ms,known_kind.as_str(),version,original,managed,PARSER_ID,job.id.to_string()])?;
        }
        if tx.execute("UPDATE knowledge_managed_files SET document_id=?1,document_version=?2,job_id=?3 WHERE file_id=?4 AND state='copying'",params![id.to_string(),version,job.id.to_string(),file_id.to_string()])?!=1 {return Err(error(Code::Storage));}
        tx.execute("INSERT INTO knowledge_document_imports (document_id,document_version,job_id,parser_identity,sha256,file_id,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![id.to_string(),version,job.id.to_string(),PARSER_ID,copy.sha256,file_id.to_string(),job.created_at_ms])?;
        if *cancel.borrow(){return Err(error(Code::Cancelled));}
        Ok((job,true))
    })).map_err(safe)
}
