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
