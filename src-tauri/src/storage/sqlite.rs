use super::models::{
    ExportData, SessionSummary, StoredSession, StoredSessionLifecycle, StoredTranscriptSegment,
    SummaryVersion,
};
use crate::{
    domain::{
        launch::SessionContext,
        session::SessionDirection,
        transcript::{
            DiagnosticPage, DiagnosticPayload, RuntimeDiagnostic, RuntimeFailureCode,
            TranscriptPage, TranscriptUpdate,
        },
    },
    error::{AppError, AppResult},
};
use rusqlite::{Connection, OptionalExtension, params};
use std::{path::Path, sync::Mutex};
use uuid::Uuid;

#[derive(Debug)]
pub struct LocalStore {
    connection: Mutex<Connection>,
    summary_snapshot_scopes: std::sync::Arc<tokio::sync::Semaphore>,
}
pub(crate) struct SummarySnapshotReadScope {
    deadline: std::time::Instant,
    cancel: tokio::sync::watch::Receiver<bool>,
}
impl SummarySnapshotReadScope {
    pub(crate) fn check(&self) -> AppResult<()> {
        use crate::intelligence::types::IntelligenceErrorCode as Code;
        if *self.cancel.borrow() || self.cancel.has_changed().is_err() {
            return Err(AppError::Intelligence(Code::Cancelled));
        }
        if std::time::Instant::now() >= self.deadline {
            return Err(AppError::Intelligence(Code::Timeout));
        }
        Ok(())
    }
}
impl LocalStore {
    /// A private borrowed Deferred read. The blocking closure owns both its
    /// process/job lease and its slot until SQLite actually releases them.
    pub(crate) async fn with_summary_snapshot_owned<T: Send + 'static>(
        self: &std::sync::Arc<Self>,
        owner: crate::intelligence::jobs::OperationLease,
        mut cancel: tokio::sync::watch::Receiver<bool>,
        read: impl FnOnce(&rusqlite::Transaction<'_>, &SummarySnapshotReadScope) -> AppResult<T>
        + Send
        + 'static,
    ) -> AppResult<T> {
        use crate::intelligence::{http::cancelled, types::IntelligenceErrorCode as Code};
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let observed = tokio::time::Instant::from_std(deadline);
        let permit = tokio::select! {biased;
            _=cancelled(&mut cancel)=>return Err(AppError::Intelligence(Code::Cancelled)),
            p=tokio::time::timeout_at(observed,self.summary_snapshot_scopes.clone().acquire_owned())=>p.map_err(|_|AppError::Intelligence(Code::Timeout))?.map_err(|_|AppError::Intelligence(Code::Interrupted))?,
        };
        let store = self.clone();
        let scan_cancel = cancel.clone();
        let work = tokio::task::spawn_blocking(move || {
            let (_owner, _permit) = (owner, permit);
            let scope = SummarySnapshotReadScope {
                deadline,
                cancel: scan_cancel,
            };
            let mut connection = loop {
                scope.check()?;
                match store.connection.try_lock() {
                    Ok(connection) => break connection,
                    Err(std::sync::TryLockError::WouldBlock) => {
                        std::thread::sleep(std::time::Duration::from_millis(1))
                    }
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        return Err(AppError::Intelligence(Code::Storage));
                    }
                }
            };
            scope.check()?;
            let tx =
                connection.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
            let value = read(&tx, &scope)?;
            scope.check()?;
            tx.commit()?;
            Ok(value)
        });
        tokio::select! {biased;
            _=cancelled(&mut cancel)=>Err(AppError::Intelligence(Code::Cancelled)),
            result=tokio::time::timeout_at(observed,work)=>result.map_err(|_|AppError::Intelligence(Code::Timeout))?.map_err(|_|AppError::Intelligence(Code::Interrupted))?.map_err(crate::intelligence::store::safe_store_error),
        }
    }
    #[cfg(test)]
    pub(crate) fn fail_lifecycle_status_for_tests(&self, status: &str) -> AppResult<()> {
        if !matches!(
            status,
            "running" | "failed" | "starting" | "stopping" | "completed"
        ) {
            return Err(crate::error::AppError::InvalidRequest(
                "unsupported test lifecycle status".into(),
            ));
        }
        self.connection.lock().unwrap().execute_batch(&format!(
            "CREATE TRIGGER fail_{status} BEFORE UPDATE \
                    ON session_lifecycle WHEN NEW.status = \
                    '{status}' BEGIN SELECT RAISE(ABORT, \
                    'injected lifecycle failure'); END;"
        ))?;
        Ok(())
    }
    pub fn open(data_dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        Self::initialize(Connection::open(data_dir.join("vox-bridge.sqlite"))?)
    }
    pub fn in_memory() -> AppResult<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }
    fn initialize(mut conn: Connection) -> AppResult<Self> {
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        const MIGRATIONS: &[&str] = &[
            include_str!("../../migrations/0001_session_foundation.sql"),
            include_str!("../../migrations/0002_runtime.sql"),
            include_str!("../../migrations/0003_intelligence.sql"),
            include_str!("../../migrations/0004_knowledge.sql"),
            include_str!("../../migrations/0005_knowledge_index.sql"),
            include_str!("../../migrations/0006_summaries.sql"),
        ];
        if version > MIGRATIONS.len() as i64 {
            return Err(AppError::InvalidRequest(
                "database schema is newer than this application".into(),
            ));
        }
        conn.pragma_update(None, "foreign_keys", true)?;
        if version < MIGRATIONS.len() as i64 {
            let tx = conn.transaction()?;
            for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
                tx.execute_batch(migration)?;
                tx.pragma_update(None, "user_version", index + 1)?;
            }
            tx.commit()?;
        }
        Ok(Self {
            connection: Mutex::new(conn),
            summary_snapshot_scopes: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        })
    }
    /// The existing SQLite owner is the only authority. The closure must never
    /// call ConfigIo: cross-store operations acquire config before SQLite.
    pub(crate) fn transaction<T>(
        &self,
        operation: impl FnOnce(&rusqlite::Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| AppError::Internal("database lock unavailable".into()))?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let value = operation(&tx)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn create_session(&self, s: &StoredSession) -> AppResult<()> {
        let mut c = self.connection.lock().unwrap();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO sessions (id,started_at,mode,\
                    direction,source_language,target_language) \
                    VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                s.id.to_string(),
                s.started_at,
                s.mode,
                s.direction,
                s.source_language,
                s.target_language
            ],
        )?;
        tx.execute(
            "INSERT INTO session_lifecycle (session_id,\
                    status,ended_at) VALUES (?1,'created',NULL)",
            params![s.id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Admission is atomic: no runtime may open until this transaction commits.
    pub fn create_runtime_session(
        &self,
        session: &StoredSession,
        context: &SessionContext,
    ) -> AppResult<()> {
        let context_json = json(context)?;
        let mut connection = self.connection.lock().unwrap();
        let tx = connection.transaction()?;
        tx.execute(
            "INSERT INTO sessions (id,started_at,mode,\
                    direction,source_language,target_language) \
                    VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                session.id.to_string(),
                session.started_at,
                session.mode,
                session.direction,
                session.source_language,
                session.target_language
            ],
        )?;
        tx.execute(
            "INSERT INTO session_lifecycle (session_id,\
                    status,ended_at) VALUES (?1,'starting',NULL)",
            params![session.id.to_string()],
        )?;
        tx.execute(
            "INSERT INTO session_runtime_context \
                    (session_id,context_json) VALUES (?1,?2)",
            params![session.id.to_string(), context_json],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn session_context(&self, session_id: Uuid) -> AppResult<Option<SessionContext>> {
        let encoded: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT context_json FROM session_runtime_context WHERE session_id=?1",
                params![session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        encoded
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|_| AppError::Internal("cannot decode session context".into()))
            })
            .transpose()
    }

    /// Caller must hold the exclusive data-directory process owner before recovery.
    /// The transaction retains transcript/context and never reopens runtime resources.
    pub fn interrupt_incomplete_sessions(&self, ended_at: i64) -> AppResult<Vec<Uuid>> {
        let occurred_at_ms = ended_at
            .checked_mul(1000)
            .ok_or_else(|| AppError::InvalidRequest("recovery timestamp out of range".into()))?;
        let mut connection = self.connection.lock().unwrap();
        let tx = connection.transaction()?;
        let ids = {
            let mut query = tx.prepare(
                "SELECT session_id FROM session_lifecycle \
                    WHERE status IN ('created','starting',\
                    'running','stopping','paused') ORDER BY \
                    session_id",
            )?;
            let rows = query.query_map([], |row| row.get::<_, String>(0))?;
            rows.map(|row| {
                Uuid::parse_str(&row?)
                    .map_err(|_| AppError::Internal("invalid stored session ID".into()))
            })
            .collect::<AppResult<Vec<_>>>()?
        };
        for id in &ids {
            tx.execute(
                "UPDATE session_lifecycle SET status='failed',\
                    ended_at=?2 WHERE session_id=?1",
                params![id.to_string(), ended_at],
            )?;
            let diagnostic = RuntimeDiagnostic {
                id: Uuid::new_v4(),
                session_id: *id,
                direction: None,
                stream_epoch: None,
                occurred_at_ms,
                payload: DiagnosticPayload::Failure {
                    code: RuntimeFailureCode::Interrupted,
                    provider_code: None,
                    log_id: None,
                },
            };
            tx.execute(
                "INSERT INTO session_diagnostics (id,\
                    session_id,occurred_at_ms,payload_json) \
                    VALUES (?1,?2,?3,?4)",
                params![
                    diagnostic.id.to_string(),
                    id.to_string(),
                    occurred_at_ms,
                    json(&diagnostic)?
                ],
            )?;
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn update_session_lifecycle(&self, lifecycle: &StoredSessionLifecycle) -> AppResult<()> {
        let c = self.connection.lock().unwrap();
        let changed = c.execute(
            "UPDATE session_lifecycle SET status=?2,ended_at=?3 WHERE session_id=?1",
            params![
                lifecycle.session_id.to_string(),
                lifecycle.status,
                lifecycle.ended_at
            ],
        )?;
        if changed == 0 {
            return Err(crate::error::AppError::SessionNotFound(
                lifecycle.session_id,
            ));
        }
        Ok(())
    }

    pub fn get_session_lifecycle(&self, id: Uuid) -> AppResult<Option<StoredSessionLifecycle>> {
        let c = self.connection.lock().unwrap();
        Ok(c.query_row(
            "SELECT session_id,status,ended_at FROM \
                    session_lifecycle WHERE session_id=?1",
            params![id.to_string()],
            |r| {
                Ok(StoredSessionLifecycle {
                    session_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                    status: r.get(1)?,
                    ended_at: r.get(2)?,
                })
            },
        )
        .optional()?)
    }
    /// Legacy writer: typed provider events must use append_transcript.
    pub fn append_final_segment(&self, s: &StoredTranscriptSegment) -> AppResult<()> {
        if !s.is_final || s.details.is_some() || s.occurred_at_ms.is_some() {
            return Err(crate::error::AppError::InvalidRequest(
                "legacy writer requires final segments without runtime metadata".into(),
            ));
        }
        let c = self.connection.lock().unwrap();
        c.execute(
            "INSERT INTO translation_segments (id,\
                    session_id,direction,sequence,occurred_at,\
                    language,text,is_final) VALUES (?1,?2,?3,?4,\
                    ?5,?6,?7,1)",
            params![
                s.id.to_string(),
                s.session_id.to_string(),
                s.direction,
                s.sequence,
                s.occurred_at,
                s.language,
                s.text
            ],
        )?;
        Ok(())
    }
    pub fn append_transcript(&self, update: &TranscriptUpdate) -> AppResult<()> {
        update.validate()?;
        if !update.is_final {
            return Err(AppError::InvalidRequest(
                "only final transcript events may be persisted".into(),
            ));
        }
        let details = json(update.details.as_ref().expect("validated details"))?;
        let sequence = i64::try_from(update.sequence)
            .map_err(|_| crate::domain::transcript::invalid_transcript())?;
        self.connection.lock().unwrap().execute(
            "INSERT INTO translation_segments (id,\
                    session_id,direction,sequence,occurred_at,\
                    language,text,is_final,details_json,\
                    occurred_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,\
                    1,?8,?9)",
            params![
                update.id.to_string(),
                update.session_id.to_string(),
                direction_name(update.direction),
                sequence,
                update.occurred_at_ms.div_euclid(1000),
                update.language,
                update.text,
                details,
                update.occurred_at_ms
            ],
        )?;
        Ok(())
    }
    pub fn transcript_page(
        &self,
        session_id: Uuid,
        after: Option<i64>,
        limit: u32,
    ) -> AppResult<TranscriptPage> {
        validate_page(after, limit)?;
        let c = self.connection.lock().unwrap();
        let mut query = c.prepare(
            "SELECT rowid,id,session_id,direction,\
                    sequence,occurred_at,language,text,is_final,\
                    details_json,occurred_at_ms FROM \
                    translation_segments WHERE session_id=?1 AND \
                    rowid>?2 ORDER BY rowid LIMIT ?3",
        )?;
        let mut rows = query.query(params![
            session_id.to_string(),
            after.unwrap_or(0),
            limit + 1
        ])?;
        let mut found = Vec::new();
        while let Some(row) = rows.next()? {
            let rowid: i64 = row.get(0)?;
            let segment = read_segment_at(row, 1)?;
            let occurred_at_ms = segment.occurred_at_ms.map(Ok).unwrap_or_else(|| {
                segment.occurred_at.checked_mul(1000).ok_or_else(|| {
                    AppError::Internal("legacy transcript timestamp out of range".into())
                })
            })?;
            let update = TranscriptUpdate {
                id: segment.id,
                session_id: segment.session_id,
                direction: parse_direction(&segment.direction)?,
                sequence: u64::try_from(segment.sequence).map_err(|_| {
                    AppError::Internal("legacy transcript sequence out of range".into())
                })?,
                occurred_at_ms,
                language: segment.language,
                text: segment.text,
                is_final: segment.is_final,
                details: segment.details,
            };
            found.push((rowid, update));
        }
        let next_cursor = page_cursor(&mut found, limit);
        Ok(TranscriptPage {
            rows: found.into_iter().map(|(_, row)| row).collect(),
            next_cursor,
        })
    }
    pub fn next_transcript_sequence(
        &self,
        session_id: Uuid,
        direction: SessionDirection,
    ) -> AppResult<u64> {
        if direction == SessionDirection::Bidirectional {
            return Err(crate::domain::transcript::invalid_transcript());
        }
        let max: Option<i64> = self.connection.lock().unwrap().query_row(
            "SELECT MAX(sequence) FROM \
                    translation_segments WHERE session_id=?1 AND \
                    direction=?2",
            params![session_id.to_string(), direction_name(direction)],
            |r| r.get(0),
        )?;
        max.map(|value| {
            value
                .checked_add(1)
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(crate::domain::transcript::invalid_transcript)
        })
        .unwrap_or(Ok(0))
    }
    pub fn append_diagnostic(&self, diagnostic: &RuntimeDiagnostic) -> AppResult<()> {
        diagnostic.validate()?;
        self.connection.lock().unwrap().execute(
            "INSERT INTO session_diagnostics (id,\
                    session_id,occurred_at_ms,payload_json) \
                    VALUES (?1,?2,?3,?4)",
            params![
                diagnostic.id.to_string(),
                diagnostic.session_id.to_string(),
                diagnostic.occurred_at_ms,
                json(diagnostic)?
            ],
        )?;
        Ok(())
    }
    pub fn diagnostic_page(
        &self,
        session_id: Uuid,
        after: Option<i64>,
        limit: u32,
    ) -> AppResult<DiagnosticPage> {
        validate_page(after, limit)?;
        let c = self.connection.lock().unwrap();
        let mut query = c.prepare(
            "SELECT rowid,payload_json FROM \
                    session_diagnostics WHERE session_id=?1 AND \
                    rowid>?2 ORDER BY rowid LIMIT ?3",
        )?;
        let mut found = query
            .query_map(
                params![session_id.to_string(), after.unwrap_or(0), limit + 1],
                |r| Ok((r.get::<_, i64>(0)?, read_json::<RuntimeDiagnostic>(r, 1)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = page_cursor(&mut found, limit);
        Ok(DiagnosticPage {
            rows: found.into_iter().map(|(_, row)| row).collect(),
            next_cursor,
        })
    }
    pub fn list_sessions(
        &self,
        limit: u32,
        cursor: Option<String>,
    ) -> AppResult<Vec<SessionSummary>> {
        let p = cursor.map(|v| parse_cursor(&v)).transpose()?;
        let (st, id) = p.map_or((None, None), |(s, i)| (Some(s), Some(i)));
        let c = self.connection.lock().unwrap();
        let mut q = c.prepare(
            "SELECT id,started_at,mode,direction,\
                    source_language,target_language FROM \
                    sessions WHERE (?1 IS NULL OR started_at < \
                    ?1 OR (started_at = ?1 AND id < ?2)) ORDER \
                    BY started_at DESC,id DESC LIMIT ?3",
        )?;
        let rows = q.query_map(params![st, id, limit], |r| {
            Ok(SessionSummary {
                id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                started_at: r.get(1)?,
                mode: r.get(2)?,
                direction: r.get(3)?,
                source_language: r.get(4)?,
                target_language: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn delete_session(&self, id: Uuid) -> AppResult<()> {
        let c = self.connection.lock().unwrap();
        let tx = c.unchecked_transaction()?;
        tx.execute("DELETE FROM sessions WHERE id=?1", params![id.to_string()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn get_session(&self, id: Uuid) -> AppResult<Option<StoredSession>> {
        let c = self.connection.lock().unwrap();
        Ok(c.query_row(
            "SELECT id,started_at,mode,direction,\
                    source_language,target_language FROM \
                    sessions WHERE id=?1",
            params![id.to_string()],
            |r| {
                Ok(StoredSession {
                    id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                    started_at: r.get(1)?,
                    mode: r.get(2)?,
                    direction: r.get(3)?,
                    source_language: r.get(4)?,
                    target_language: r.get(5)?,
                })
            },
        )
        .optional()?)
    }
    pub fn count_segments(&self, id: Uuid) -> AppResult<u32> {
        let c = self.connection.lock().unwrap();
        Ok(c.query_row(
            "SELECT COUNT(*) FROM translation_segments WHERE session_id=?1",
            params![id.to_string()],
            |r| r.get(0),
        )?)
    }
    pub fn count_summaries(&self, id: Uuid) -> AppResult<u32> {
        let c = self.connection.lock().unwrap();
        Ok(c.query_row(
            "SELECT COUNT(*) FROM session_summaries WHERE session_id=?1",
            params![id.to_string()],
            |r| r.get(0),
        )?)
    }
    pub fn create_summary_version(&self, s: &SummaryVersion) -> AppResult<()> {
        let mut c = self.connection.lock().unwrap();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO session_summaries (id,session_id,\
                    version,created_at,status,structured_json) \
                    VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                s.id.to_string(),
                s.session_id.to_string(),
                s.version,
                s.created_at,
                s.status,
                s.structured_json
            ],
        )?;
        // Foundation writes remain Legacy. Advancing the counter in this same
        // transaction prevents deletion of a high signed version enabling reuse.
        tx.execute(
            "INSERT INTO summary_version_counters(session_id,last_version) VALUES(?1,?2) \
             ON CONFLICT(session_id) DO UPDATE SET last_version=\
             max(summary_version_counters.last_version,excluded.last_version)",
            params![s.session_id.to_string(), s.version.max(0)],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn list_summary_versions(&self, id: Uuid) -> AppResult<Vec<SummaryVersion>> {
        let c = self.connection.lock().unwrap();
        let mut q = c.prepare(
            "SELECT id,session_id,version,created_at,\
                    status,structured_json FROM \
                    session_summaries WHERE session_id=?1 ORDER \
                    BY version",
        )?;
        let rows = q.query_map(params![id.to_string()], |r| {
            Ok(SummaryVersion {
                id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap(),
                session_id: Uuid::parse_str(&r.get::<_, String>(1)?).unwrap(),
                version: r.get(2)?,
                created_at: r.get(3)?,
                status: r.get(4)?,
                structured_json: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_export_data(&self, id: Uuid) -> AppResult<Option<ExportData>> {
        let Some(session) = self.get_session(id)? else {
            return Ok(None);
        };
        let lifecycle = self.get_session_lifecycle(id)?.ok_or_else(|| {
            crate::error::AppError::Internal("session lifecycle is missing".into())
        })?;
        let c = self.connection.lock().unwrap();
        let mut q = c.prepare(
            "SELECT id,session_id,direction,sequence,\
                    occurred_at,language,text,is_final,\
                    details_json,occurred_at_ms
             \
                    FROM translation_segments WHERE \
                    session_id=?1 AND is_final=1
             \
                    ORDER BY COALESCE(occurred_at_ms, \
                    occurred_at * 1000), sequence, id",
        )?;
        let rows = q.query_map(params![id.to_string()], read_segment)?;
        let segments = rows.collect::<Result<Vec<_>, _>>()?;
        drop(q);
        let mut q = c.prepare(
            "SELECT payload_json FROM session_diagnostics \
                    WHERE session_id=?1 ORDER BY rowid",
        )?;
        let diagnostics = q
            .query_map([id.to_string()], |r| read_json(r, 0))?
            .collect::<Result<Vec<RuntimeDiagnostic>, _>>()?;
        let context = c
            .query_row(
                "SELECT context_json FROM session_runtime_context WHERE session_id=?1",
                [id.to_string()],
                |r| read_json(r, 0),
            )
            .optional()?;
        drop(q);
        drop(c);
        Ok(Some(ExportData {
            session,
            lifecycle,
            segments,
            summaries: self.summary_export_records(id)?,
            diagnostics,
            context,
        }))
    }
}
fn parse_cursor(v: &str) -> AppResult<(i64, String)> {
    let (s, id) = v
        .rsplit_once(':')
        .ok_or_else(|| crate::error::AppError::InvalidRequest("invalid session cursor".into()))?;
    let s = s
        .parse()
        .map_err(|_| crate::error::AppError::InvalidRequest("invalid session cursor".into()))?;
    Uuid::parse_str(id)
        .map_err(|_| crate::error::AppError::InvalidRequest("invalid session cursor".into()))?;
    Ok((s, id.into()))
}

fn read_segment(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTranscriptSegment> {
    read_segment_at(row, 0)
}
fn read_segment_at(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<StoredTranscriptSegment> {
    let details: Option<String> = row.get(offset + 8)?;
    Ok(StoredTranscriptSegment {
        id: read_uuid(row, offset)?,
        session_id: read_uuid(row, offset + 1)?,
        direction: row.get(offset + 2)?,
        sequence: row.get(offset + 3)?,
        occurred_at: row.get(offset + 4)?,
        language: row.get(offset + 5)?,
        text: row.get(offset + 6)?,
        is_final: row.get::<_, i64>(offset + 7)? == 1,
        details: details
            .map(|value| serde_json::from_str(&value).map_err(|e| conversion_error(offset + 8, e)))
            .transpose()?,
        occurred_at_ms: row.get(offset + 9)?,
    })
}
fn read_uuid(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(index)?).map_err(|e| conversion_error(index, e))
}
fn read_json<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    serde_json::from_str(&row.get::<_, String>(index)?).map_err(|e| conversion_error(index, e))
}
fn conversion_error(
    index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
}
fn json(value: &impl serde::Serialize) -> AppResult<String> {
    serde_json::to_string(value)
        .map_err(|_| AppError::Internal("cannot encode runtime data".into()))
}
fn direction_name(direction: SessionDirection) -> &'static str {
    match direction {
        SessionDirection::SourceToTarget => "source_to_target",
        SessionDirection::TargetToSource => "target_to_source",
        SessionDirection::Bidirectional => "bidirectional",
    }
}
fn parse_direction(value: &str) -> AppResult<SessionDirection> {
    match value {
        "source_to_target" => Ok(SessionDirection::SourceToTarget),
        "target_to_source" => Ok(SessionDirection::TargetToSource),
        "bidirectional" => Ok(SessionDirection::Bidirectional),
        _ => Err(AppError::Internal(
            "unknown stored transcript direction".into(),
        )),
    }
}
fn validate_page(after: Option<i64>, limit: u32) -> AppResult<()> {
    if !(1..=200).contains(&limit) || after.is_some_and(|cursor| cursor < 0) {
        return Err(AppError::InvalidRequest(
            "page size must be 1–200 and cursor nonnegative".into(),
        ));
    }
    Ok(())
}
fn page_cursor<T>(rows: &mut Vec<(i64, T)>, limit: u32) -> Option<i64> {
    if rows.len() > limit as usize {
        rows.pop();
        rows.last().map(|(rowid, _)| *rowid)
    } else {
        None
    }
}
