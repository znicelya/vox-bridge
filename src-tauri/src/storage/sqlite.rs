use super::models::{SessionRecord, SessionSummary, TranscriptSegment};
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
        let conn = Connection::open(data_dir.join("vox-bridge.sqlite"))?;
        Self::initialize(conn)
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
    pub fn create_session(&self, session: &SessionRecord) -> AppResult<()> {
        let conn = self.connection.lock().unwrap();
        conn.execute("INSERT INTO sessions (id,started_at,mode,direction,source_language,target_language) VALUES (?1,?2,?3,?4,?5,?6)", params![session.id.to_string(), session.started_at, session.mode, session.direction, session.source_language, session.target_language])?;
        Ok(())
    }
    pub fn append_final_segment(&self, segment: &TranscriptSegment) -> AppResult<()> {
        if !segment.is_final {
            return Err(crate::error::AppError::InvalidRequest(
                "only final transcript segments may be persisted".into(),
            ));
        }
        let conn = self.connection.lock().unwrap();
        conn.execute("INSERT INTO translation_segments (id,session_id,direction,sequence,occurred_at,language,text,is_final) VALUES (?1,?2,?3,?4,?5,?6,?7,1)", params![segment.id.to_string(), segment.session_id.to_string(), segment.direction, segment.sequence, segment.occurred_at, segment.language, segment.text])?;
        Ok(())
    }
    pub fn list_sessions(
        &self,
        limit: u32,
        cursor: Option<String>,
    ) -> AppResult<Vec<SessionSummary>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id,started_at,mode,direction,source_language,target_language FROM sessions WHERE (?1 IS NULL OR started_at < CAST(?1 AS INTEGER)) ORDER BY started_at DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![cursor, limit], |r| {
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
        let conn = self.connection.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM sessions WHERE id=?1", params![id.to_string()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn get_session(&self, id: Uuid) -> AppResult<Option<SessionRecord>> {
        let conn = self.connection.lock().unwrap();
        Ok(conn.query_row("SELECT id,started_at,mode,direction,source_language,target_language FROM sessions WHERE id=?1", params![id.to_string()], |r| Ok(SessionRecord{id:Uuid::parse_str(&r.get::<_,String>(0)?).unwrap(),started_at:r.get(1)?,mode:r.get(2)?,direction:r.get(3)?,source_language:r.get(4)?,target_language:r.get(5)?})).optional()?)
    }
    pub fn count_segments(&self, id: Uuid) -> AppResult<u32> {
        let conn = self.connection.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM translation_segments WHERE session_id=?1",
            params![id.to_string()],
            |r| r.get(0),
        )?)
    }
    pub fn count_summaries(&self, id: Uuid) -> AppResult<u32> {
        let conn = self.connection.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM session_summaries WHERE session_id=?1",
            params![id.to_string()],
            |r| r.get(0),
        )?)
    }
}
