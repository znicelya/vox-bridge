use super::*;
use crate::intelligence::jobs::{AfterTerminal, AfterTerminalNotice, JobAdmissionFence, JobEvents};
use tokio::sync::Notify;
pub(super) struct ImportContinuation {
    pub(super) config: Arc<ServiceConfigStore>,
    pub(super) events: JobEvents,
    pub(super) wake: Arc<Notify>,
}
fn matches(tx: &rusqlite::Transaction<'_>, notice: &AfterTerminalNotice) -> AppResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM documents d JOIN \
                    knowledge_document_imports i ON \
                    i.document_id=d.id AND \
                    i.document_version=d.version JOIN \
                    intelligence_jobs j ON j.id=i.job_id WHERE \
                    d.id=?1 AND d.version=?2 AND d.sha256=?3 AND \
                    d.parser_identity=?4 AND d.import_job_id=?5 \
                    AND d.status='parsed' AND \
                    i.job_id=d.import_job_id AND \
                    i.sha256=d.sha256 AND \
                    i.parser_identity=d.parser_identity AND \
                    j.status='succeeded')",
        params![
            notice.document_id.to_string(),
            notice.version,
            notice.sha256,
            notice.parser,
            notice.parent_id.to_string()
        ],
        |r| r.get(0),
    )?)
}
fn defer(tx: &rusqlite::Transaction<'_>, notice: &AfterTerminalNotice) -> AppResult<()> {
    if matches(tx, notice)? {
        tx.execute(
            "INSERT INTO knowledge_repairs(document_id,\
                    reason) VALUES (?1,'auto_index_deferred') ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
            [notice.document_id.to_string()],
        )?;
    }
    Ok(())
}
#[async_trait::async_trait]
impl AfterTerminal for ImportContinuation {
    fn notice(
        &self,
        tx: &rusqlite::Transaction<'_>,
        job: &JobView,
    ) -> AppResult<Option<AfterTerminalNotice>> {
        if job.kind != JobKind::Import {
            return Ok(None);
        }
        let mut q = tx.prepare(
            "SELECT d.id,d.version,d.sha256,\
                    d.parser_identity FROM documents d JOIN \
                    knowledge_document_imports i ON \
                    i.document_id=d.id AND \
                    i.document_version=d.version JOIN \
                    intelligence_jobs j ON j.id=i.job_id WHERE \
                    d.import_job_id=?1 AND \
                    i.job_id=d.import_job_id AND \
                    i.sha256=d.sha256 AND \
                    i.parser_identity=d.parser_identity AND \
                    d.status='parsed' AND j.status='running'",
        )?;
        let mut rows = q.query([job.id.to_string()])?;
        let Some(r) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(AfterTerminalNotice {
            parent_id: job.id,
            document_id: Uuid::parse_str(state::text(r, 0, 36)?)
                .map_err(|_| error(Code::Storage))?,
            version: r.get(1)?,
            sha256: state::text(r, 2, 64)?.into(),
            parser: state::text(r, 3, 128)?.into(),
        }))
    }
    fn deferred(
        &self,
        tx: &rusqlite::Transaction<'_>,
        notice: &AfterTerminalNotice,
    ) -> AppResult<()> {
        defer(tx, notice)
    }
    async fn succeeded(
        &self,
        notice: AfterTerminalNotice,
        owner: OperationLease,
        fence: JobAdmissionFence,
    ) {
        let events = self.events.clone();
        let wake = self.wake.clone();
        let _ = self
            .config
            .scoped(owner)
            .with_selected_embedding_completion(
                move |tx, profile| {
                    if !matches(tx, &notice)? {
                        return Ok(None);
                    }
                    let admitted=fence.run(|| {
                let Some(profile)=profile else{
defer(tx,
&notice)?;
return Ok(None);
}
;
                let job=admit_job(tx,
JobDraft{
id:Uuid::new_v4(),
kind:JobKind::Reindex,
session_id:None,
object_id:Some(notice.document_id),
admission:JobAdmission::Manual{
exclusion_key:format!("document:{}",
notice.document_id)}
,
payload:serde_json::json!({
"version":1,
"profiles":[profile],
"input":{
"document_id":notice.document_id,
"document_version":notice.version}
}
),
initial_failure:None}
)?.view().clone();
                tx.execute("UPDATE documents SET status='indexing' WHERE id=?1 AND status='parsed'",
[notice.document_id.to_string()])?;
                tx.execute("UPDATE document_chunks SET \
                    index_state='indexing' WHERE document_id=?1 \
                    AND document_version=?2",
params![notice.document_id.to_string(),
notice.version])?;
Ok(Some(job))
            }
);
                    match admitted {
                        Err(crate::error::AppError::Intelligence(Code::Busy)) => {
                            defer(tx, &notice)?;
                            Ok(None)
                        }
                        other => other,
                    }
                },
                move |job: &Option<JobView>| {
                    if let Some(job) = job {
                        events.publish(job.clone());
                        wake.notify_one();
                    }
                },
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence::test_support::{IntelligenceHarness, payload, test_llm_profile};
    struct Park(tokio::sync::Notify);
    #[async_trait::async_trait]
    impl JobHandler for Park {
        async fn run(&self, mut context: JobContext, _: serde_json::Value) -> Result<(), Code> {
            self.0.notify_one();
            crate::intelligence::http::cancelled(&mut context.cancel).await;
            Err(Code::Cancelled)
        }
    }
    fn draft(object: Uuid, kind: JobKind) -> JobDraft {
        JobDraft {
            id: Uuid::new_v4(),
            kind,
            session_id: None,
            object_id: Some(object),
            admission: JobAdmission::Manual {
                exclusion_key: format!("document:{object}"),
            },
            payload: payload(vec![]),
            initial_failure: None,
        }
    }
    async fn fixture() -> (
        IntelligenceHarness,
        ImportContinuation,
        AfterTerminalNotice,
        JobAdmissionFence,
    ) {
        let h = IntelligenceHarness::new();
        let profile = ModelProfile {
            kind: crate::intelligence::types::ServiceKind::Embedding,
            max_context_tokens: None,
            max_output_tokens: None,
            dimensions: Some(128),
            ..test_llm_profile()
        };
        h.config
            .save(crate::intelligence::types::SaveModelProfile {
                profile: profile.clone(),
                credential: crate::app::CredentialChange::Replace {
                    api_key: "synthetic".into(),
                },
            })
            .await
            .unwrap();
        let mut settings = h.config.settings().await.unwrap();
        settings.embedding_profile_id = Some(profile.id);
        h.config.save_settings(settings).await.unwrap();
        let park = Arc::new(Park(tokio::sync::Notify::new()));
        h.jobs.register(JobKind::Reindex, park.clone()).unwrap();
        h.jobs.start().unwrap();
        h.store
            .transaction(|tx| {
                admit_job(tx, draft(Uuid::new_v4(), JobKind::Reindex))?;
                Ok(())
            })
            .unwrap();
        h.jobs.wake();
        park.0.notified().await;
        let document = Uuid::new_v4();
        let parent = Uuid::new_v4();
        h.store
            .transaction(|tx| {
                let mut job = draft(document, JobKind::Import);
                job.id = parent;
                admit_job(tx, job)?;
                tx.execute(
                    "UPDATE intelligence_jobs SET status='succeeded' WHERE id=?1",
                    [parent.to_string()],
                )?;
                tx.execute(
                    "INSERT INTO documents(id,sha256,uri,title,\
                    created_at,origin,kind,status,\
                    parser_identity,import_job_id) VALUES (?1,?2,\
                    '','fixture',1,'native','text','parsed',\
                    'fixture',?3)",
                    params![document.to_string(), "a".repeat(64), parent.to_string()],
                )?;
                tx.execute(
                    "INSERT INTO \
                    knowledge_document_imports(document_id,\
                    document_version,job_id,sha256,\
                    parser_identity,created_at_ms) VALUES (?1,1,\
                    ?2,?3,'fixture',1)",
                    params![document.to_string(), parent.to_string(), "a".repeat(64)],
                )?;
                Ok(())
            })
            .unwrap();
        let hook = ImportContinuation {
            config: h.config.clone(),
            events: h.jobs.event_port(),
            wake: h.jobs.wake_port(),
        };
        let notice = AfterTerminalNotice {
            parent_id: parent,
            document_id: document,
            version: 1,
            sha256: "a".repeat(64),
            parser: "fixture".into(),
        };
        let fence = h.jobs.admission_fence(JobKind::Reindex).unwrap();
        (h, hook, notice, fence)
    }
    #[tokio::test]
    async fn knowledge_import_continuation_capacity_exclusion_missing_profile_shutdown_defer_parsed()
     {
        for condition in ["capacity", "exclusion", "profile", "shutdown"] {
            let (h, hook, notice, fence) = fixture().await;
            match condition {
                "capacity" => h
                    .store
                    .transaction(|tx| {
                        for _ in 0..31 {
                            admit_job(tx, draft(Uuid::new_v4(), JobKind::Reindex))?;
                        }
                        Ok(())
                    })
                    .unwrap(),
                "exclusion" => h
                    .store
                    .transaction(|tx| {
                        admit_job(tx, draft(notice.document_id, JobKind::Reindex))?;
                        Ok(())
                    })
                    .unwrap(),
                "profile" => {
                    let mut settings = h.config.settings().await.unwrap();
                    settings.embedding_profile_id = None;
                    h.config.save_settings(settings).await.unwrap();
                }
                "shutdown" => h.jobs.shutdown().await.unwrap(),
                _ => unreachable!(),
            }
            let before: u32 = h
                .store
                .transaction(|tx| {
                    Ok(tx.query_row(
                        "SELECT count(*) FROM intelligence_jobs WHERE kind='reindex'",
                        [],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            hook.succeeded(
                AfterTerminalNotice {
                    parent_id: notice.parent_id,
                    document_id: notice.document_id,
                    version: notice.version,
                    sha256: notice.sha256.clone(),
                    parser: notice.parser.clone(),
                },
                Arc::new(()),
                fence,
            )
            .await;
            let (status, reason, after, parent): (String, String, u32, String) = h
                .store
                .transaction(|tx| {
                    Ok((
                        tx.query_row(
                            "SELECT status FROM documents WHERE id=?1",
                            [notice.document_id.to_string()],
                            |r| r.get(0),
                        )?,
                        tx.query_row(
                            "SELECT reason FROM knowledge_repairs WHERE document_id=?1",
                            [notice.document_id.to_string()],
                            |r| r.get(0),
                        )?,
                        tx.query_row(
                            "SELECT count(*) FROM intelligence_jobs WHERE kind='reindex'",
                            [],
                            |r| r.get(0),
                        )?,
                        tx.query_row(
                            "SELECT status FROM intelligence_jobs WHERE id=?1",
                            [notice.parent_id.to_string()],
                            |r| r.get(0),
                        )?,
                    ))
                })
                .unwrap();
            assert_eq!(
                (status.as_str(), reason.as_str(), after, parent.as_str()),
                ("parsed", "auto_index_deferred", before, "succeeded"),
                "{condition}"
            );
            assert!(h.http.requests.lock().unwrap().is_empty());
            h.jobs.shutdown().await.unwrap();
        }
    }
    #[tokio::test]
    async fn knowledge_import_continuation_stale_or_deleted_notice_never_changes_new_association() {
        for mutation in [
            "version", "sha", "parser", "import", "status", "deleted", "parent",
        ] {
            let (h, hook, notice, fence) = fixture().await;
            h.store
                .transaction(|tx| {
                    match mutation {
                        "version" => {
                            tx.execute(
                                "UPDATE documents SET version=2 WHERE id=?1",
                                [notice.document_id.to_string()],
                            )?;
                        }
                        "sha" => {
                            tx.execute(
                                "UPDATE documents SET sha256=?1 WHERE id=?2",
                                params!["b".repeat(64), notice.document_id.to_string()],
                            )?;
                        }
                        "parser" => {
                            tx.execute(
                                "UPDATE documents SET parser_identity='new-parser' WHERE id=?1",
                                [notice.document_id.to_string()],
                            )?;
                        }
                        "import" => {
                            tx.execute(
                                "UPDATE documents SET import_job_id=NULL WHERE id=?1",
                                [notice.document_id.to_string()],
                            )?;
                        }
                        "status" => {
                            tx.execute(
                                "UPDATE documents SET status='deleting' WHERE id=?1",
                                [notice.document_id.to_string()],
                            )?;
                        }
                        "deleted" => {
                            tx.execute(
                                "DELETE FROM documents WHERE id=?1",
                                [notice.document_id.to_string()],
                            )?;
                        }
                        "parent" => {
                            tx.execute(
                                "UPDATE intelligence_jobs SET status='cancelled' WHERE id=?1",
                                [notice.parent_id.to_string()],
                            )?;
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                })
                .unwrap();
            hook.succeeded(
                AfterTerminalNotice {
                    parent_id: notice.parent_id,
                    document_id: notice.document_id,
                    version: notice.version,
                    sha256: notice.sha256.clone(),
                    parser: notice.parser.clone(),
                },
                Arc::new(()),
                fence,
            )
            .await;
            let (jobs, repairs): (u32, u32) = h
                .store
                .transaction(|tx| {
                    Ok((
                        tx.query_row(
                            "SELECT count(*) FROM intelligence_jobs WHERE \
                    kind='reindex' AND object_id=?1",
                            [notice.document_id.to_string()],
                            |r| r.get(0),
                        )?,
                        tx.query_row(
                            "SELECT count(*) FROM knowledge_repairs WHERE document_id=?1",
                            [notice.document_id.to_string()],
                            |r| r.get(0),
                        )?,
                    ))
                })
                .unwrap();
            assert_eq!((jobs, repairs), (0, 0), "{mutation}");
            assert!(h.http.requests.lock().unwrap().is_empty());
            h.jobs.shutdown().await.unwrap();
        }
    }
}
