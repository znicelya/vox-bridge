use super::super::{
    SummaryJobRequest, SummaryService,
    types::{NativeSummaryView, SummaryView},
};
use crate::summary::readiness_test_support::delete_local_owned;
pub(in crate::summary) use crate::summary::readiness_test_support::generate_local_owned;
use crate::{
    app::CredentialChange,
    config::{ConfigIo, ConfigStore, SecretStore},
    error::AppResult,
    intelligence::{
        config::ServiceConfigStore,
        jobs::{JobScheduler, OperationLease},
        types::*,
    },
    llm::{LlmExecution, LlmProvider, LlmRequest, LlmResponse},
    storage::{LocalStore, StoredSession, StoredSessionLifecycle, StoredTranscriptSegment},
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::File,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use uuid::Uuid;

pub(super) const VALID: &str = r#"{"summary":"24V需求","participants":[],"companies":[],"requirements":["24V"],"action_items":[],"risks":[],"terms":[]}"#;

#[derive(Default)]
pub(in crate::summary) struct RecordingLlm {
    responses: Mutex<VecDeque<String>>,
    pub calls: Mutex<Vec<(ModelProfile, LlmRequest)>>,
    pub block: AtomicBool,
    pub entered: Notify,
    pub release: Notify,
}
impl RecordingLlm {
    pub fn push(&self, response: &str) {
        self.responses.lock().unwrap().push_back(response.into());
    }
}
#[async_trait::async_trait]
impl LlmProvider for RecordingLlm {
    async fn complete(
        &self,
        profile: &ModelProfile,
        request: LlmRequest,
        execution: LlmExecution,
    ) -> Result<LlmResponse, IntelligenceErrorCode> {
        // The external completion double retains the complete real job-child scope.
        let _execution = execution;
        self.calls.lock().unwrap().push((profile.clone(), request));
        self.entered.notify_one();
        if self.block.load(Ordering::SeqCst) {
            self.release.notified().await;
        }
        let content = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| VALID.into());
        let response_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        Ok(LlmResponse {
            content,
            response_hash,
        })
    }
}

pub(in crate::summary) struct SummaryHarness {
    pub store: Arc<LocalStore>,
    pub config: Arc<ServiceConfigStore>,
    pub io: ConfigIo,
    pub jobs: Arc<JobScheduler>,
    pub service: SummaryService,
    pub llm: Arc<RecordingLlm>,
    pub profile: ModelProfile,
    pub process: OperationLease,
    pub dir: tempfile::TempDir,
}
impl SummaryHarness {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let process = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.path().join("harness-process.lock"))
            .unwrap();
        process.try_lock_exclusive().unwrap();
        let process: OperationLease = Arc::new(process);
        let store = Arc::new(LocalStore::open(dir.path()).unwrap());
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        let io = ConfigIo::with_runtime_for_tests(
            ConfigStore::new(dir.path(), secrets).unwrap(),
            Arc::new(tokio::sync::Semaphore::new(2)),
            Duration::from_secs(10),
        );
        let config = Arc::new(ServiceConfigStore::new(io.clone(), store.clone()));
        let profile = crate::intelligence::test_support::test_llm_profile();
        config
            .save(SaveModelProfile {
                profile: profile.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-summary-key".into(),
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
        let llm = Arc::new(RecordingLlm::default());
        let jobs = Arc::new(JobScheduler::new(store.clone()));
        let service = SummaryService::new(store.clone(), config.clone(), jobs.clone(), llm.clone());
        service.register_handler().unwrap();
        jobs.reconcile().unwrap();
        service
            .reconcile_local_owned(process.clone(), 0, jobs.reap_port())
            .await
            .unwrap();
        jobs.start_owned(process.clone()).unwrap();
        Self {
            store,
            config,
            io,
            jobs,
            service,
            llm,
            profile,
            process,
            dir,
        }
    }
    pub async fn completed_session(&self, finals: &[&str]) -> Uuid {
        let id = Uuid::new_v4();
        self.store
            .create_session(&StoredSession {
                id,
                started_at: 17,
                mode: "live".into(),
                direction: "source_to_target".into(),
                source_language: "en".into(),
                target_language: "zh".into(),
            })
            .unwrap();
        for (sequence, text) in finals.iter().enumerate() {
            self.insert_final(id, "source_to_target", sequence as i64, text);
        }
        self.store
            .update_session_lifecycle(&StoredSessionLifecycle {
                session_id: id,
                status: "completed".into(),
                ended_at: Some(23),
            })
            .unwrap();
        id
    }
    pub fn insert_final(&self, session: Uuid, direction: &str, sequence: i64, text: &str) -> Uuid {
        let id = Uuid::new_v4();
        self.store
            .append_final_segment(&StoredTranscriptSegment {
                id,
                session_id: session,
                direction: direction.into(),
                sequence,
                occurred_at: 19,
                language: "zh".into(),
                text: text.into(),
                is_final: true,
                details: None,
                occurred_at_ms: None,
            })
            .unwrap();
        id
    }
    pub async fn generate(&self, session: Uuid, profile: Option<Uuid>) -> AppResult<JobView> {
        generate_local_owned(&self.service, session, profile, self.process.clone()).await
    }
    pub async fn run_jobs(&self, id: Uuid) -> JobView {
        self.wait_with_timeout(id, Duration::from_secs(5)).await
    }
    pub async fn wait_with_timeout(&self, id: Uuid, timeout: Duration) -> JobView {
        self.jobs.wake();
        tokio::time::timeout(timeout, async {
            loop {
                let row = self.store.get_job(id).unwrap().unwrap();
                if !matches!(row.status, JobStatus::Queued | JobStatus::Running) {
                    return row;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("admitted summary must reach durable terminal status within the fixture deadline")
    }
    pub async fn wait(&self, id: Uuid) -> JobView {
        self.run_jobs(id).await
    }
    pub async fn llm_entered(&self) {
        let entered =
            tokio::time::timeout(Duration::from_secs(2), self.llm.entered.notified()).await;
        if entered.is_err() {
            self.llm.release.notify_one();
            self.jobs.shutdown().await.unwrap();
        }
        assert!(
            entered.is_ok(),
            "actual summary handler did not enter the LLM within the fixture deadline"
        );
    }
    pub fn native_versions(&self, session: Uuid) -> AppResult<Vec<NativeSummaryView>> {
        self.service.list(session, None, 100).map(|page| {
            page.rows
                .into_iter()
                .map(|row| match row {
                    SummaryView::Native(v) => v,
                    SummaryView::Legacy(_) => panic!("native fixture was projected as Legacy"),
                })
                .collect()
        })
    }
    pub fn raw_responses(&self, id: Uuid) -> AppResult<Vec<String>> {
        self.store.transaction(|tx| {
            let mut q=tx.prepare("SELECT raw_text FROM summary_responses WHERE summary_id=?1 ORDER BY ordinal LIMIT 64")?;
            let rows=q.query_map([id.to_string()],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
            assert!(rows.iter().all(|r|r.len()<=2*1024*1024));
            Ok(rows)
        })
    }
    pub fn coverage(&self, id: Uuid) -> AppResult<Vec<(String, u32, u32)>> {
        self.store.transaction(|tx| {
            let mut q=tx.prepare("SELECT segment_id,part_index,part_count FROM summary_coverage WHERE summary_id=?1 ORDER BY segment_id,part_index LIMIT 100001")?;
            Ok(q.query_map([id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<Result<Vec<_>,_>>()?)
        })
    }
    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        delete_local_owned(&self.service, id, self.process.clone()).await
    }
    pub async fn close(&self) {
        self.jobs.shutdown().await.unwrap();
    }
    pub fn create_stored(&self, session: Uuid, automatic: bool) -> AppResult<JobView> {
        self.store.create_summary_job(SummaryJobRequest {
            session_id: session,
            profile: Some(self.profile.clone()),
            automatic,
            initial_failure: None,
        })
    }
}
