use super::*;
use crate::intelligence::{
    jobs::{JobContext, JobHandler},
    test_support::payload,
};
use std::time::Duration;
struct Wait;
#[async_trait::async_trait]
impl JobHandler for Wait {
    async fn run(
        &self,
        mut c: JobContext,
        _: serde_json::Value,
    ) -> Result<(), IntelligenceErrorCode> {
        crate::intelligence::http::cancelled(&mut c.cancel).await;
        Err(IntelligenceErrorCode::Cancelled)
    }
}
#[test]
fn intelligence_app_inert_construction_projects_empty_library_directory_and_recovers_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(dir.path()).unwrap();
    let id = Uuid::new_v4();
    let job = store
        .transaction(|tx| {
            crate::intelligence::store::admit_job(
                tx,
                crate::intelligence::store::JobDraft {
                    id: Uuid::new_v4(),
                    kind: JobKind::Import,
                    session_id: None,
                    object_id: Some(id),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{id}"),
                    },
                    payload: payload(vec![]),
                    initial_failure: None,
                },
            )
        })
        .unwrap()
        .view()
        .clone();
    drop(store);
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(app.get_managed_data_location().data_dir, dir.path());
    assert_eq!(
        app.store.get_job(job.id).unwrap().unwrap().status,
        JobStatus::Interrupted
    );
    assert!(!dir.path().join("secrets.dek.lock").exists());
}
#[tokio::test]
async fn intelligence_app_close_cancels_jobs_and_restarts_only_on_explicit_host_start() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    app.jobs.register(JobKind::Reindex, Arc::new(Wait)).unwrap();
    app.start_intelligence().await.unwrap();
    let id = Uuid::new_v4();
    let job = app
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
    app.close_all().await.unwrap();
    assert!(matches!(
        app.store.get_job(job.id).unwrap().unwrap().status,
        JobStatus::Cancelled
    ));
    assert!(app.start_intelligence().await.is_err());
    app.reopen_window().await.unwrap();
    app.close_all().await.unwrap();
    drop(app);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if AppState::new(dir.path().to_path_buf()).is_ok() {
                break;
            }
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn intelligence_app_entered_clear_retains_process_and_close_barrier_after_observer_loss() {
    use fs2::FileExt;
    for abort in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let h = crate::app::tests::fixture();
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        let mut app = crate::app::tests::injected(dir.path(), &h, secrets.clone());
        let io = ConfigIo::with_runtime_for_tests(
            ConfigStore::new(dir.path(), secrets.clone()).unwrap(),
            Arc::new(Semaphore::new(2)),
            Duration::from_millis(150),
        );
        app.service_config = Arc::new(crate::intelligence::config::ServiceConfigStore::new(
            io,
            app.store.clone(),
        ));
        app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(30));
        let profile = crate::intelligence::test_support::test_llm_profile();
        app.save_model_profile(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-clear".into(),
            },
        })
        .await
        .unwrap();
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.path().join("secrets.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();
        let clone = app.clone();
        let task = tokio::spawn(async move {
            clone
                .save_model_profile(SaveModelProfile {
                    profile,
                    credential: CredentialChange::Clear,
                })
                .await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let data =
                    std::fs::read_to_string(dir.path().join("service-configs.json")).unwrap();
                if data.contains("\"credential_reference\": null") {
                    break;
                }
                tokio::task::yield_now().await
            }
        })
        .await
        .unwrap();
        if abort {
            task.abort();
            let _ = task.await;
        } else {
            assert!(task.await.unwrap().is_err());
        }
        assert!(app.close_all().await.is_err());
        assert!(app.open_window().is_err());
        drop(app);
        let retained = AppState::new(dir.path().to_path_buf()).is_err();
        lock.unlock().unwrap();
        assert!(retained);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if AppState::new(dir.path().to_path_buf()).is_ok() {
                    break;
                }
                tokio::task::yield_now().await
            }
        })
        .await
        .unwrap();
        assert_eq!(h.audio.open_calls(), 0);
    }
}
#[tokio::test]
async fn intelligence_app_dropping_external_handles_releases_process_after_worker_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppState::new(dir.path().to_path_buf()).unwrap();
    app.jobs.register(JobKind::Reindex, Arc::new(Wait)).unwrap();
    app.start_intelligence().await.unwrap();
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
    drop(app);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if AppState::new(dir.path().to_path_buf()).is_ok() {
                break;
            }
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn intelligence_close_stops_capture_while_snapshot_sqlite_is_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let h = crate::app::tests::fixture();
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
    app.start_intelligence().await.unwrap();
    let _session = app.create_session(h.launch.clone()).await.unwrap();
    crate::app::tests::eventually(|| h.audio.open_ports() > 0).await;
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let config = app.service_config.clone();
    let blocker = tokio::spawn(async move {
        config
            .with_profile_snapshot(vec![], move |_, _| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .await
    });
    entered_rx.await.unwrap();
    let owner = app.clone();
    let close = tokio::spawn(async move { owner.close_all().await });
    let audio = h.audio.clone();
    let (check_tx, check_rx) = tokio::sync::oneshot::channel();
    let check = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let stopped = runtime
            .block_on(
                audio.send_capture(
                    "live-input",
                    crate::audio::AudioFrame::new(
                        crate::domain::audio::AudioSource::Microphone,
                        0,
                        PcmSpec::pcm16_mono(16000),
                        vec![0; 320],
                    )
                    .unwrap(),
                ),
            )
            .is_err();
        release_tx.send(()).unwrap();
        check_tx.send(stopped).unwrap();
    });
    let stopped = check_rx.await.unwrap();
    check.join().unwrap();
    blocker.await.unwrap().unwrap();
    close.await.unwrap().unwrap();
    assert!(
        stopped,
        "close waited for SQLite before cancelling active capture"
    );
}
