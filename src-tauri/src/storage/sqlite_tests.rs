use uuid::Uuid;

use super::{
    LocalStore,
    models::{StoredSession, StoredTranscriptSegment, SummaryVersion},
};

fn fixture_session() -> StoredSession {
    StoredSession {
        id: Uuid::new_v4(),
        started_at: 1_799_000_000,
        mode: "live".into(),
        direction: "source_to_target".into(),
        source_language: "en".into(),
        target_language: "fr".into(),
    }
}

fn fixture_segment(session_id: Uuid) -> StoredTranscriptSegment {
    StoredTranscriptSegment {
        id: Uuid::new_v4(),
        session_id,
        direction: "source_to_target".into(),
        sequence: 1,
        occurred_at: 1_799_000_001,
        language: "en".into(),
        text: "Hello".into(),
        is_final: true,
        details: None,
        occurred_at_ms: None,
    }
}

#[test]
fn tied_session_timestamps_paginate_without_duplicates() {
    let store = LocalStore::in_memory().unwrap();
    let mut sessions = Vec::new();
    for _ in 0..3 {
        let session = fixture_session();
        store.create_session(&session).unwrap();
        sessions.push(session);
    }
    let first = store.list_sessions(2, None).unwrap();
    assert_eq!(first.len(), 2);
    let second = store
        .list_sessions(2, Some(first.last().unwrap().cursor()))
        .unwrap();
    assert_eq!(second.len(), 1);
    assert!(first.iter().all(|a| second.iter().all(|b| a.id != b.id)));
}

#[test]
fn summary_versions_are_transactionally_persisted_and_listed() {
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let summary = SummaryVersion {
        id: Uuid::new_v4(),
        session_id: session.id,
        version: 1,
        created_at: 10,
        status: "complete".into(),
        structured_json: r#"{"text":"Hello"}"#.into(),
    };
    store.create_summary_version(&summary).unwrap();
    assert_eq!(
        store.list_summary_versions(session.id).unwrap(),
        vec![summary.clone()]
    );
    assert!(
        store
            .create_summary_version(&SummaryVersion {
                id: Uuid::new_v4(),
                version: 1,
                ..summary
            })
            .is_err()
    );
}

#[test]
fn deleting_session_removes_segments_and_summary() {
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    store
        .append_final_segment(&fixture_segment(session.id))
        .unwrap();
    store.delete_session(session.id).unwrap();
    assert!(store.get_session(session.id).unwrap().is_none());
    assert_eq!(store.count_segments(session.id).unwrap(), 0);
    assert_eq!(store.count_summaries(session.id).unwrap(), 0);
}

#[test]
fn final_segments_are_unique_by_session_direction_and_sequence() {
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    store
        .append_final_segment(&fixture_segment(session.id))
        .unwrap();
    assert!(
        store
            .append_final_segment(&fixture_segment(session.id))
            .is_err()
    );
}

#[test]
fn interim_segments_are_not_persisted() {
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let mut interim = fixture_segment(session.id);
    interim.is_final = false;
    assert!(store.append_final_segment(&interim).is_err());
    assert_eq!(store.count_segments(session.id).unwrap(), 0);
}

#[test]
fn legacy_database_migrates_once_preserving_original_rows() {
    let dir = tempfile::tempdir().unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
        .unwrap();
    let s = fixture_session();
    let row = fixture_segment(s.id);
    connection.execute("INSERT INTO sessions (id,started_at,mode,direction,source_language,target_language) VALUES (?1,1,'live','source_to_target','en','fr')", [s.id.to_string()]).unwrap();
    connection
        .execute(
            "INSERT INTO session_lifecycle (session_id,status) VALUES (?1,'completed')",
            [s.id.to_string()],
        )
        .unwrap();
    connection.execute("INSERT INTO translation_segments (id,session_id,direction,sequence,occurred_at,language,text,is_final) VALUES (?1,?2,'source_to_target',1,1799000001,'en','Hello',1)", rusqlite::params![row.id.to_string(), s.id.to_string()]).unwrap();
    for _ in 0..2 {
        let store = LocalStore::open(dir.path()).unwrap();
        assert_eq!(
            store.get_export_data(s.id).unwrap().unwrap().segments,
            vec![row.clone()]
        );
        let page = store.transcript_page(s.id, None, 50).unwrap();
        assert_eq!(page.rows[0].id, row.id);
        assert_eq!(page.rows[0].occurred_at_ms, 1_799_000_001_000);
        assert!(page.rows[0].details.is_none());
    }
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
}
#[test]
fn failed_migration_rolls_back_columns_version_and_preserves_data() {
    let dir = tempfile::tempdir().unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
        .unwrap();
    connection.execute_batch("CREATE TABLE session_runtime_context (conflict TEXT); INSERT INTO documents (id,sha256,uri,created_at) VALUES ('keep','hash','file',1);").unwrap();
    assert!(LocalStore::open(dir.path()).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        connection
            .prepare("SELECT details_json FROM translation_segments")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT id FROM documents", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}
#[test]
fn unknown_newer_schema_is_rejected_without_creating_tables() {
    let dir = tempfile::tempdir().unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    assert!(LocalStore::open(dir.path()).is_err());
    assert!(connection.prepare("SELECT id FROM sessions").is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
}

#[test]
fn intelligence_migration_from_v2_reopens_twice_and_preserves_rows() {
    let dir = tempfile::tempdir().unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0002_runtime.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    connection
        .execute(
            "INSERT INTO documents (id,sha256,uri,created_at) VALUES ('keep','hash','file',1)",
            [],
        )
        .unwrap();
    for _ in 0..2 {
        drop(LocalStore::open(dir.path()).unwrap());
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            5
        );
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM intelligence_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT id FROM documents", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep"
        );
    }
}

#[test]
fn intelligence_migration_failure_rolls_back_all_new_tables_and_version() {
    let dir = tempfile::tempdir().unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0001_session_foundation.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../../migrations/0002_runtime.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    connection.execute_batch("CREATE TABLE service_probes (conflict TEXT); INSERT INTO service_probes VALUES ('keep');").unwrap();
    assert!(LocalStore::open(dir.path()).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(
        connection
            .prepare("SELECT id FROM intelligence_jobs")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT conflict FROM service_probes", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}

fn typed_row(
    session_id: Uuid,
    direction: crate::domain::session::SessionDirection,
    sequence: u64,
) -> crate::domain::transcript::TranscriptUpdate {
    use crate::domain::{session::ProviderId, transcript::*};
    TranscriptUpdate {
        id: Uuid::new_v4(),
        session_id,
        direction,
        sequence,
        occurred_at_ms: 10123,
        language: "en".into(),
        text: "pump".into(),
        is_final: true,
        details: Some(TranscriptDetails {
            kind: TranscriptKind::Source,
            provider: ProviderId::new("fixture").unwrap(),
            provider_config_id: Uuid::new_v4(),
            stream_epoch: 0,
            provider_event_type: Some("652".into()),
            provider_timestamp_ms: Some(10),
            sentence_id: None,
            local_correlation_id: Some(3),
            log_id: None,
            start_ms: Some(10),
            end_ms: Some(30),
        }),
    }
}
#[test]
fn transcript_pages_follow_insertion_across_directions_and_sessions() {
    use crate::domain::session::SessionDirection::*;
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let other = fixture_session();
    store.create_session(&other).unwrap();
    let a = typed_row(session.id, SourceToTarget, 2);
    let b = typed_row(session.id, TargetToSource, 0);
    let c = typed_row(session.id, SourceToTarget, 1);
    store.append_transcript(&a).unwrap();
    store
        .append_transcript(&typed_row(other.id, SourceToTarget, 0))
        .unwrap();
    store.append_transcript(&b).unwrap();
    store.append_transcript(&c).unwrap();
    let first = store.transcript_page(session.id, None, 2).unwrap();
    assert_eq!(first.rows, vec![a, b]);
    let last = store
        .transcript_page(session.id, first.next_cursor, 2)
        .unwrap();
    assert_eq!(last.rows, vec![c]);
    assert!(last.next_cursor.is_none());
    assert_eq!(
        store
            .next_transcript_sequence(session.id, SourceToTarget)
            .unwrap(),
        3
    );
    assert_eq!(
        store
            .next_transcript_sequence(session.id, TargetToSource)
            .unwrap(),
        1
    );
    assert!(store.transcript_page(session.id, None, 0).is_err());
    assert!(store.transcript_page(session.id, None, 201).is_err());
    assert!(store.transcript_page(session.id, Some(-1), 20).is_err());
}
#[test]
fn new_transcripts_reject_interims_missing_details_and_overflow() {
    use crate::domain::session::SessionDirection::*;
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let row = typed_row(session.id, SourceToTarget, 0);
    let mut bad = row.clone();
    bad.is_final = false;
    assert!(store.append_transcript(&bad).is_err());
    bad = row.clone();
    bad.details = None;
    assert!(store.append_transcript(&bad).is_err());
    bad = row.clone();
    bad.sequence = u64::MAX;
    assert!(store.append_transcript(&bad).is_err());
    bad = row.clone();
    bad.direction = Bidirectional;
    assert!(store.append_transcript(&bad).is_err());
    assert_eq!(store.count_segments(session.id).unwrap(), 0);
    store.append_transcript(&row).unwrap();
}
fn diagnostic(session_id: Uuid) -> crate::domain::transcript::RuntimeDiagnostic {
    use crate::domain::transcript::*;
    RuntimeDiagnostic {
        id: Uuid::new_v4(),
        session_id,
        direction: None,
        stream_epoch: None,
        occurred_at_ms: 9,
        payload: DiagnosticPayload::Failure {
            code: RuntimeFailureCode::Storage,
            provider_code: None,
            log_id: Some("log_1".into()),
        },
    }
}
#[test]
fn diagnostic_pages_and_exports_persist_safe_payloads_and_cascade_delete() {
    let store = LocalStore::in_memory().unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let a = diagnostic(session.id);
    let b = diagnostic(session.id);
    store.append_diagnostic(&a).unwrap();
    store.append_diagnostic(&b).unwrap();
    let page = store.diagnostic_page(session.id, None, 1).unwrap();
    assert_eq!(page.rows, vec![a.clone()]);
    let page = store
        .diagnostic_page(session.id, page.next_cursor, 1)
        .unwrap();
    assert_eq!(page.rows, vec![b.clone()]);
    assert!(page.next_cursor.is_none());
    assert_eq!(
        store
            .get_export_data(session.id)
            .unwrap()
            .unwrap()
            .diagnostics,
        vec![a, b]
    );
    store.delete_session(session.id).unwrap();
    assert!(
        store
            .diagnostic_page(session.id, None, 20)
            .unwrap()
            .rows
            .is_empty()
    );
}
#[test]
fn diagnostics_reject_unsafe_log_ids_and_storage_failure() {
    use crate::domain::transcript::*;
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(dir.path()).unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let mut bad = diagnostic(session.id);
    bad.payload = DiagnosticPayload::Failure {
        code: RuntimeFailureCode::Provider,
        provider_code: Some(401),
        log_id: Some("raw remote text".into()),
    };
    assert!(store.append_diagnostic(&bad).is_err());
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_diagnostic BEFORE INSERT ON session_diagnostics BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    assert!(store.append_diagnostic(&diagnostic(session.id)).is_err());
    assert!(
        store
            .diagnostic_page(session.id, None, 20)
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
fn export_reads_persisted_context_and_deletion_cascades_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(dir.path()).unwrap();
    let session = fixture_session();
    store.create_session(&session).unwrap();
    let connection = rusqlite::Connection::open(dir.path().join("vox-bridge.sqlite")).unwrap();
    connection
        .execute(
            "INSERT INTO session_runtime_context (session_id,context_json) VALUES (?1,?2)",
            rusqlite::params![session.id.to_string(), r#"{"provider_name":"Profile one"}"#],
        )
        .unwrap();
    let data = store.get_export_data(session.id).unwrap().unwrap();
    assert_eq!(data.context.unwrap()["provider_name"], "Profile one");
    store.delete_session(session.id).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM session_runtime_context", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
