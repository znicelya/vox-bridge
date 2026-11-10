use super::*;
use crate::{
    app::{CredentialChange, SaveProviderRequest},
    config::SecretStore,
    intelligence::{
        jobs::{JobContext, JobHandler},
        test_support::payload,
        types::*,
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct WaitAtShutdown {
    entered: AtomicUsize,
}
#[async_trait::async_trait]
impl JobHandler for WaitAtShutdown {
    async fn run(
        &self,
        mut context: JobContext,
        _: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        crate::intelligence::http::cancelled(&mut context.cancel).await;
        Err(IntelligenceErrorCode::Cancelled)
    }
}

#[tokio::test]
async fn window_close_jobs_shutdown_error_still_joins_delivery_and_retry_keeps_the_same_key() {
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    let handler = Arc::new(WaitAtShutdown {
        entered: AtomicUsize::new(0),
    });
    crate::intelligence::jobs::test_support::replace_pre_start_handler(
        &app.jobs,
        JobKind::Reindex,
        handler.clone(),
    )
    .unwrap();
    app.start_intelligence().await.unwrap();
    for _ in 0..2 {
        let id = Uuid::new_v4();
        app.jobs
            .enqueue(
                JobKind::Reindex,
                None,
                Some(id),
                JobAdmission::Manual {
                    exclusion_key: format!("document:{id}"),
                },
                payload(vec![]),
            )
            .await
            .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while handler.entered.load(Ordering::SeqCst) != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let id = Uuid::new_v4();
    let queued = app
        .jobs
        .enqueue(
            JobKind::Reindex,
            None,
            Some(id),
            JobAdmission::Manual {
                exclusion_key: format!("document:{id}"),
            },
            payload(vec![]),
        )
        .await
        .unwrap();
    assert_eq!(queued.status, JobStatus::Queued);
    let session = app.create_session(h.launch.clone()).await.unwrap().id;
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(session)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    app.store.transaction(|tx| {
        tx.execute_batch("CREATE TRIGGER reject_queued_cancellation BEFORE UPDATE ON intelligence_jobs WHEN OLD.status='queued' AND NEW.status='cancelled' BEGIN SELECT RAISE(FAIL,'injected jobs shutdown failure'); END;")?;
        Ok(())
    }).unwrap();
    let failed = app.close_all().await;
    let key = {
        let state = app.close_state.lock().unwrap();
        assert!(state.failures[2].is_some());
        assert!(
            state.failures[6].is_none(),
            "jobs failure must not abandon the independent owned delivery branch"
        );
        assert!(state.audio_ack.is_some());
        state.audio_key.clone().unwrap()
    };
    let rows = app.list_summaries(session, None, None).unwrap().rows;
    let crate::summary::types::SummaryView::Native(row) = &rows[0] else {
        panic!("local close retry must be Native")
    };
    assert_eq!(row.error, Some(IntelligenceErrorCode::Interrupted));
    assert!(failed.is_err());
    assert!(app.open_window().is_err());
    app.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER reject_queued_cancellation")?;
            Ok(())
        })
        .unwrap();
    app.close_all().await.unwrap();
    assert!(app.close_state.lock().unwrap().audio_key.as_ref() == Some(&key));
    assert_eq!(
        app.get_job(queued.id).unwrap().unwrap().status,
        JobStatus::Cancelled
    );
    app.open_window().unwrap();
    app.close_all().await.unwrap();
}

#[tokio::test]
async fn window_close_sql_repair_disposes_automatic_journal_before_same_key_reopen() {
    use crate::{
        app::summary_test_support::{enable_automatic, with_runtime_http},
        audio::PcmSpec,
        intelligence::test_support::RecordingHttp,
        providers::{
            ProviderEvent,
            fixture::{FixtureScript, FixtureStep},
        },
        session::test_support::{RuntimeHarness, final_text, metadata},
        summary::types::SummaryView,
    };
    let h = RuntimeHarness::live(
        FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::Emit(ProviderEvent::TranslationFinal {
                    metadata: metadata(),
                    text: final_text("zh", "retained final for repaired close"),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(RecordingHttp::default());
    let app = with_runtime_http(dir.path(), &h, http.clone());
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    enable_automatic(&app).await;
    app.start_intelligence().await.unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap().id;
    crate::app::tests::eventually(|| app.store.count_segments(session).unwrap() == 1).await;
    app.store
        .fail_lifecycle_status_for_tests("completed")
        .unwrap();
    let first = app.close_all().await;
    let (key, original_failure) = {
        let state = app.close_state.lock().unwrap();
        let key = state.audio_key.clone().unwrap();
        state.audio_ack.as_ref().unwrap().require_key(&key).unwrap();
        assert!(state.failures[0].is_some());
        assert!(state.failures[6].is_none());
        (key, state.first_failure.clone())
    };
    let journal = || {
        app.store
            .transaction(|tx| {
                Ok(tx.query_row(
                    "SELECT summary_terminal_pending FROM session_lifecycle WHERE session_id=?1",
                    [session.to_string()],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
    };
    assert!(first.is_err());
    assert_eq!(journal(), 0);
    assert!(
        app.list_summaries(session, None, None)
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(http.requests.lock().unwrap().is_empty());
    assert!(app.open_window().is_err());
    assert!(
        app.close_all().await.is_err(),
        "unresolved SQL must stay fenced"
    );
    assert_eq!(journal(), 0);
    assert!(app.open_window().is_err());
    app.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER fail_completed")?;
            Ok(())
        })
        .unwrap();
    let repaired = app.close_all().await;
    // Capture the actual effects before any reopen could run startup recovery.
    let pending = journal();
    let rows = app.list_summaries(session, None, None).unwrap().rows;
    let identities = app.store.transaction(|tx| Ok(tx.query_row(
        "SELECT count(*) FROM intelligence_jobs WHERE kind='summary' AND session_id=?1 AND permanent_key='auto-summary:' || ?1",
        [session.to_string()], |r| r.get::<_, i64>(0),
    )?)).unwrap();
    let state = app.close_state.lock().unwrap();
    assert!(state.audio_key.as_ref() == Some(&key));
    state.audio_ack.as_ref().unwrap().require_key(&key).unwrap();
    assert_eq!(state.first_failure, original_failure);
    drop(state);
    assert!(
        repaired.is_ok(),
        "corrected same-key close failed: {repaired:?}"
    );
    assert_eq!(
        pending, 0,
        "current delivery tail must consume the repaired journal before reopen"
    );
    assert_eq!(rows.len(), 1);
    let SummaryView::Native(row) = &rows[0] else {
        panic!("repair must materialize Native")
    };
    assert_eq!(row.version, 1);
    assert_eq!(row.status, JobStatus::Failed);
    assert_eq!(row.error, Some(IntelligenceErrorCode::Interrupted));
    assert_eq!(
        identities, 1,
        "automatic identity must already be permanent"
    );
    assert!(http.requests.lock().unwrap().is_empty());
    app.open_window().unwrap();
    app.close_all().await.unwrap();
}

#[tokio::test]
async fn window_close_sql_repair_result_survives_a_failed_delivery_tail_retry() {
    use crate::app::summary_test_support::{enable_automatic, with_runtime_http};
    let h = crate::app::tests::fixture();
    let dir = tempfile::tempdir().unwrap();
    let http = Arc::new(crate::intelligence::test_support::RecordingHttp::default());
    let app = with_runtime_http(dir.path(), &h, http.clone());
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    enable_automatic(&app).await;
    app.start_intelligence().await.unwrap();
    let session = app.create_session(h.launch.clone()).await.unwrap().id;
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(session)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    app.store
        .fail_lifecycle_status_for_tests("completed")
        .unwrap();
    assert!(app.close_all().await.is_err());
    let key = app.close_state.lock().unwrap().audio_key.clone().unwrap();
    let first_failure = app.close_state.lock().unwrap().first_failure.clone();
    app.store.transaction(|tx| {
        tx.execute_batch("DROP TRIGGER fail_completed; CREATE TRIGGER reject_repaired_summary BEFORE INSERT ON session_summaries BEGIN SELECT RAISE(FAIL,'injected repaired delivery failure'); END;")?;
        Ok(())
    }).unwrap();
    assert!(app.close_all().await.is_err());
    let journal =
        app.store
            .transaction(|tx| {
                Ok(tx.query_row(
        "SELECT status,summary_terminal_pending FROM session_lifecycle WHERE session_id=?1",
        [session.to_string()], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    )?)
            })
            .unwrap();
    assert_eq!(journal, ("completed".into(), 1));
    assert!(app.open_window().is_err());
    {
        let state = app.close_state.lock().unwrap();
        assert!(
            state.failures[0].is_none(),
            "the real terminal repair already succeeded"
        );
        assert!(
            state.failures[6].is_some(),
            "failed current delivery must stay visible"
        );
    }
    app.store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER reject_repaired_summary")?;
            Ok(())
        })
        .unwrap();
    app.close_all().await.unwrap();
    let state = app.close_state.lock().unwrap();
    assert!(state.audio_key.as_ref() == Some(&key));
    assert_eq!(state.first_failure, first_failure);
    drop(state);
    let journal = app
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT summary_terminal_pending FROM session_lifecycle WHERE session_id=?1",
                [session.to_string()],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(journal, 0);
    assert_eq!(
        app.list_summaries(session, None, None).unwrap().rows.len(),
        1
    );
    assert!(http.requests.lock().unwrap().is_empty());
    app.open_window().unwrap();
    app.close_all().await.unwrap();
}
