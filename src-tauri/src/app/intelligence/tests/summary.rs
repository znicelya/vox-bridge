use super::*;
use crate::{storage::StoredSession, summary::SummaryJobRequest};
fn session(app: &AppState) -> Uuid {
    let id = Uuid::new_v4();
    app.store
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
async fn terminal(app: &AppState, id: Uuid) -> JobView {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let job = app.get_job(id).unwrap().unwrap();
            if !matches!(job.status, JobStatus::Queued | JobStatus::Running) {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn summary_app_fresh_process_recovery_interrupts_registered_native_then_recovers_delete_before_manual_admission()
 {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    let session = session(&app);
    let old = app
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: session,
            profile: None,
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    app.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE intelligence_jobs SET status='running' WHERE id=?1",
                [old.id.to_string()],
            )?;
            tx.execute(
                "UPDATE session_summaries SET status='running',deleting=1 WHERE id=?1",
                [old.object_id.unwrap().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    // The first owner is inert; releasing its actual process lock permits the
    // next AppState to establish genuinely fresh process authority.
    drop(app);
    let reopened = AppState::new(dir.path().to_path_buf()).unwrap();
    let prior = reopened.list_summaries(session, None, None).unwrap();
    let crate::summary::types::SummaryView::Native(prior) = &prior.rows[0] else {
        panic!("native startup hook was not registered");
    };
    assert_eq!(prior.status, JobStatus::Interrupted);
    assert!(prior.deleting);
    assert!(reopened.generate_summary(session, None).await.is_err());
    reopened.start_intelligence().await.unwrap();
    assert!(
        reopened
            .list_summaries(session, None, None)
            .unwrap()
            .rows
            .is_empty()
    );
    let next = reopened.generate_summary(session, None).await.unwrap();
    assert_eq!(
        terminal(&reopened, next.id).await.error,
        Some(IntelligenceErrorCode::NoFinalText)
    );
    let rows = reopened.list_summaries(session, None, None).unwrap();
    let crate::summary::types::SummaryView::Native(row) = &rows.rows[0] else {
        panic!("new manual row lost Native origin");
    };
    assert_eq!(row.version, 2);
    assert!(reopened.get_job(old.id).unwrap().is_some());
    reopened.close_all().await.unwrap();
}
#[tokio::test]
async fn summary_app_joined_same_process_close_rearm_recovers_delete_and_opens_only_new_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    let session = session(&app);
    app.start_intelligence().await.unwrap();
    let old = app.generate_summary(session, None).await.unwrap();
    terminal(&app, old.id).await;
    app.close_all().await.unwrap();
    assert!(app.generate_summary(session, None).await.is_err());
    app.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET deleting=1 WHERE id=?1",
                [old.object_id.unwrap().to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    app.reopen_window().await.unwrap();
    assert!(
        app.list_summaries(session, None, None)
            .unwrap()
            .rows
            .is_empty()
    );
    let new = app.generate_summary(session, None).await.unwrap();
    terminal(&app, new.id).await;
    let page = app.list_summaries(session, None, None).unwrap();
    let crate::summary::types::SummaryView::Native(row) = &page.rows[0] else {
        panic!("new epoch lost Native origin");
    };
    assert_eq!(row.version, 2);
    assert_ne!(old.id, new.id);
    app.close_all().await.unwrap();
}
#[tokio::test]
async fn summary_app_failed_local_recovery_keeps_manual_readiness_closed_until_successful_retry() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    let session = session(&app);
    let old = app
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: session,
            profile: None,
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    app.jobs.cancel(old.id).await.unwrap();
    let witness = app
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT job_payload_hash FROM session_summaries WHERE id=?1",
                [old.object_id.unwrap().to_string()],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    app.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",
                rusqlite::params![old.object_id.unwrap().to_string(), "b".repeat(64)],
            )?;
            Ok(())
        })
        .unwrap();
    let startup = app.start_intelligence().await;
    assert!(matches!(
        startup,
        Err(AppError::Intelligence(IntelligenceErrorCode::Storage))
    ));
    assert!(!app.is_index_running_for_test());
    assert!(app.generate_summary(session, None).await.is_err());
    assert_eq!(app.store.count_summaries(session).unwrap(), 1);
    app.store
        .transaction(|tx| {
            tx.execute(
                "UPDATE session_summaries SET job_payload_hash=?2 WHERE id=?1",
                rusqlite::params![old.object_id.unwrap().to_string(), witness],
            )?;
            Ok(())
        })
        .unwrap();
    app.start_intelligence().await.unwrap();
    let new = app.generate_summary(session, None).await.unwrap();
    terminal(&app, new.id).await;
    assert_eq!(app.store.count_summaries(session).unwrap(), 2);
    app.close_all().await.unwrap();
}

#[tokio::test]
async fn summary_app_repeated_started_startup_preserves_current_ready_epoch_and_owned_index() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    let session = session(&app);
    app.start_intelligence().await.unwrap();
    let before = app.index_epoch_for_test().unwrap();
    let again = app.start_intelligence().await;
    let after = app.index_epoch_for_test();
    let generated = app.generate_summary(session, None).await;
    if let Ok(job) = &generated {
        terminal(&app, job.id).await;
    }
    app.close_all().await.unwrap();
    assert!(
        again.is_ok(),
        "repeating the previously idempotent startup must preserve open summary authority: {again:?}"
    );
    assert_eq!(after.unwrap(), before);
    assert!(generated.is_ok());
}
