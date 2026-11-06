use crate::{error::AppResult, storage::LocalStore};
use rusqlite::{Connection, params};
use uuid::Uuid;

#[test]
fn documents_migration_preserves_legacy_nullable_metadata_and_chunks_across_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let document = Uuid::new_v4().to_string();
    let chunk = Uuid::new_v4().to_string();
    let connection = Connection::open(directory.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0002_runtime.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    connection.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'legacy-hash','legacy-reference',NULL,37)", [&document]).unwrap();
    connection.execute("INSERT INTO document_chunks (id,document_id,chunk_index,text) VALUES (?1,?2,0,'legacy canonical text')", params![chunk,document]).unwrap();
    drop(connection);
    for _ in 0..2 {
        let store = LocalStore::open(directory.path()).unwrap();
        store.transaction(|tx| -> AppResult<()> {
            let old: (String, String, Option<String>, i64) = tx.query_row(
                "SELECT sha256,uri,title,created_at FROM documents WHERE id=?1", [&document],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
            )?;
            assert_eq!(old,("legacy-hash".into(),"legacy-reference".into(),None,37));
            let metadata: (String,String,u32,Option<String>,Option<String>,String,Option<i64>) = tx.query_row(
                "SELECT kind,origin,version,original_path,managed_path,status,updated_at_ms FROM documents WHERE id=?1", [&document],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
            )?;
            assert_eq!(metadata,("unknown".into(),"legacy".into(),1,None,None,"repair_required".into(),None));
            let retained: (String,String,u32,Option<String>,String) = tx.query_row(
                "SELECT id,text,chunk_index,vector_id,index_state FROM document_chunks WHERE document_id=?1", [&document],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
            )?;
            assert_eq!(retained,(chunk.clone(),"legacy canonical text".into(),0,None,"repair_required".into()));
            let version: u32 = tx.pragma_query_value(None,"user_version",|row|row.get(0))?;
            assert_eq!(version,4);
            Ok(())
        }).unwrap();
        assert!(
            !directory.path().join("knowledge").exists(),
            "legacy migration cannot create/copy managed sources"
        );
    }
}

#[test]
fn documents_migration_generation_seals_reject_untrusted_json_and_cascade_cleanup() {
    let store = LocalStore::in_memory().unwrap();
    store.transaction(|tx| {
        tx.execute("INSERT INTO knowledge_index_generations (identity,profile_id,dimension,active_generation,state) VALUES ('identity',NULL,128,NULL,'repair_required')",[])?;
        let insert="INSERT INTO knowledge_index_generation_seals (identity,generation,seal_json,dirty,intent_json,updated_at_ms) VALUES ('identity',?1,?2,?3,?4,17)";
        for (i,(seal,dirty,intent)) in [
            (None,0,None), (Some("{}"),0,None), (Some("null"),0,None), (Some("{"),0,None),
            (Some("{\"version\":null}"),0,None), (Some("{\"version\":true}"),0,None), (Some("{\"version\":1.0}"),0,None),
            (Some("{\"version\":2}"),0,None), (Some("{\"version\":1}"),0,Some("{\"version\":1}")),
            (None,1,None), (None,1,Some("{}")), (None,1,Some("null")), (None,1,Some("{")),
            (None,2,Some("{\"version\":1}")),
        ].into_iter().enumerate() {
            assert!(tx.execute(insert,params![format!("bad-{i}"),seal,dirty,intent]).is_err(),"invalid seal/intent {i} was accepted");
        }
        let large=format!("{{\"version\":1,\"data\":\"{}\"}}","x".repeat(64*1024));
        assert!(tx.execute(insert,params!["oversize-seal",large,0,None::<String>]).is_err());
        let large=format!("{{\"version\":1,\"data\":\"{}\"}}","中".repeat(6*1024));
        assert!(tx.execute(insert,params!["oversize-intent",None::<String>,1,large]).is_err(),"limit counts encoded bytes");
        tx.execute(insert,params!["fresh",None::<String>,1,"{\"version\":1}"])?;
        tx.execute(insert,params!["clean","{\"version\":1}",0,None::<String>])?;
        tx.execute(insert,params!["dirty","{\"version\":1}",1,"{\"version\":1}"])?;
        assert_eq!(tx.query_row("SELECT count(*) FROM knowledge_index_generation_seals",[],|row|row.get::<_,u32>(0))?,3);
        tx.execute("DELETE FROM knowledge_index_generations WHERE identity='identity'",[])?;
        assert_eq!(tx.query_row("SELECT count(*) FROM knowledge_index_generation_seals",[],|row|row.get::<_,u32>(0))?,0);
        assert!(tx.execute(insert,params!["orphan",None::<String>,1,"{\"version\":1}"]).is_err());
        Ok(())
    }).unwrap();
}

#[test]
fn documents_projection_pages_legacy_rows_without_rewriting_nullable_metadata() {
    use super::{
        store,
        types::{DocumentKind, DocumentStatus},
    };
    use crate::intelligence::types::RecordOrigin;
    let local = LocalStore::in_memory().unwrap();
    let ids: Vec<_> = (0..3).map(|_| Uuid::new_v4()).collect();
    local.transaction(|tx| {
        for id in &ids {
            tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'original-hash','legacy-reference',NULL,37)",[id.to_string()])?;
        }
        tx.execute("INSERT INTO document_chunks (id,document_id,chunk_index,text) VALUES (?1,?2,0,'retained')",params![Uuid::new_v4().to_string(),ids[0].to_string()])?;
        Ok(())
    }).unwrap();
    let detail = store::get(&local, ids[0])
        .unwrap()
        .expect("legacy row must remain visible");
    assert_eq!(detail.summary.title, None);
    assert_eq!(detail.summary.updated_at_ms, None);
    assert_eq!(detail.summary.created_at_ms, 37);
    assert_eq!(detail.summary.chunk_count, 1);
    assert_eq!(detail.summary.origin, RecordOrigin::Legacy);
    assert_eq!(detail.summary.kind, DocumentKind::Unknown);
    assert_eq!(detail.summary.status, DocumentStatus::RepairRequired);
    assert_eq!(detail.summary.sha256, "original-hash");
    assert_eq!(detail.legacy_uri.as_deref(), Some("legacy-reference"));
    assert_eq!(detail.original_path, None);
    assert_eq!(detail.managed_path, None);
    let first = store::page(&local, None, 2).unwrap();
    let next = store::page(&local, first.next_cursor, 2).unwrap();
    assert_eq!(
        first.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        ids[..2]
    );
    assert_eq!(
        next.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        ids[2..]
    );
    assert_eq!(next.next_cursor, None);
    assert_eq!(store::get(&local, ids[0]).unwrap(), Some(detail));
    assert!(store::get(&local, Uuid::new_v4()).unwrap().is_none());
    for (after, limit) in [(None, 0), (None, 201), (Some(-1), 50)] {
        assert!(store::page(&local, after, limit).is_err());
    }
}
#[test]
fn documents_migration4_failure_rolls_back_added_columns_and_preserves_legacy_rows() {
    let directory = tempfile::tempdir().unwrap();
    let connection = Connection::open(directory.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0002_runtime.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0003_intelligence.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    connection.execute_batch("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES ('retained','hash','legacy-reference',NULL,37); CREATE TABLE knowledge_index_generation_seals (conflict TEXT);").unwrap();
    assert!(LocalStore::open(directory.path()).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        3
    );
    assert!(connection.prepare("SELECT kind FROM documents").is_err());
    assert!(
        connection
            .prepare("SELECT identity FROM knowledge_index_generations")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT uri,title FROM documents WHERE id='retained'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            )
            .unwrap(),
        ("legacy-reference".into(), None)
    );
}

mod admission;
mod copying;
mod derived_result;
mod index_schema;
mod startup;
mod wire;
