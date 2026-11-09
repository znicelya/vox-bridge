use super::super::{MAX_CHUNKS, PARSER_ID, chunk_blocks_bounded, chunk_hash, types::TextBlock};
use super::*;
use crate::intelligence::{
    jobs::{JobContext, JobEvents},
    store::job_in_transaction,
    types::JobStatus,
};
use rusqlite::params;
use serde::Deserialize;
pub(super) struct ImportHandler {
    pub store: Arc<LocalStore>,
    pub parser: Arc<dyn DocumentParser>,
    pub gate: Arc<Semaphore>,
    pub events: JobEvents,
    pub data_dir: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    document_id: Uuid,
    document_version: u32,
    parser_identity: String,
}
fn code(error: crate::error::AppError) -> Code {
    match error {
        crate::error::AppError::Intelligence(code) => code,
        _ => Code::Storage,
    }
}
impl ImportHandler {
    fn owns(
        &self,
        tx: &rusqlite::Transaction<'_>,
        context: &JobContext,
        input: &Input,
    ) -> AppResult<()> {
        let owns: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM documents d JOIN \
                    knowledge_document_imports i ON \
                    i.document_id=d.id AND \
                    i.document_version=d.version WHERE d.id=?1 \
                    AND d.version=?2 AND d.import_job_id=?3 AND \
                    d.parser_identity=?4 AND d.status='parsing' \
                    AND i.job_id=d.import_job_id AND \
                    i.parser_identity=d.parser_identity AND \
                    i.sha256=d.sha256)",
            params![
                input.document_id.to_string(),
                input.document_version,
                context.id.to_string(),
                PARSER_ID
            ],
            |r| r.get(0),
        )?;
        if owns {
            Ok(())
        } else {
            Err(error(Code::Cancelled))
        }
    }
    fn progress(&self, context: &JobContext, input: &Input, total: u32) -> AppResult<()> {
        let job = self
            .store
            .transaction(|tx| {
                context.commit(tx, || {
                    self.owns(tx, context, input)?;
                    tx.execute(
                        "UPDATE intelligence_jobs SET total_units=?1,\
                    updated_at_ms=?2 WHERE id=?3 AND \
                    status='running' AND completed_units=0",
                        params![
                            total,
                            crate::intelligence::jobs::now_ms(),
                            context.id.to_string()
                        ],
                    )?;
                    job_in_transaction(tx, context.id)?.ok_or_else(|| error(Code::Storage))
                })
            })
            .map_err(safe)?;
        self.events.publish(job);
        Ok(())
    }
    fn chunks(&self, context: &JobContext, input: &Input, chunks: Vec<TextBlock>) -> AppResult<()> {
        let job = self
            .store
            .transaction(|tx| {
                context.commit(tx, || {
                    self.owns(tx, context, input)?;
                    tx.execute(
                        "DELETE FROM document_chunks WHERE document_id=?1",
                        [input.document_id.to_string()],
                    )?;
                    let mut statement = tx.prepare(
                        "INSERT INTO document_chunks (id,document_id,\
                    chunk_index,text,location_json,content_hash,\
                    document_version,index_state) VALUES (?1,?2,\
                    ?3,?4,?5,?6,?7,'parsed')",
                    )?;
                    for (index, chunk) in chunks.iter().enumerate() {
                        statement.execute(params![
                            Uuid::new_v4().to_string(),
                            input.document_id.to_string(),
                            index as u32,
                            chunk.text,
                            serde_json::to_string(&chunk.location)
                                .map_err(|_| error(Code::Storage))?,
                            chunk_hash(&chunk.text),
                            input.document_version
                        ])?;
                    }
                    tx.execute(
                        "UPDATE documents SET status='parsed',\
                    error_code=NULL,index_identity=NULL,\
                    updated_at_ms=?1 WHERE id=?2",
                        params![
                            crate::intelligence::jobs::now_ms(),
                            input.document_id.to_string()
                        ],
                    )?;
                    tx.execute(
                        "DELETE FROM knowledge_repairs WHERE document_id=?1",
                        [input.document_id.to_string()],
                    )?;
                    tx.execute(
                        "UPDATE knowledge_managed_files SET \
                    state='retained' WHERE job_id=?1 AND \
                    state='copying'",
                        [context.id.to_string()],
                    )?;
                    tx.execute(
                        "UPDATE intelligence_jobs SET \
                    completed_units=?1,total_units=?1,\
                    updated_at_ms=?2 WHERE id=?3 AND \
                    status='running'",
                        params![
                            chunks.len() as u32,
                            crate::intelligence::jobs::now_ms(),
                            context.id.to_string()
                        ],
                    )?;
                    job_in_transaction(tx, context.id)?.ok_or_else(|| error(Code::Storage))
                })
            })
            .map_err(safe)?;
        self.events.publish(job);
        Ok(())
    }
}
#[async_trait::async_trait]
impl JobHandler for ImportHandler {
    async fn run(&self, mut context: JobContext, payload: serde_json::Value) -> Result<(), Code> {
        let envelope = crate::intelligence::store::decode_payload(
            &serde_json::to_string(&payload).map_err(|_| Code::ParseFailed)?,
        )
        .map_err(code)?;
        if !envelope.profiles.is_empty() {
            return Err(Code::Configuration);
        }
        let input: Input = serde_json::from_value(envelope.input).map_err(|_| Code::ParseFailed)?;
        if input.document_version == 0 || input.parser_identity != PARSER_ID {
            return Err(Code::ParseFailed);
        }
        let _permit = tokio::select! {
        biased;
                    _=crate::intelligence::http::cancelled(&mut context.cancel)=>return Err(Code::Cancelled),
                    permit=self.gate.clone().acquire_owned()=>permit.map_err(|_|Code::Interrupted)?,
                };
        self.store
            .transaction(|tx| context.commit(tx, || self.owns(tx, &context, &input)))
            .map_err(code)?;
        let detail = store::get(&self.store, input.document_id)
            .map_err(code)?
            .ok_or(Code::Storage)?;
        let path = detail.managed_path.ok_or(Code::UnsupportedDocument)?;
        let blocks = self
            .parser
            .parse(path, detail.summary.kind, context.cancel.clone())
            .await?;
        // Direct await retains the sole permit through actual reap, then result fencing.
        let chunks = chunk_blocks_bounded(&blocks).map_err(code)?;
        if chunks.is_empty() || chunks.len() > MAX_CHUNKS {
            return Err(Code::ResourceLimit);
        }
        self.progress(&context, &input, chunks.len() as u32)
            .map_err(code)?;
        self.chunks(&context, &input, chunks).map_err(code)
    }
    fn terminal_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        id: Uuid,
        status: JobStatus,
        code: Option<Code>,
        awaiting_reap: bool,
    ) -> AppResult<()> {
        if status == JobStatus::Succeeded {
            return Ok(());
        }
        tx.execute(
            "UPDATE documents SET status='failed',\
                    error_code=?1,managed_path=NULL,\
                    index_identity=NULL,updated_at_ms=?2 WHERE \
                    import_job_id=?3 AND status='parsing' AND \
                    EXISTS(SELECT 1 FROM \
                    knowledge_document_imports i WHERE \
                    i.document_id=documents.id AND \
                    i.document_version=documents.version AND \
                    i.job_id=?3)",
            params![
                code.map(Code::as_str),
                crate::intelligence::jobs::now_ms(),
                id.to_string()
            ],
        )?;
        tx.execute(
            "UPDATE knowledge_managed_files SET state=?1 \
                    WHERE job_id=?2 AND state='copying'",
            params![
                if awaiting_reap {
                    "awaiting_reap"
                } else {
                    "delete_pending"
                },
                id.to_string()
            ],
        )?;
        Ok(())
    }
    async fn cleanup(&self, reaped: Option<Uuid>) -> AppResult<()> {
        let store = self.store.clone();
        let data_dir = self.data_dir.clone();
        tokio::task::spawn_blocking(move || cleanup_rows(&store, &data_dir, reaped, false))
            .await
            .map_err(|_| error(Code::Interrupted))?
    }
    async fn cleanup_after_stop(&self) -> AppResult<()> {
        let store = self.store.clone();
        let data_dir = self.data_dir.clone();
        tokio::task::spawn_blocking(move || cleanup_rows(&store, &data_dir, None, true))
            .await
            .map_err(|_| error(Code::Interrupted))?
    }
    fn reconcile_startup(&self) -> AppResult<()> {
        self.store
            .transaction(|tx| {
                // Older terminal mutations may not have called the domain hook. Match
                // only the current version/job association; never replay its handler.
                let terminal = tx
                    .prepare(
                        "SELECT j.id,j.status,j.error_code FROM \
                    documents d JOIN knowledge_document_imports \
                    i ON i.document_id=d.id AND \
                    i.document_version=d.version AND \
                    i.job_id=d.import_job_id JOIN \
                    intelligence_jobs j ON j.id=i.job_id WHERE \
                    d.status='parsing' AND j.status IN ('failed',\
                    'cancelled','interrupted') LIMIT 33",
                    )?
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                if terminal.len() > 32 {
                    return Err(error(Code::ResourceLimit));
                }
                for (id, status, raw_code) in terminal {
                    let id = Uuid::parse_str(&id).map_err(|_| error(Code::Storage))?;
                    let status: JobStatus =
                        serde_json::from_value(serde_json::Value::String(status))
                            .map_err(|_| error(Code::Storage))?;
                    let code = raw_code
                        .map(|c| {
                            serde_json::from_value(serde_json::Value::String(c))
                                .map_err(|_| error(Code::Storage))
                        })
                        .transpose()?;
                    self.terminal_tx(tx, id, status, code, false)?;
                }
                tx.execute(
                    "UPDATE knowledge_managed_files SET \
                    state='delete_pending' WHERE state IN \
                    ('copying','awaiting_reap')",
                    [],
                )?;
                Ok(())
            })
            .map_err(safe)?;
        cleanup_rows(&self.store, &self.data_dir, None, true)
    }
}
pub(crate) fn cleanup_rows(
    store: &LocalStore,
    data_dir: &std::path::Path,
    reaped: Option<Uuid>,
    all_reaped: bool,
) -> AppResult<()> {
    store
        .transaction(|tx| {
            if all_reaped {
                tx.execute(
                    "UPDATE knowledge_managed_files SET \
                    state='delete_pending' WHERE \
                    state='awaiting_reap'",
                    [],
                )?;
            } else if let Some(id) = reaped {
                tx.execute(
                    "UPDATE knowledge_managed_files SET \
                    state='delete_pending' WHERE job_id=?1 AND \
                    state='awaiting_reap'",
                    [id.to_string()],
                )?;
            }
            Ok(())
        })
        .map_err(safe)?;
    // Each bounded page releases SQLite before filesystem I/O; no full history allocation.
    for _ in 0..1000 {
        crate::knowledge::index::delete_completion::guard_managed_cleanup(store, data_dir)?;
        let ids = store
            .transaction(|tx| {
                Ok(tx
                    .prepare(
                        "SELECT file_id FROM knowledge_managed_files \
                    WHERE state='delete_pending' ORDER BY \
                    created_at_ms,file_id LIMIT 200",
                    )?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?)
            })
            .map_err(safe)?;
        if ids.is_empty() {
            return Ok(());
        }
        for raw_id in ids {
            let id = Uuid::parse_str(&raw_id).map_err(|_| error(Code::Storage))?;
            if let Err(failure) = copying::cleanup_candidate(store, data_dir, id) {
                store
                    .transaction(|tx| {
                        tx.execute(
                            "UPDATE documents SET \
                    status='repair_required',\
                    error_code='storage',updated_at_ms=?1 WHERE \
                    status IN ('failed','repair_required') AND \
                    EXISTS(SELECT 1 FROM knowledge_managed_files \
                    f WHERE f.file_id=?2 AND \
                    f.document_id=documents.id AND \
                    f.document_version=documents.version AND \
                    f.job_id=documents.import_job_id)",
                            params![crate::intelligence::jobs::now_ms(), raw_id],
                        )?;
                        tx.execute(
                            "INSERT INTO knowledge_repairs (document_id,\
                    reason) SELECT d.id,\
                    'managed_copy_cleanup_failed' FROM documents \
                    d JOIN knowledge_managed_files f ON \
                    f.document_id=d.id AND \
                    f.document_version=d.version AND \
                    f.job_id=d.import_job_id WHERE f.file_id=?1 \
                    AND d.status='repair_required' ON \
                    CONFLICT(document_id) DO UPDATE SET \
                    reason=excluded.reason",
                            [raw_id],
                        )?;
                        Ok(())
                    })
                    .map_err(safe)?;
                return Err(failure);
            }
        }
    }
    Err(error(Code::ResourceLimit))
}
