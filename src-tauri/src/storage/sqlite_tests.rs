use uuid::Uuid;

use super::{
    LocalStore,
    models::{SessionRecord, TranscriptSegment},
};

fn fixture_session() -> SessionRecord {
    SessionRecord {
        id: Uuid::new_v4(),
        started_at: 1_799_000_000,
        mode: "live".into(),
        direction: "source_to_target".into(),
        source_language: "en".into(),
        target_language: "fr".into(),
    }
}

fn fixture_segment(session_id: Uuid) -> TranscriptSegment {
    TranscriptSegment {
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
