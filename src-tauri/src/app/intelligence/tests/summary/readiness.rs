use super::*;
use crate::{
    app::summary_test_support::{enable_automatic, with_runtime_http},
    intelligence::test_support::RecordingHttp,
    storage::{StoredSessionLifecycle, StoredTranscriptSegment},
    summary::readiness_test_support,
};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

async fn app_with_http() -> (tempfile::TempDir, AppState, Arc<RecordingHttp>) {
    let dir = tempfile::tempdir().unwrap();
    let runtime = crate::app::tests::fixture();
    let http = Arc::new(RecordingHttp::default());
    let app = with_runtime_http(dir.path(), &runtime, http.clone());
    enable_automatic(&app).await;
    (dir, app, http)
}

async fn configured_runtime_app() -> (
    tempfile::TempDir,
    AppState,
    crate::session::test_support::RuntimeHarness,
    Arc<RecordingHttp>,
) {
    let dir = tempfile::tempdir().unwrap();
    let runtime = crate::app::tests::fixture();
    let http = Arc::new(RecordingHttp::default());
    let app = with_runtime_http(dir.path(), &runtime, http.clone());
    app.save_provider_config(crate::app::SaveProviderRequest {
        config: runtime.saved.clone(),
        credential: crate::app::CredentialChange::Keep,
    })
    .await
    .unwrap();
    enable_automatic(&app).await;
    (dir, app, runtime, http)
}

fn retained_final(app: &AppState, session: Uuid) {
    app.store
        .append_final_segment(&StoredTranscriptSegment {
            id: Uuid::new_v4(),
            session_id: session,
            direction: "source_to_target".into(),
            sequence: 1,
            occurred_at: 2,
            language: "zh".into(),
            text: "retained manual input".into(),
            is_final: true,
            details: None,
            occurred_at_ms: None,
        })
        .unwrap();
}

fn mutation_snapshot(app: &AppState, session: Uuid) -> Value {
    let (counter, jobs, witnesses) = app.store.transaction(|tx| {
        let counter: i64 = tx.query_row(
            "SELECT coalesce((SELECT last_version FROM summary_version_counters WHERE session_id=?1),0)",
            [session.to_string()], |r| r.get(0),
        )?;
        let mut q = tx.prepare("SELECT id,status,payload_json,permanent_key,error_code FROM intelligence_jobs WHERE kind='summary' AND session_id=?1 ORDER BY id")?;
        let jobs = q.query_map([session.to_string()], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?,
        )))?.collect::<Result<Vec<_>, _>>()?;
        let mut q = tx.prepare("SELECT id,job_id,job_payload_hash,deleting FROM session_summaries WHERE session_id=?1 ORDER BY id")?;
        let witnesses = q.query_map([session.to_string()], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?,
        )))?.collect::<Result<Vec<_>, _>>()?;
        Ok((counter, jobs, witnesses))
    }).unwrap();
    json!({"summaries": app.list_summaries(session, None, None).unwrap(), "counter": counter, "jobs": jobs, "witnesses": witnesses})
}

fn is_fenced<T>(result: &AppResult<T>) -> bool {
    matches!(
        result,
        Err(AppError::Intelligence(
            IntelligenceErrorCode::Busy | IntelligenceErrorCode::Interrupted
        ))
    )
}

async fn drain_direct(app: &AppState) {
    let all = tokio::time::timeout(
        Duration::from_secs(2),
        app.intelligence_operations.clone().acquire_many_owned(16),
    )
    .await
    .unwrap()
    .unwrap();
    drop(all);
}

#[tokio::test]
async fn summary_app_manual_mutations_reject_local_ready_bootstrap_without_durable_changes() {
    let (_dir, app, http) = app_with_http().await;
    let bootstrap_session = session(&app);
    app.store
        .update_session_lifecycle(&StoredSessionLifecycle {
            session_id: bootstrap_session,
            status: "completed".into(),
            ended_at: Some(4),
        })
        .unwrap();
    let target = session(&app);
    let old = app
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: target,
            profile: None,
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    app.jobs.cancel(old.id).await.unwrap();
    let (entered, release) = app.service_config.hold_next_summary_selection_for_tests();
    let start_owner = app.clone();
    let startup = tokio::spawn(async move { start_owner.start_intelligence().await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    app.summaries.require_epoch(0).unwrap();
    assert!(app.summary_delivery.require_epoch(0).is_err());
    let before = mutation_snapshot(&app, target);
    let selections = app.service_config.summary_selection_entries_for_tests();
    let (generate, delete) = tokio::join!(
        tokio::time::timeout(
            Duration::from_millis(200),
            app.generate_summary(target, None)
        ),
        tokio::time::timeout(
            Duration::from_millis(200),
            app.delete_summary(old.object_id.unwrap())
        ),
    );
    let after = mutation_snapshot(&app, target);
    let calls = http.requests.lock().unwrap().len();
    let selection_delta = app.service_config.summary_selection_entries_for_tests() - selections;
    release.send(()).unwrap();
    startup.await.unwrap().unwrap();
    let live_job = app.generate_summary(target, None).await.unwrap();
    terminal(&app, live_job.id).await;
    app.delete_summary(live_job.object_id.unwrap())
        .await
        .unwrap();
    app.close_all().await.unwrap();
    println!(
        "held bootstrap: generate={generate:?}, delete={delete:?}, unchanged={}, selections={selection_delta}, provider_requests={calls}",
        before == after
    );
    assert!(
        generate.as_ref().is_ok_and(|result| is_fenced(result)),
        "generate must reject incomplete startup promptly: {generate:?}"
    );
    assert!(
        delete.as_ref().is_ok_and(|result| is_fenced(result)),
        "delete must reject incomplete startup promptly: {delete:?}"
    );
    assert_eq!(
        after, before,
        "held bootstrap cannot alter rows, deletion flags, counters, witnesses or permanent identities"
    );
    assert_eq!(selection_delta, 0);
    assert_eq!(calls, 0);
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_app_manual_generation_rejects_started_scheduler_before_delivery_publication() {
    let (_dir, app, http) = app_with_http().await;
    let target = session(&app);
    retained_final(&app, target);
    http.block.store(true, Ordering::SeqCst);
    let before = mutation_snapshot(&app, target);
    let (entered, release) = readiness_test_support::hold_publication(&app.summary_delivery);
    let start_owner = app.clone();
    let startup = tokio::spawn(async move { start_owner.start_intelligence().await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    app.summaries.require_epoch(0).unwrap();
    app.jobs
        .admission_fence(JobKind::Summary)
        .unwrap()
        .run(|| Ok(()))
        .unwrap();
    assert!(!startup.is_finished());
    let generated = tokio::time::timeout(
        Duration::from_millis(200),
        app.generate_summary(target, None),
    )
    .await;
    let model_entered = if generated.as_ref().is_ok_and(Result::is_ok) {
        tokio::time::timeout(Duration::from_secs(2), http.entered.notified())
            .await
            .is_ok()
    } else {
        false
    };
    let after = mutation_snapshot(&app, target);
    let calls = http.requests.lock().unwrap().len();
    release.send(()).unwrap();
    startup.await.unwrap().unwrap();
    http.block.store(false, Ordering::SeqCst);
    http.resume.notify_one();
    if let Ok(Ok(job)) = &generated {
        terminal(&app, job.id).await;
    }
    let live_job = app.generate_summary(target, None).await.unwrap();
    terminal(&app, live_job.id).await;
    app.delete_summary(live_job.object_id.unwrap())
        .await
        .unwrap();
    app.close_all().await.unwrap();
    println!(
        "held scheduler-to-Live: generation={generated:?}, unchanged={}, provider_requests={calls}, model_entered={model_entered}",
        before == after
    );
    assert!(
        generated.as_ref().is_ok_and(|result| is_fenced(result)),
        "started jobs cannot authorize generation before combined publication: {generated:?}"
    );
    assert_eq!(after, before);
    assert_eq!(
        calls, 0,
        "no provider dispatch before complete delivery readiness"
    );
    assert!(
        !http.requests.lock().unwrap().is_empty(),
        "generation after full readiness still executes the real handler"
    );
}

#[tokio::test]
async fn summary_app_manual_entered_config_rechecks_delivery_at_admission_after_observer_loss() {
    let (_dir, app, http) = app_with_http().await;
    app.start_intelligence().await.unwrap();
    drain_direct(&app).await;
    let target = session(&app);
    retained_final(&app, target);
    let before = mutation_snapshot(&app, target);
    let (entered, release) = app.service_config.hold_next_summary_selection_for_tests();
    let generate_owner = app.clone();
    let observer = tokio::spawn(async move { generate_owner.generate_summary(target, None).await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    let key = app
        .summary_delivery
        .elect_closing(1, app.audio.actual_epoch().unwrap())
        .unwrap();
    app.summaries.require_epoch(0).unwrap();
    app.jobs
        .admission_fence(JobKind::Summary)
        .unwrap()
        .run(|| Ok(()))
        .unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let held = app.intelligence_operations.available_permits();
    release.send(()).unwrap();
    drain_direct(&app).await;
    let after = mutation_snapshot(&app, target);
    app.close_all().await.unwrap();
    app.open_window().unwrap();
    app.close_all().await.unwrap();
    println!(
        "entered ConfigIo observer lost: direct_slots={held}, delivery={} unchanged={}",
        key.delivery_id(),
        before == after
    );
    assert_eq!(
        held, 15,
        "cancelled observer must retain the actual ConfigIo/direct owner"
    );
    assert_eq!(
        after, before,
        "actual admission must consume the delivery fence even with local/jobs readiness still open"
    );
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn summary_app_manual_entered_delete_sql_rechecks_delivery_mark_after_observer_loss() {
    let (_dir, app, http) = app_with_http().await;
    let target = session(&app);
    let old = app
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: target,
            profile: None,
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    app.jobs.cancel(old.id).await.unwrap();
    app.start_intelligence().await.unwrap();
    drain_direct(&app).await;
    let before = mutation_snapshot(&app, target);
    let (entered, release) = readiness_test_support::hold_sql_entry(&app.summaries);
    let delete_owner = app.clone();
    let observer =
        tokio::spawn(async move { delete_owner.delete_summary(old.object_id.unwrap()).await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    let key = app
        .summary_delivery
        .elect_closing(1, app.audio.actual_epoch().unwrap())
        .unwrap();
    app.summaries.require_epoch(0).unwrap();
    app.jobs
        .admission_fence(JobKind::Summary)
        .unwrap()
        .run(|| Ok(()))
        .unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let held = app.intelligence_operations.available_permits();
    release.send(()).unwrap();
    drain_direct(&app).await;
    let after = mutation_snapshot(&app, target);
    app.close_all().await.unwrap();
    app.open_window().unwrap();
    app.close_all().await.unwrap();
    println!(
        "entered SQLite observer lost: direct_slots={held}, delivery={} unchanged={}",
        key.delivery_id(),
        before == after
    );
    assert_eq!(
        held, 15,
        "cancelled observer must retain the actual SQL/direct owner"
    );
    assert_eq!(
        after, before,
        "actual deletion mark must consume complete delivery readiness"
    );
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_app_cancelled_issued_ready_observer_preserves_actual_publication_and_repeated_startup()
 {
    use crate::app::{CredentialChange, SaveProviderRequest};
    let dir = tempfile::tempdir().unwrap();
    let runtime = crate::app::tests::fixture();
    let http = Arc::new(RecordingHttp::default());
    let app = with_runtime_http(dir.path(), &runtime, http.clone());
    app.save_provider_config(SaveProviderRequest {
        config: runtime.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    enable_automatic(&app).await;
    let target = session(&app);
    let (entered, release, published) =
        readiness_test_support::hold_ready_issued(&app.summary_delivery);
    let start_owner = app.clone();
    let observer = tokio::spawn(async move { start_owner.start_intelligence().await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    let index = app.index_epoch_for_test().unwrap();
    observer.abort();
    assert!(observer.await.unwrap_err().is_cancelled());
    let repeat_before =
        tokio::time::timeout(Duration::from_millis(200), app.start_intelligence()).await;
    let manual_before = tokio::time::timeout(
        Duration::from_millis(200),
        app.generate_summary(target, None),
    )
    .await;
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), published)
        .await
        .unwrap()
        .unwrap();
    if let Ok(Ok(job)) = &manual_before {
        terminal(&app, job.id).await;
    }
    let repeated = app.start_intelligence().await;
    let after_index = app.index_epoch_for_test().unwrap();
    let runtime_session = app.create_session(runtime.launch.clone()).await.unwrap().id;
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(runtime_session)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    app.stop_session(runtime_session).await.unwrap();
    crate::app::tests::eventually(|| {
        !app.list_summaries(runtime_session, None, None)
            .unwrap()
            .rows
            .is_empty()
    })
    .await;
    let automatic = app
        .list_summaries(runtime_session, None, None)
        .unwrap()
        .rows;
    app.close_all().await.unwrap();
    println!(
        "cancelled issued Ready: repeated_before={repeat_before:?}, manual_before={manual_before:?}, automatic={automatic:?}"
    );
    assert!(
        repeat_before.as_ref().is_ok_and(|result| is_fenced(result)),
        "a missing actual App publication must not be reconstructed from started status"
    );
    assert!(manual_before.as_ref().is_ok_and(|result| is_fenced(result)));
    assert!(repeated.is_ok());
    assert_eq!(
        after_index, index,
        "repeat must preserve the actually published generation"
    );
    assert_eq!(automatic.len(), 1);
    let crate::summary::types::SummaryView::Native(row) = &automatic[0] else {
        panic!("automatic must be Native")
    };
    assert_eq!(
        row.error,
        Some(IntelligenceErrorCode::NoFinalText),
        "actual publisher must install the live terminal port independently of its observer"
    );
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn summary_app_manual_mutations_reject_foreign_and_retired_actual_ready_capabilities() {
    let (_first_dir, first, first_http) = app_with_http().await;
    let (_second_dir, second, second_http) = app_with_http().await;
    let target = session(&second);
    let old = second
        .store
        .create_summary_job(SummaryJobRequest {
            session_id: target,
            profile: None,
            automatic: false,
            initial_failure: None,
        })
        .unwrap();
    second.jobs.cancel(old.id).await.unwrap();
    first.start_intelligence().await.unwrap();
    second.start_intelligence().await.unwrap();
    let epoch = second.admitted_epoch().unwrap();
    let foreign = first.summary_readiness(epoch).unwrap();
    let retired = second.summary_readiness(epoch).unwrap();
    foreign.require_epoch(epoch).unwrap();
    retired.require_epoch(epoch).unwrap();

    let before_foreign = mutation_snapshot(&second, target);
    let foreign_selections = second.service_config.summary_selection_entries_for_tests();
    let (foreign_generate, foreign_delete) = tokio::join!(
        tokio::time::timeout(
            Duration::from_millis(200),
            second.summaries.generate_owned(
                target,
                None,
                second.intelligence_lease().unwrap(),
                foreign.clone(),
            ),
        ),
        tokio::time::timeout(
            Duration::from_millis(200),
            second.summaries.delete_owned(
                old.object_id.unwrap(),
                second.intelligence_lease().unwrap(),
                foreign,
            ),
        ),
    );
    let after_foreign = mutation_snapshot(&second, target);
    let foreign_selection_delta =
        second.service_config.summary_selection_entries_for_tests() - foreign_selections;

    second.close_all().await.unwrap();
    second.reopen_window().await.unwrap();
    let new_epoch = second.admitted_epoch().unwrap();
    second.summary_readiness(new_epoch).unwrap();
    let before_retired = mutation_snapshot(&second, target);
    let retired_selections = second.service_config.summary_selection_entries_for_tests();
    let (retired_generate, retired_delete) = tokio::join!(
        tokio::time::timeout(
            Duration::from_millis(200),
            second.summaries.generate_owned(
                target,
                None,
                second.intelligence_lease().unwrap(),
                retired.clone(),
            ),
        ),
        tokio::time::timeout(
            Duration::from_millis(200),
            second.summaries.delete_owned(
                old.object_id.unwrap(),
                second.intelligence_lease().unwrap(),
                retired,
            ),
        ),
    );
    let after_retired = mutation_snapshot(&second, target);
    let retired_selection_delta =
        second.service_config.summary_selection_entries_for_tests() - retired_selections;
    first.close_all().await.unwrap();
    second.close_all().await.unwrap();

    println!(
        "actual Ready identity: foreign_generate={foreign_generate:?}, foreign_delete={foreign_delete:?}, retired_generate={retired_generate:?}, retired_delete={retired_delete:?}, epochs={epoch}->{new_epoch}"
    );
    assert!(
        foreign_generate
            .as_ref()
            .is_ok_and(|result| is_fenced(result))
    );
    assert!(
        foreign_delete
            .as_ref()
            .is_ok_and(|result| is_fenced(result))
    );
    assert!(
        retired_generate
            .as_ref()
            .is_ok_and(|result| is_fenced(result))
    );
    assert!(
        retired_delete
            .as_ref()
            .is_ok_and(|result| is_fenced(result))
    );
    assert_eq!(after_foreign, before_foreign);
    assert_eq!(after_retired, before_retired);
    assert_ne!(new_epoch, epoch);
    assert_eq!(foreign_selection_delta, 0);
    assert_eq!(retired_selection_delta, 0);
    assert!(first_http.requests.lock().unwrap().is_empty());
    assert!(second_http.requests.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_app_runtime_creation_rejects_issued_ready_before_actual_publication() {
    let (_dir, app, runtime, http) = configured_runtime_app().await;
    let (entered, release, published) =
        readiness_test_support::hold_ready_issued(&app.summary_delivery);
    let startup_owner = app.clone();
    let startup = tokio::spawn(async move { startup_owner.start_intelligence().await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    let raw_delivery_ready = app.summary_delivery.require_epoch(0).is_ok();
    let app_ready_missing = app.summary_readiness(0).is_err();
    let before_sessions = app.list_sessions(Some(10), None).await.unwrap().len();
    let creation = tokio::time::timeout(
        Duration::from_millis(200),
        app.create_session(runtime.launch.clone()),
    )
    .await;
    if let Ok(Ok(record)) = &creation {
        crate::app::tests::eventually(|| {
            app.store
                .get_session_lifecycle(record.id)
                .unwrap()
                .unwrap()
                .status
                == "running"
        })
        .await;
    }
    let after_sessions = app.list_sessions(Some(10), None).await.unwrap().len();
    let inventory = runtime.audio.inventory_calls();
    let opens = runtime.audio.open_calls();
    let connections = runtime.provider.connect_counts();
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), published)
        .await
        .unwrap()
        .unwrap();
    startup.await.unwrap().unwrap();
    if let Ok(Ok(record)) = &creation {
        app.stop_session(record.id).await.unwrap();
    }
    let after_publication = app.create_session(runtime.launch.clone()).await.unwrap();
    app.stop_session(after_publication.id).await.unwrap();
    app.close_all().await.unwrap();

    println!(
        "issued Ready creation: result={creation:?}, sessions={before_sessions}->{after_sessions}, inventory={inventory}, opens={opens}, connections={connections:?}"
    );
    assert!(raw_delivery_ready && app_ready_missing);
    assert!(creation.as_ref().is_ok_and(|result| is_fenced(result)));
    assert_eq!(after_sessions, before_sessions);
    assert_eq!(inventory, 0);
    assert_eq!(opens, 0);
    assert_eq!(connections, [0, 0]);
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_app_runtime_resume_rejects_issued_ready_before_actual_publication() {
    use crate::domain::launch::DirectionPhase;
    let (_dir, app, runtime, http) = configured_runtime_app().await;
    // Establish a genuine retained paused runtime through the production
    // orchestrator, so this test can isolate the App resume admission gate.
    let record = app
        .orchestrator
        .start(runtime.launch.clone(), runtime.saved.clone())
        .await
        .unwrap();
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(record.id)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    runtime.audio.lose("live-input");
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(record.id)
            .unwrap()
            .unwrap()
            .status
            == "paused"
    })
    .await;
    runtime.audio.restore("live-input");
    let (entered, release, published) =
        readiness_test_support::hold_ready_issued(&app.summary_delivery);
    let startup_owner = app.clone();
    let startup = tokio::spawn(async move { startup_owner.start_intelligence().await });
    tokio::time::timeout(Duration::from_secs(2), entered)
        .await
        .unwrap()
        .unwrap();
    let raw_delivery_ready = app.summary_delivery.require_epoch(0).is_ok();
    let app_ready_missing = app.summary_readiness(0).is_err();
    let before_inventory = runtime.audio.inventory_calls();
    let before_opens = runtime.audio.open_calls();
    let before_connections = runtime.provider.connect_counts();
    let resume = tokio::time::timeout(
        Duration::from_millis(200),
        app.resume_session_direction(record.id, SessionDirection::SourceToTarget),
    )
    .await;
    let after_inventory = runtime.audio.inventory_calls();
    let after_opens = runtime.audio.open_calls();
    let after_connections = runtime.provider.connect_counts();
    let remained_paused = app
        .get_session(record.id)
        .await
        .unwrap()
        .unwrap()
        .runtime
        .unwrap()
        .directions
        .iter()
        .any(|direction| {
            direction.direction == SessionDirection::SourceToTarget
                && direction.phase == DirectionPhase::Paused
        });
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), published)
        .await
        .unwrap()
        .unwrap();
    startup.await.unwrap().unwrap();
    if let Ok(Ok(())) = &resume {
        runtime.audio.lose("live-input");
        crate::app::tests::eventually(|| {
            app.store
                .get_session_lifecycle(record.id)
                .unwrap()
                .unwrap()
                .status
                == "paused"
        })
        .await;
        runtime.audio.restore("live-input");
    }
    app.resume_session_direction(record.id, SessionDirection::SourceToTarget)
        .await
        .unwrap();
    app.close_all().await.unwrap();

    println!(
        "issued Ready resume: result={resume:?}, inventory={before_inventory}->{after_inventory}, opens={before_opens}->{after_opens}, connections={before_connections:?}->{after_connections:?}, remained_paused={remained_paused}"
    );
    assert!(raw_delivery_ready && app_ready_missing);
    assert!(resume.as_ref().is_ok_and(|result| is_fenced(result)));
    assert!(remained_paused);
    assert_eq!(after_inventory, before_inventory);
    assert_eq!(after_opens, before_opens);
    assert_eq!(after_connections, before_connections);
    assert!(http.requests.lock().unwrap().is_empty());
}
