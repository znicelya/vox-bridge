use super::models::{
    SessionSummary, StoredSession, StoredSessionLifecycle, StoredTranscriptSegment, SummaryVersion,
};
use crate::error::AppResult;
use rusqlite::{Connection, OptionalExtension, params};
use std::{path::Path, sync::Mutex};
use uuid::Uuid;

#[derive(Debug)]
pub struct LocalStore {
    connection: Mutex<Connection>,
}
impl LocalStore {
    pub fn open(data_dir: &Path) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        Self::initialize(Connection::open(data_dir.join("vox-bridge.sqlite"))?)
    }
    pub fn in_memory() -> AppResult<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }
    fn initialize(conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))?;
        Ok(Self {
            connection: Mutex::new(conn),
        })
    }
    pub fn create_session(&self, s: &StoredSession) -> AppResult<()> {
        let c = self.connection.lock().unwrap();
        c.execute(
            "INSERT INTO sessions VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                s.id.to_string(),
                s.started_at,
                s.mode,
                s.direction,
                s.source_language,
                s.target_language
            ],
        )?;
        c.execute(
            "INSERT INTO session_lifecycle (session_id,status,ended_at) VALUES (?1,'created',NULL)",
            params![s.id.to_string()],
        )?;
        Ok(())
    }

    pub fn update_session_lifecycle(&self, lifecycle: &StoredSessionLifecycle) -> AppResult<()> {
        let c = self.connection.lock().unwrap();
        c.execute(
            "UPDATE session_lifecycle SET status=?2,ended_at=?3 WHERE session_id=?1",
            params![
                lifecycle.session_id.to_string(),
                lifecycle.status,
                lifecycle.ended_at
            ],
        )?;
        Ok(())
    }

    pub fn get_session_lifecycle(&self, id: Uuid) -> AppResult<Option<StoredSessionLifecycle>> {
        let c = self.connection.lock().unwrap();
        Ok(c.query_row(
            "SELECT session_id,status,ended_at FROM session_lifecycle WHERE session_id=?1",
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
    pub fn append_final_segment(&self, s: &StoredTranscriptSegment) -> AppResult<()> {
        if !s.is_final {
            return Err(crate::error::AppError::InvalidRequest(
                "only final transcript segments may be persisted".into(),
            ));
        }
        let c = self.connection.lock().unwrap();
        c.execute(
            "INSERT INTO translation_segments VALUES (?1,?2,?3,?4,?5,?6,?7,1)",
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
    pub fn list_sessions(
        &self,
        limit: u32,
        cursor: Option<String>,
    ) -> AppResult<Vec<SessionSummary>> {
        let p = cursor.map(|v| parse_cursor(&v)).transpose()?;
        let (st, id) = p.map_or((None, None), |(s, i)| (Some(s), Some(i)));
        let c = self.connection.lock().unwrap();
        let mut q=c.prepare("SELECT id,started_at,mode,direction,source_language,target_language FROM sessions WHERE (?1 IS NULL OR started_at < ?1 OR (started_at = ?1 AND id < ?2)) ORDER BY started_at DESC,id DESC LIMIT ?3")?;
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
            "SELECT * FROM sessions WHERE id=?1",
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
            "INSERT INTO session_summaries VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                s.id.to_string(),
                s.session_id.to_string(),
                s.version,
                s.created_at,
                s.status,
                s.structured_json
            ],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn list_summary_versions(&self, id: Uuid) -> AppResult<Vec<SummaryVersion>> {
        let c = self.connection.lock().unwrap();
        let mut q =
            c.prepare("SELECT * FROM session_summaries WHERE session_id=?1 ORDER BY version")?;
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
