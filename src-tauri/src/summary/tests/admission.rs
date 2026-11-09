use crate::{
    intelligence::{test_support::IntelligenceHarness, types::*},
    summary::SummaryJobRequest,
};
use rusqlite::params;
use uuid::Uuid;

fn request(id: Uuid, automatic: bool) -> SummaryJobRequest {
    SummaryJobRequest {
        session_id: id,
        profile: None,
        automatic,
        initial_failure: None,
    }
}
fn terminal(h: &IntelligenceHarness, id: Uuid) {
    h.store.transaction(|tx|{tx.execute("UPDATE intelligence_jobs SET status='failed',error_code='configuration' WHERE id=?1",[id.to_string()])?;Ok(())}).unwrap();
}
fn counter(h: &IntelligenceHarness, id: Uuid) -> i64 {
    h.store.transaction(|tx|Ok(tx.query_row("SELECT coalesce((SELECT last_version FROM summary_version_counters WHERE session_id=?1),0)",[id.to_string()],|r|r.get(0))?)).unwrap()
}

#[test]
fn summary_admission_delete_highest_and_all_versions_never_reuses_counter() {
    let h = IntelligenceHarness::new();
    let id = h.session();
    h.store
        .update_session_lifecycle(&crate::storage::StoredSessionLifecycle {
            session_id: id,
            status: "completed".into(),
            ended_at: Some(1),
        })
        .unwrap();
    let first = h.store.create_summary_job(request(id, false)).unwrap();
    terminal(&h, first.id);
    let second = h.store.create_summary_job(request(id, false)).unwrap();
    terminal(&h, second.id);
    h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM session_summaries WHERE id=?1",
                [second.object_id.unwrap().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let third = h.store.create_summary_job(request(id, false)).unwrap();
    terminal(&h, third.id);
    assert_eq!(
        h.store
            .list_summary_versions(id)
            .unwrap()
            .iter()
            .map(|v| v.version)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM session_summaries WHERE session_id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    h.store.create_summary_job(request(id, false)).unwrap();
    assert_eq!(h.store.list_summary_versions(id).unwrap()[0].version, 4);
    assert_eq!(counter(&h, id), 4);
}

#[test]
fn summary_admission_permanent_existing_survives_missing_rows_and_exhaustion() {
    let h = IntelligenceHarness::new();
    let id = h.session();
    h.store
        .update_session_lifecycle(&crate::storage::StoredSessionLifecycle {
            session_id: id,
            status: "completed".into(),
            ended_at: Some(1),
        })
        .unwrap();
    let first = h.store.create_summary_job(request(id, true)).unwrap();
    terminal(&h, first.id);
    h.store
        .transaction(|tx| {
            tx.execute(
                "DELETE FROM session_summaries WHERE session_id=?1",
                [id.to_string()],
            )?;
            tx.execute(
                "UPDATE summary_version_counters SET last_version=4294967295 WHERE session_id=?1",
                [id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let existing = h
        .store
        .create_summary_job(SummaryJobRequest {
            initial_failure: Some(IntelligenceErrorCode::Configuration),
            ..request(id, true)
        })
        .unwrap();
    assert_eq!(existing.id, first.id);
    assert_eq!(counter(&h, id), 4_294_967_295);
    assert!(h.store.list_summary_versions(id).unwrap().is_empty());
    assert_eq!(h.store.list_jobs(None, 100).unwrap().rows.len(), 1);
}

#[test]
fn summary_admission_insert_failure_rolls_back_job_version_counter_and_permanent_marker() {
    let h = IntelligenceHarness::new();
    let id = h.session();
    h.store
        .update_session_lifecycle(&crate::storage::StoredSessionLifecycle {
            session_id: id,
            status: "completed".into(),
            ended_at: Some(1),
        })
        .unwrap();
    h.store.transaction(|tx|{tx.execute_batch("CREATE TRIGGER fail_native_summary BEFORE INSERT ON session_summaries BEGIN SELECT RAISE(ABORT,'fixture'); END")?;Ok(())}).unwrap();
    assert!(h.store.create_summary_job(request(id, true)).is_err());
    assert_eq!(counter(&h, id), 0);
    assert!(h.store.list_jobs(None, 100).unwrap().rows.is_empty());
    assert_eq!(h.store.count_summaries(id).unwrap(), 0);
    h.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER fail_native_summary")?;
            Ok(())
        })
        .unwrap();
    h.store.create_summary_job(request(id, true)).unwrap();
    assert_eq!(counter(&h, id), 1);
}

#[test]
fn summary_admission_u32_exhaustion_inserts_nothing() {
    for maximum in [4_294_967_295_i64, 9_007_199_254_740_993] {
        let h = IntelligenceHarness::new();
        let id = h.session();
        h.store
            .update_session_lifecycle(&crate::storage::StoredSessionLifecycle {
                session_id: id,
                status: "completed".into(),
                ended_at: Some(1),
            })
            .unwrap();
        h.store
            .transaction(|tx| {
                tx.execute(
                    "INSERT INTO summary_version_counters VALUES(?1,?2)",
                    params![id.to_string(), maximum],
                )?;
                Ok(())
            })
            .unwrap();
        let result = h.store.create_summary_job(request(id, true));
        assert!(matches!(
            result,
            Err(crate::error::AppError::Intelligence(
                IntelligenceErrorCode::ResourceLimit
            ))
        ));
        assert_eq!(counter(&h, id), maximum);
        assert_eq!(h.store.count_summaries(id).unwrap(), 0);
        assert!(h.store.list_jobs(None, 100).unwrap().rows.is_empty());
    }
}
