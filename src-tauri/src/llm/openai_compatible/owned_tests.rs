use super::*;
use crate::{
    app::CredentialChange,
    config::{ConfigIo, ConfigStore, KeyBackend, SecretStore},
    intelligence::{
        jobs::OperationLease,
        test_support::{RecordingHttp, test_llm_profile},
        types::SaveModelProfile,
    },
    storage::LocalStore,
};
use fs2::FileExt;
use std::{
    fs::File,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
struct EnteredKeyBackend {
    inner: Arc<dyn KeyBackend>,
    block: AtomicBool,
    entered: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}
impl KeyBackend for EnteredKeyBackend {
    fn load_or_create(&self, path: &Path) -> crate::error::AppResult<Vec<u8>> {
        if self.block.swap(false, Ordering::SeqCst) {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                let _ = entered.send(());
            }
            if let Some(release) = self.release.lock().unwrap().take() {
                let _ = release.recv();
            }
        }
        self.inner.load_or_create(path)
    }
}
struct Owner {
    _process: File,
    _slot: tokio::sync::OwnedSemaphorePermit,
}
#[tokio::test]
async fn llm_entered_resolver_timeout_keeps_combined_request_and_process_slots_until_actual_key_release()
 {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let backend = Arc::new(EnteredKeyBackend {
        inner: SecretStore::test_backend(),
        block: AtomicBool::new(false),
        entered: Mutex::new(Some(entered)),
        release: Mutex::new(Some(gate)),
    });
    let secrets = Arc::new(SecretStore::with_test_backend(dir.path(), backend.clone()).unwrap());
    let io = ConfigIo::with_runtime_for_tests(
        ConfigStore::new(dir.path(), secrets).unwrap(),
        Arc::new(tokio::sync::Semaphore::new(2)),
        Duration::from_millis(100),
    );
    let config = Arc::new(ServiceConfigStore::new(io, store));
    let profile = test_llm_profile();
    config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-entered-resolver".into(),
            },
        })
        .await
        .unwrap();
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dir.path().join("actual-process.lock"))
        .unwrap();
    file.try_lock_exclusive().unwrap();
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let owner: OperationLease = Arc::new(Owner {
        _process: file,
        _slot: slots.clone().try_acquire_owned().unwrap(),
    });
    let weak = Arc::downgrade(&owner);
    let http = Arc::new(RecordingHttp::default());
    let provider = Arc::new(OpenAiCompatibleLlm::new(config, http.clone()));
    backend.block.store(true, Ordering::SeqCst);
    let entered_provider = provider.clone();
    let (_keep, cancel) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move {
        entered_provider
            .complete(
                &profile,
                LlmRequest {
                    system: "strict JSON".into(),
                    user: "quoted input".into(),
                    max_output_tokens: 512,
                },
                LlmExecution::new(owner, cancel),
            )
            .await
    });
    let entered = tokio::time::timeout(Duration::from_secs(2), entry).await;
    if !matches!(entered, Ok(Ok(()))) {
        drop(release);
        let _ = task.await;
        assert!(false, "actual credential resolver never entered");
        return;
    }
    let result = task.await.unwrap();
    let held = weak.upgrade().is_some();
    let request_slots = provider.requests.available_permits();
    let process_slots = slots.available_permits();
    drop(release);
    tokio::time::timeout(Duration::from_secs(2), async {
        while slots.available_permits() != 16 || provider.requests.available_permits() != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(result.err(), Some(Code::Configuration));
    assert!(held);
    assert_eq!(request_slots, 3);
    assert_eq!(process_slots, 15);
    assert!(http.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn llm_real_summary_terminal_after_entered_resolver_timeout_cannot_reap_or_delete_before_actual_key_release()
 {
    use crate::{
        intelligence::{jobs::JobScheduler, types::*},
        storage::{StoredSession, StoredTranscriptSegment},
        summary::SummaryService,
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalStore::open(dir.path()).unwrap());
    let (entered, entry) = tokio::sync::oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel();
    let backend = Arc::new(EnteredKeyBackend {
        inner: SecretStore::test_backend(),
        block: AtomicBool::new(false),
        entered: Mutex::new(Some(entered)),
        release: Mutex::new(Some(gate)),
    });
    let secrets = Arc::new(SecretStore::with_test_backend(dir.path(), backend.clone()).unwrap());
    let io = ConfigIo::with_runtime_for_tests(
        ConfigStore::new(dir.path(), secrets).unwrap(),
        Arc::new(tokio::sync::Semaphore::new(2)),
        Duration::from_millis(100),
    );
    let config = Arc::new(ServiceConfigStore::new(io, store.clone()));
    let profile = test_llm_profile();
    config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-summary-resolver".into(),
            },
        })
        .await
        .unwrap();
    config
        .save_settings(IntelligenceSettings {
            summary_profile_id: Some(profile.id),
            ..Default::default()
        })
        .await
        .unwrap();
    let file = File::options()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dir.path().join("actual-summary-process.lock"))
        .unwrap();
    file.try_lock_exclusive().unwrap();
    let owner: OperationLease = Arc::new(file);
    let http = Arc::new(RecordingHttp::default());
    let llm = Arc::new(OpenAiCompatibleLlm::new(config.clone(), http.clone()));
    let jobs = Arc::new(JobScheduler::new(store.clone()));
    let summaries = SummaryService::new(store.clone(), config, jobs.clone(), llm.clone());
    summaries.register_handler().unwrap();
    jobs.reconcile().unwrap();
    summaries
        .reconcile_local_owned(owner.clone(), 0, jobs.reap_port())
        .await
        .unwrap();
    jobs.start_owned(owner.clone()).unwrap();
    let session = uuid::Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id: session,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    store
        .append_final_segment(&StoredTranscriptSegment {
            id: uuid::Uuid::new_v4(),
            session_id: session,
            direction: "source_to_target".into(),
            sequence: 0,
            occurred_at: 1,
            language: "zh".into(),
            text: "entered resolver input".into(),
            is_final: true,
            details: None,
            occurred_at_ms: None,
        })
        .unwrap();
    backend.block.store(true, Ordering::SeqCst);
    let job = crate::summary::readiness_test_support::generate_local_owned(
        &summaries,
        session,
        None,
        owner.clone(),
    )
    .await
    .unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(2), entry).await;
    if !matches!(observed, Ok(Ok(()))) {
        drop(release);
        let _ = jobs.shutdown().await;
        panic!("real summary credential resolver never entered");
    }
    let terminal = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let row = store.get_job(job.id).unwrap().unwrap();
            if row.status == JobStatus::Failed {
                break row;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if terminal.is_err() {
        drop(release);
        let _ = jobs.shutdown().await;
        panic!("real summary worker did not publish the resolver timeout failure");
    }
    let terminal = terminal.unwrap();
    let unproved = store
        .transaction(|tx| jobs.reap_port().prove(tx, job.id).map(|p| p.is_none()))
        .unwrap();
    let retained_requests = llm.requests.available_permits();
    let mut deletion = Box::pin(crate::summary::readiness_test_support::delete_local_owned(
        &summaries,
        job.object_id.unwrap(),
        owner,
    ));
    let pending = tokio::time::timeout(Duration::from_millis(50), deletion.as_mut())
        .await
        .is_err();
    drop(release);
    let deleted = if pending {
        tokio::time::timeout(Duration::from_secs(2), deletion)
            .await
            .unwrap()
    } else {
        Err(crate::error::AppError::Intelligence(Code::Interrupted))
    };
    jobs.shutdown().await.unwrap();
    assert_eq!(terminal.error, Some(Code::Configuration));
    assert!(unproved && pending);
    assert_eq!(retained_requests, 3);
    assert!(deleted.is_ok());
    assert_eq!(llm.requests.available_permits(), 4);
    assert_eq!(store.count_summaries(session).unwrap(), 0);
    assert!(http.requests.lock().unwrap().is_empty());
}
