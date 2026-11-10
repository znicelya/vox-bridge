//! Runs the actual terminal observer, store, selected config and scheduler.
use super::harness::RecordingLlm;
use crate::{
    app::CredentialChange,
    audio::{PcmSpec, runtime::AudioBackend},
    config::{ConfigIo, ConfigStore, SecretStore},
    intelligence::{
        config::ServiceConfigStore,
        jobs::{JobScheduler, OperationLease},
        types::*,
    },
    providers::{
        ProviderEvent,
        fixture::{FixtureScript, FixtureStep},
    },
    session::{
        SessionOrchestrator, SessionStatus,
        test_support::{RuntimeHarness, final_text, metadata},
    },
    summary::SummaryService,
};
use fs2::FileExt;
use std::{fs::File, sync::Arc, time::Duration};

struct RuntimeSummaryHarness {
    runtime: RuntimeHarness,
    orchestrator: SessionOrchestrator,
    service: SummaryService,
    delivery: crate::summary::SummaryDeliveryOwner,
    jobs: Arc<JobScheduler>,
    llm: Arc<RecordingLlm>,
    _process: OperationLease,
    _dir: tempfile::TempDir,
}
impl RuntimeSummaryHarness {
    async fn new() -> Self {
        let script = FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::Emit(ProviderEvent::TranslationFinal {
                    metadata: metadata(),
                    text: final_text("zh", "客户询问24V泵"),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap();
        let runtime = RuntimeHarness::live(script).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.path().join("automatic-process.lock"))
            .unwrap();
        file.try_lock_exclusive().unwrap();
        let process: OperationLease = Arc::new(file);
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        let io = ConfigIo::with_runtime_for_tests(
            ConfigStore::new(dir.path(), secrets).unwrap(),
            Arc::new(tokio::sync::Semaphore::new(2)),
            Duration::from_secs(10),
        );
        let config = Arc::new(ServiceConfigStore::new(io, runtime.store.clone()));
        let profile = crate::intelligence::test_support::test_llm_profile();
        config
            .save(SaveModelProfile {
                profile: profile.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-automatic-key".into(),
                },
            })
            .await
            .unwrap();
        config
            .save_settings(IntelligenceSettings {
                automatic_summary: true,
                summary_profile_id: Some(profile.id),
                ..Default::default()
            })
            .await
            .unwrap();
        let llm = Arc::new(RecordingLlm::default());
        let jobs = Arc::new(JobScheduler::new(runtime.store.clone()));
        let service = SummaryService::new(runtime.store.clone(), config, jobs.clone(), llm.clone());
        service.register_handler().unwrap();
        jobs.reconcile().unwrap();
        let local = service
            .reconcile_local_owned(process.clone(), 0, jobs.reap_port())
            .await
            .unwrap();
        let delivery = service.delivery_owner();
        let ready = delivery
            .start_owned(process.clone(), 0, local)
            .await
            .unwrap();
        let orchestrator = runtime.orchestrator();
        orchestrator.install_terminal_port(ready.terminal_port());
        Self {
            runtime,
            orchestrator,
            service,
            delivery,
            jobs,
            llm,
            _process: process,
            _dir: dir,
        }
    }
    async fn shutdown(&self) {
        let key = self
            .delivery
            .elect_closing(1, self.runtime.audio.actual_epoch().unwrap())
            .unwrap();
        let backend = self.runtime.audio.begin_close(key.clone()).unwrap();
        let session = self.orchestrator.capture_close_join(key.clone()).unwrap();
        let orchestrator = self.orchestrator.clone();
        let audio = crate::audio::join::AudioCloseJoin::start_owned(
            self._process.clone(),
            key.clone(),
            session,
            backend,
            async move { orchestrator.stop_active().await },
        )
        .unwrap();
        self.jobs.signal_shutdown();
        let (jobs, delivery) = tokio::join!(
            self.jobs.shutdown(),
            self.delivery
                .close_join_owned(self._process.clone(), key, audio),
        );
        jobs.unwrap();
        delivery.unwrap();
        self.service.signal_closing().unwrap();
        self.service.drain_actual().await.unwrap();
    }
}

// Removing the committed terminal notice or delaying stop on LLM completion
// must break this test. All assertions observe the real durable/runtime paths.
#[tokio::test]
async fn audio_stop_completion_precedes_delayed_automatic_llm() {
    let h = RuntimeSummaryHarness::new().await;
    h.llm.block.store(true, std::sync::atomic::Ordering::SeqCst);
    let session = h
        .orchestrator
        .start(h.runtime.launch.clone(), h.runtime.saved.clone())
        .await
        .unwrap()
        .id;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if h.runtime.store.count_segments(session).unwrap() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the real runtime must commit its final transcript");
    let stopped =
        tokio::time::timeout(Duration::from_secs(2), h.orchestrator.stop_session(session)).await;
    let entered = tokio::time::timeout(Duration::from_secs(2), h.llm.entered.notified()).await;
    let rows = h.service.list(session, None, 100).unwrap().rows;
    let durable = h
        .runtime
        .store
        .get_session_lifecycle(session)
        .unwrap()
        .unwrap();
    let active = h.orchestrator.has_active_operation();
    h.llm.release.notify_one();
    h.shutdown().await;
    assert_eq!(
        stopped
            .expect("stop waited for held automatic LLM")
            .unwrap()
            .status,
        SessionStatus::Completed
    );
    assert_eq!(durable.status, "completed");
    assert!(!active, "terminal notification must follow active release");
    assert!(
        entered.is_ok(),
        "committed terminal must deliver one automatic summary through the real scheduler"
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(h.llm.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn summary_automatic_terminal_commit_survives_later_diagnostic_failure() {
    let h = RuntimeSummaryHarness::new().await;
    let session = h
        .orchestrator
        .start(h.runtime.launch.clone(), h.runtime.saved.clone())
        .await
        .unwrap()
        .id;
    tokio::time::timeout(Duration::from_secs(2), async {
        while h.runtime.store.count_segments(session).unwrap() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.runtime.store.transaction(|tx| {
        tx.execute_batch("CREATE TABLE terminal_update_witness(status TEXT,pending INTEGER,generation TEXT,error TEXT); CREATE TRIGGER witness_terminal_update AFTER UPDATE ON session_lifecycle WHEN NEW.status IN ('completed','failed') AND OLD.status NOT IN ('completed','failed') BEGIN INSERT INTO terminal_update_witness VALUES(NEW.status,NEW.summary_terminal_pending,NEW.summary_terminal_generation,NEW.summary_terminal_error); END; CREATE TRIGGER reject_committed_terminal_diagnostic BEFORE INSERT ON session_diagnostics WHEN EXISTS(SELECT 1 FROM session_lifecycle WHERE session_id=NEW.session_id AND status IN ('completed','failed')) BEGIN SELECT RAISE(FAIL,'injected post-terminal diagnostic failure'); END;")?;
        Ok(())
    }).unwrap();
    h.runtime.fail_final_capture_accounting_writes();
    let stopped = h.orchestrator.stop_session(session).await;
    let pending = h
        .runtime
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT status,pending,generation,error FROM terminal_update_witness",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )?)
        })
        .unwrap();
    let entered = tokio::time::timeout(Duration::from_secs(2), h.llm.entered.notified()).await;
    h.llm.release.notify_one();
    h.shutdown().await;
    assert!(
        stopped.is_err(),
        "the post-commit diagnostic error must stay visible"
    );
    assert_eq!(pending.0, "failed");
    assert_eq!(
        pending.1, 1,
        "a successful lifecycle UPDATE must atomically retain its terminal journal"
    );
    assert!(pending.2.is_some());
    assert!(
        entered.is_ok(),
        "a later diagnostic error must not erase the committed automatic intent"
    );
    assert_eq!(h.llm.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn summary_automatic_failed_lifecycle_commit_sends_no_terminal_notice_or_rescan() {
    let h = RuntimeSummaryHarness::new().await;
    let (terminal, mut notices, rescan) = h.delivery.observed_terminal_for_tests().unwrap();
    h.orchestrator.install_terminal_port(terminal);
    let session = h
        .orchestrator
        .start(h.runtime.launch.clone(), h.runtime.saved.clone())
        .await
        .unwrap()
        .id;
    tokio::time::timeout(Duration::from_secs(2), async {
        while h.runtime.store.count_segments(session).unwrap() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    h.runtime.store.transaction(|tx| {
        tx.execute_batch("CREATE TRIGGER reject_terminal_lifecycle BEFORE UPDATE ON session_lifecycle WHEN NEW.status IN ('completed','failed') AND OLD.status NOT IN ('completed','failed') BEGIN SELECT RAISE(FAIL,'injected terminal lifecycle failure'); END;")?;
        Ok(())
    }).unwrap();
    let stopped = h.orchestrator.stop_session(session).await;
    let notice = tokio::time::timeout(Duration::from_millis(250), notices.recv()).await;
    let wake = tokio::time::timeout(Duration::from_millis(20), rescan.notified()).await;
    let journal = h.runtime.store.transaction(|tx| Ok(tx.query_row(
        "SELECT status,summary_terminal_pending,summary_terminal_generation,summary_terminal_error FROM session_lifecycle WHERE session_id=?1", [session.to_string()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?)),
    )?)).unwrap();
    h.runtime
        .store
        .transaction(|tx| {
            tx.execute_batch("DROP TRIGGER reject_terminal_lifecycle")?;
            Ok(())
        })
        .unwrap();
    h.shutdown().await;
    assert!(stopped.is_err());
    assert!(
        notice.is_err() && wake.is_err(),
        "a failed lifecycle UPDATE must drop its owned permit without notifying delivery"
    );
    assert!(!matches!(journal.0.as_str(), "completed" | "failed"));
    assert_eq!((journal.1, journal.2, journal.3), (0, None, None));
    assert!(h.service.list(session, None, 100).unwrap().rows.is_empty());
    assert!(h.llm.calls.lock().unwrap().is_empty());
}
