use crate::storage::{LocalStore, StoredSession, SummaryVersion};
use rusqlite::{Connection, params};
use uuid::Uuid;

fn session(store: &LocalStore) -> Uuid {
    let id = Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    id
}

#[test]
fn summary_migration6_preserves_legacy_and_durable_counter_on_reopen() {
    // Removing the durable counter advancement would permit version reuse.
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(dir.path()).unwrap();
    let id = session(&store);
    let row = SummaryVersion {
        id: Uuid::new_v4(),
        session_id: id,
        version: 73,
        created_at: -123,
        status: "complete".into(),
        structured_json: r#"{"title":"旧版","text":"完整文本"}"#.into(),
    };
    store.create_summary_version(&row).unwrap();
    let version = store
        .transaction(|tx| Ok(tx.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?))
        .unwrap();
    assert_eq!(
        version, 6,
        "completed upgrade must expose the manual-summary schema"
    );
    store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM session_summaries WHERE id=?1",
                [row.id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    drop(store);
    let reopened = LocalStore::open(dir.path()).unwrap();
    assert_eq!(
        reopened
            .transaction(|tx| Ok(tx.query_row(
                "SELECT last_version FROM summary_version_counters WHERE session_id=?1",
                [id.to_string()],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        73
    );
}

fn old_database(dir: &std::path::Path) -> Connection {
    let c = Connection::open(dir.join("vox-bridge.sqlite")).unwrap();
    for sql in [
        include_str!("../../../migrations/0001_session_foundation.sql"),
        include_str!("../../../migrations/0002_runtime.sql"),
        include_str!("../../../migrations/0003_intelligence.sql"),
        include_str!("../../../migrations/0004_knowledge.sql"),
        include_str!("../../../migrations/0005_knowledge_index.sql"),
    ] {
        c.execute_batch(sql).unwrap();
    }
    c.pragma_update(None, "user_version", 5).unwrap();
    c
}

#[test]
fn summary_migration6_seeds_negative_zero_and_large_signed_exhaustion_without_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let c = old_database(dir.path());
    let negative = Uuid::new_v4();
    let exhausted = Uuid::new_v4();
    for (id, version) in [(negative, -9_i64), (exhausted, 9_007_199_254_740_993_i64)] {
        c.execute(
            "INSERT INTO sessions VALUES(?1,1,'live','unknown','en','zh')",
            [id.to_string()],
        )
        .unwrap();
        c.execute(
            "INSERT INTO session_lifecycle(session_id,status) VALUES(?1,'completed')",
            [id.to_string()],
        )
        .unwrap();
        c.execute(
            "INSERT INTO session_summaries VALUES(?1,?2,?3,-17,'complete',?4)",
            params![
                Uuid::new_v4().to_string(),
                id.to_string(),
                version,
                r#"{"text":"keep"}"#
            ],
        )
        .unwrap();
    }
    let store = LocalStore::open(dir.path()).unwrap();
    assert_eq!(
        store.list_summary_versions(negative).unwrap()[0].version,
        -9
    );
    assert_eq!(
        store.list_summary_versions(exhausted).unwrap()[0].created_at,
        -17
    );
    for (id, want) in [(negative, 0), (exhausted, 9_007_199_254_740_993)] {
        assert_eq!(
            store
                .transaction(|tx| Ok(tx.query_row(
                    "SELECT last_version FROM summary_version_counters WHERE session_id=?1",
                    [id.to_string()],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
            want
        );
    }
    assert_eq!(store.transaction(|tx| Ok(tx.query_row("SELECT count(*) FROM session_summaries WHERE origin='legacy' AND job_payload_hash IS NULL", [], |r| r.get::<_, i64>(0))?)).unwrap(), 2);
    drop(store);
    assert_eq!(
        LocalStore::open(dir.path())
            .unwrap()
            .list_summary_versions(exhausted)
            .unwrap()[0]
            .version,
        9_007_199_254_740_993
    );
}

#[test]
fn summary_migration6_failure_rolls_back_schema_and_original_rows() {
    let dir = tempfile::tempdir().unwrap();
    let c = old_database(dir.path());
    c.execute_batch("CREATE TABLE summary_responses(conflict TEXT); INSERT INTO summary_responses VALUES('keep')").unwrap();
    assert!(
        LocalStore::open(dir.path()).is_err(),
        "conflicting summary schema must fail the whole upgrade"
    );
    assert_eq!(
        c.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert!(c.prepare("SELECT origin FROM session_summaries").is_err());
    assert_eq!(
        c.query_row("SELECT conflict FROM summary_responses", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}
