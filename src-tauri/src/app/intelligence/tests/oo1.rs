use super::*;
use futures_util::FutureExt;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};
use tokio::sync::{Notify, oneshot, watch};

#[derive(Clone, Copy)]
enum Action {
    OrdinaryStop,
    AppClose,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::OrdinaryStop => "ordinary_stop",
            Self::AppClose => "app_close",
        }
    }
}

struct Timeline {
    start: Instant,
    events: Mutex<Vec<Value>>,
}

impl Timeline {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            start: Instant::now(),
            events: Mutex::new(Vec::new()),
        })
    }

    fn record(&self, name: &str, details: Value) {
        self.events.lock().unwrap().push(json!({
            "at_us": u64::try_from(self.start.elapsed().as_micros()).unwrap(),
            "name": name,
            "thread": format!("{:?}", std::thread::current().id()),
            "details": details,
        }));
    }
}

fn capture_probe(audio: &crate::session::test_support::FixtureAudioBackend) -> &'static str {
    let frame = crate::audio::AudioFrame::new(
        crate::domain::audio::AudioSource::Microphone,
        0,
        PcmSpec::pcm16_mono(16_000),
        vec![0; 320],
    )
    .unwrap();
    // This existing collaborator forwards to BoundedAudioQueue::push_sync.
    // One poll observes the actual capture gate without running another executor.
    match audio.send_capture("live-input", frame).now_or_never() {
        Some(Ok(())) => "accepted",
        Some(Err(_)) => "closed",
        None => "pending",
    }
}

async fn observe(action: Action) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let h = crate::app::tests::fixture();
    let app = crate::app::tests::injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let timeline = Timeline::new();
    let (claim_gate_open, claim_gate_watch) = watch::channel(false);
    let (workers_ready_tx, workers_ready) = oneshot::channel();
    let workers_ready_tx = Arc::new(Mutex::new(Some(workers_ready_tx)));
    let worker_tasks = Arc::new(Mutex::new(HashSet::new()));
    *app.jobs.oo1_claim_gate.lock().unwrap() = Some(Arc::new({
        let timeline = timeline.clone();
        let worker_tasks = worker_tasks.clone();
        move || -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
            let timeline = timeline.clone();
            let worker_tasks = worker_tasks.clone();
            let workers_ready_tx = workers_ready_tx.clone();
            let mut open = claim_gate_watch.clone();
            Box::pin(async move {
                let task = format!("{:?}", tokio::task::id());
                let (first, count) = {
                    let mut tasks = worker_tasks.lock().unwrap();
                    let first = tasks.insert(task.clone());
                    (first, tasks.len())
                };
                if first {
                    timeline.record(
                        "actual_worker_received_original_token",
                        json!({
                            "task": task,
                        }),
                    );
                }
                if count == 2 {
                    if let Some(ready) = workers_ready_tx.lock().unwrap().take() {
                        let _ = ready.send(());
                    }
                }
                loop {
                    if *open.borrow_and_update() {
                        break;
                    }
                    if open.changed().await.is_err() {
                        break;
                    }
                }
                timeline.record("pre_claim_rendezvous_opened", json!({"task": task}));
            })
        }
    }));
    app.save_provider_config(SaveProviderRequest {
        config: h.saved.clone(),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    app.start_intelligence().await.unwrap();
    let (native_finished, release_native) = h.audio.delay_actual_join("live-input");
    let session = app.create_session(h.launch.clone()).await.unwrap();
    crate::app::tests::eventually(|| {
        app.store
            .get_session_lifecycle(session.id)
            .unwrap()
            .unwrap()
            .status
            == "running"
    })
    .await;
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(30));

    let workers_prepositioned = matches!(
        tokio::time::timeout(Duration::from_secs(2), workers_ready).await,
        Ok(Ok(()))
    );
    let queued_jobs = app
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT COUNT(*) FROM intelligence_jobs WHERE status='queued'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    timeline.record(
        "ready_running_session",
        json!({"action": action.name(), "queued_jobs": queued_jobs}),
    );
    let held = Arc::new(AtomicBool::new(false));
    let claim_threads = Arc::new(Mutex::new(HashSet::<String>::new()));
    let (claim_tx, claim_rx) = mpsc::channel::<String>();
    *app.jobs.oo1_claim_probe.lock().unwrap() = Some(Arc::new({
        let held = held.clone();
        let timeline = timeline.clone();
        let threads = claim_threads.clone();
        move |phase, admission_closed| {
            let thread = format!("{:?}", std::thread::current().id());
            if phase == "before_sql_lock" && held.load(Ordering::Acquire) {
                let first = threads.lock().unwrap().insert(thread.clone());
                timeline.record(phase, json!({"admission_closed": admission_closed}));
                if first {
                    let _ = claim_tx.send(thread);
                }
            } else if phase == "after_sql_lock" && threads.lock().unwrap().contains(&thread) {
                timeline.record(phase, json!({"admission_closed": admission_closed}));
            }
        }
    }));

    let heartbeat_count = Arc::new(AtomicUsize::new(0));
    let (cancel_heartbeat, mut heartbeat_cancelled) = watch::channel(false);
    let (heartbeat_ready, heartbeat_started) = oneshot::channel();
    let heartbeat = tokio::spawn({
        let timeline = timeline.clone();
        let count = heartbeat_count.clone();
        async move {
            let mut ready = Some(heartbeat_ready);
            let mut interval = tokio::time::interval(Duration::from_millis(25));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    _ = heartbeat_cancelled.changed() => break,
                    _ = interval.tick() => {
                        let count = count.fetch_add(1, Ordering::AcqRel) + 1;
                        timeline.record("heartbeat_tick", json!({"count": count}));
                        if let Some(ready) = ready.take() {
                            let _ = ready.send(());
                        }
                    }
                }
            }
        }
    });
    heartbeat_started.await.unwrap();

    let timer_polled = Arc::new(AtomicBool::new(false));
    let (timer_ready, timer_started) = oneshot::channel();
    let timer = tokio::spawn({
        let timeline = timeline.clone();
        let polled = timer_polled.clone();
        async move {
            let sleep = tokio::time::sleep(Duration::from_millis(100));
            tokio::pin!(sleep);
            let mut ready = Some(timer_ready);
            std::future::poll_fn(|cx| {
                let result = sleep.as_mut().poll(cx);
                if let Some(ready) = ready.take() {
                    timeline.record("timer_first_polled", json!({"deadline_after_ms": 100}));
                    let _ = ready.send(());
                }
                result
            })
            .await;
            polled.store(true, Ordering::Release);
            timeline.record("timer_deadline_polled", json!({}));
        }
    });
    timer_started.await.unwrap();

    let trigger = Arc::new(Notify::new());
    let (action_ready, action_started) = oneshot::channel();
    let action_task = tokio::spawn({
        let app = app.clone();
        let timeline = timeline.clone();
        let trigger = trigger.clone();
        async move {
            let _ = action_ready.send(());
            trigger.notified().await;
            timeline.record("action_first_polled", json!({"action": action.name()}));
            let result = match action {
                Action::OrdinaryStop => app
                    .stop_session(session.id)
                    .await
                    .map(|record| json!({"status": format!("{:?}", record.status)})),
                Action::AppClose => app.close_all().await.map(|_| json!({"closed": true})),
            };
            timeline.record(
                "action_observer_returned",
                json!({
                    "action": action.name(),
                    "ok": result.is_ok(),
                    "error": result.as_ref().err().map(ToString::to_string),
                }),
            );
            result
        }
    });
    action_started.await.unwrap();

    let (blocker_done, blocker_returned) = oneshot::channel();
    let blocker = std::thread::spawn({
        let store = app.store.clone();
        let jobs = app.jobs.clone();
        let app = app.clone();
        let audio = h.audio.clone();
        let timeline = timeline.clone();
        let held = held.clone();
        let heartbeat_count = heartbeat_count.clone();
        let timer_polled = timer_polled.clone();
        move || {
            let result = store.transaction(|_| {
                held.store(true, Ordering::Release);
                timeline.record("sqlite_hold_entered", json!({}));
                // Both genuine token-owning worker futures are already waiting.
                // Opening from this OS thread makes both continuations runnable;
                // the following claim/SQLite acquisition is unchanged and synchronous.
                claim_gate_open.send_replace(true);
                jobs.wake();
                let claims_deadline = Instant::now() + Duration::from_secs(2);
                let mut threads = HashSet::new();
                while threads.len() < 2 {
                    let remaining = claims_deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    match claim_rx.recv_timeout(remaining) {
                        Ok(thread) => {
                            threads.insert(thread);
                        }
                        Err(_) => break,
                    }
                }
                let heartbeat_at_claims = heartbeat_count.load(Ordering::Acquire);
                let timer_at_claims = timer_polled.load(Ordering::Acquire);
                timeline.record("actual_claim_entries_observed", json!({
                    "distinct_threads": threads,
                    "heartbeat_count": heartbeat_at_claims,
                    "timer_already_polled": timer_at_claims,
                }));
                timeline.record("action_requested", json!({"action": action.name()}));
                trigger.notify_one();
                let observation_start = Instant::now();
                let mut samples = Vec::new();
                for at_ms in [200, 450, 700] {
                    let due = observation_start + Duration::from_millis(at_ms);
                    loop {
                        let remaining = due.saturating_duration_since(Instant::now());
                        if remaining.is_zero() { break; }
                        std::thread::sleep(remaining);
                    }
                    let sample = json!({
                        "window_ms": at_ms,
                        "heartbeat_count": heartbeat_count.load(Ordering::Acquire),
                        "timer_polled": timer_polled.load(Ordering::Acquire),
                    });
                    timeline.record("held_sql_progress_sample", sample.clone());
                    samples.push(sample);
                }
                let release_due = observation_start + Duration::from_millis(750);
                loop {
                    let remaining = release_due.saturating_duration_since(Instant::now());
                    if remaining.is_zero() { break; }
                    std::thread::sleep(remaining);
                }
                let heartbeat_before_release = heartbeat_count.load(Ordering::Acquire);
                let timer_before_release = timer_polled.load(Ordering::Acquire);
                let capture_before_release = capture_probe(&audio);
                let admission_fenced = app.admitted_epoch().is_err();
                let observation = json!({
                    "distinct_actual_claim_threads": threads,
                    "two_actual_claim_entries": threads.len() == 2,
                    "timer_already_polled_at_claim_entries": timer_at_claims,
                    "heartbeat_count_at_claim_entries": heartbeat_at_claims,
                    "heartbeat_count_before_release": heartbeat_before_release,
                    "heartbeat_progress_before_release": heartbeat_before_release > heartbeat_at_claims,
                    "timer_polled_before_release": timer_before_release,
                    "capture_probe_before_release": capture_before_release,
                    "app_admission_fenced_before_release": admission_fenced,
                    "external_hold_after_claim_entries_ms": 750,
                    "progress_windows": samples,
                });
                timeline.record("sqlite_release_requested", observation.clone());
                held.store(false, Ordering::Release);
                Ok(observation)
            });
            timeline.record("sqlite_transaction_returned", json!({"ok": result.is_ok()}));
            let _ = blocker_done.send(());
            result
        }
    });
    // No assertion during the held period: the independent thread always releases.
    let blocker_notified = blocker_returned.await.is_ok();
    let hold_result = blocker.join();
    timeline.record(
        "sqlite_holder_thread_joined",
        json!({"ok": hold_result.is_ok()}),
    );
    let hold = hold_result.unwrap().unwrap();
    let action_result = action_task.await.unwrap();
    let native_worker_finished = matches!(
        tokio::time::timeout(Duration::from_secs(2), native_finished).await,
        Ok(Ok(()))
    );
    timeline.record(
        "native_workers_joined_parent_still_held",
        json!({
            "observed": native_worker_finished,
        }),
    );

    let first_close_timed_out = match action {
        Action::OrdinaryStop => {
            timeline.record("cleanup_close_first_polled", json!({}));
            app.close_all().await.is_err()
        }
        Action::AppClose => action_result.is_err(),
    };
    // The close observer can now time out during the hold, before the session
    // observer's yielding terminal write. Native leaf completion alone is not
    // durable lifecycle completion. Observe that original stop owner as well,
    // still retaining its native parent, before reading the durable status.
    let terminal_complete_observed = matches!(
        tokio::time::timeout(Duration::from_secs(2), app.stop_session(session.id)).await,
        Ok(Ok(record)) if record.status == crate::session::SessionStatus::Completed
    );
    timeline.record(
        "original_terminal_completion_observed",
        json!({
            "completed": terminal_complete_observed,
        }),
    );
    let open_fenced = app.open_window().is_err();
    let reopen_fenced = app.reopen_window().await.is_err();
    let durable_status = app
        .store
        .get_session_lifecycle(session.id)
        .unwrap()
        .unwrap()
        .status;
    let capture_after_stop = capture_probe(&h.audio);
    timeline.record(
        "terminal_and_reopen_observed_with_original_parent_held",
        json!({
            "durable_status": durable_status,
            "capture_probe": capture_after_stop,
            "close_observer_timed_out": first_close_timed_out,
            "open_fenced": open_fenced,
            "reopen_fenced": reopen_fenced,
        }),
    );
    timeline.record("actual_native_parent_release_sent", json!({}));
    let native_release_sent = release_native.send(()).is_ok();
    app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_secs(2));
    let close_joined = app.close_all().await.is_ok();
    let jobs_joined = app.jobs.shutdown().await.is_ok();
    let summary_attempts_joined = app.summaries.drain_actual().await.is_ok();
    timeline.record(
        "actual_app_close_and_scheduler_joins_returned",
        json!({
            "close_joined": close_joined,
            "jobs_joined": jobs_joined,
            "summary_attempts_joined": summary_attempts_joined,
        }),
    );
    *app.jobs.oo1_claim_probe.lock().unwrap() = None;
    *app.jobs.oo1_claim_gate.lock().unwrap() = None;
    cancel_heartbeat.send_replace(true);
    let heartbeat_joined = heartbeat.await.is_ok();
    let timer_joined = timer.await.is_ok();
    timeline.record(
        "all_fixture_tasks_joined",
        json!({
            "heartbeat_joined": heartbeat_joined,
            "timer_joined": timer_joined,
        }),
    );
    drop(app);
    json!({
        "action": action.name(),
        "both_original_token_workers_prepositioned": workers_prepositioned,
        "queued_jobs_before_hold": queued_jobs,
        "hold": hold,
        "cleanup": {
            "blocker_notified": blocker_notified,
            "native_worker_finished": native_worker_finished,
            "first_close_observer_timed_out_with_actual_parent_held": first_close_timed_out,
            "original_terminal_completion_observed": terminal_complete_observed,
            "open_fenced_until_actual_parent_release": open_fenced,
            "reopen_fenced_until_actual_parent_release": reopen_fenced,
            "durable_status_after_sqlite_release": durable_status,
            "capture_probe_after_stop": capture_after_stop,
            "native_release_sent": native_release_sent,
            "actual_close_joined": close_joined,
            "both_actual_scheduler_workers_joined": jobs_joined,
            "summary_attempts_joined": summary_attempts_joined,
            "heartbeat_joined": heartbeat_joined,
            "timer_joined": timer_joined,
        },
        "timeline": timeline.events.lock().unwrap().clone(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_scheduler_keeps_timer_and_capture_stop_pollable_while_sqlite_claims_wait() {
    let ordinary = observe(Action::OrdinaryStop).await;
    let close = observe(Action::AppClose).await;
    let observations = vec![ordinary, close];
    println!(
        "OO1_OBSERVATIONS {}",
        json!({
            "configured_tokio_workers": 2,
            "host_available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
            "observations": observations,
        })
    );
    // Validate the fault and every genuine cleanup before evaluating progress.
    assert!(
        observations.iter().all(|observation| {
            let hold = &observation["hold"];
            let cleanup = &observation["cleanup"];
            hold["two_actual_claim_entries"] == true
                && observation["both_original_token_workers_prepositioned"] == true
                && observation["queued_jobs_before_hold"] == 0
                && hold["timer_already_polled_at_claim_entries"] == false
                && cleanup["blocker_notified"] == true
                && cleanup["native_worker_finished"] == true
                && cleanup["first_close_observer_timed_out_with_actual_parent_held"] == true
                && cleanup["original_terminal_completion_observed"] == true
                && cleanup["open_fenced_until_actual_parent_release"] == true
                && cleanup["reopen_fenced_until_actual_parent_release"] == true
                && cleanup["durable_status_after_sqlite_release"] == "completed"
                && cleanup["capture_probe_after_stop"] == "closed"
                && cleanup["native_release_sent"] == true
                && cleanup["actual_close_joined"] == true
                && cleanup["both_actual_scheduler_workers_joined"] == true
                && cleanup["summary_attempts_joined"] == true
                && cleanup["heartbeat_joined"] == true
                && cleanup["timer_joined"] == true
        }),
        "OO1 fixture entry/release/actual cleanup evidence is incomplete"
    );
    assert!(
        observations.iter().all(|observation| {
            let hold = &observation["hold"];
            let samples = hold["progress_windows"].as_array().unwrap();
            let counts: Vec<u64> = samples
                .iter()
                .map(|sample| sample["heartbeat_count"].as_u64().unwrap())
                .collect();
            hold["heartbeat_progress_before_release"] == true
                && hold["timer_polled_before_release"] == true
                && hold["capture_probe_before_release"] == "closed"
                && counts.len() == 3
                && counts[0] > hold["heartbeat_count_at_claim_entries"].as_u64().unwrap()
                && counts.windows(2).all(|pair| pair[1] > pair[0])
                && samples[0]["timer_polled"] == true
        }),
        "OO1 availability regression: both actual scheduler SQLite claims prevented timer/capture-stop progress until independent release"
    );
}
