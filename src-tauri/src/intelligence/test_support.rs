use super::{config::ServiceConfigStore, store::*, types::*};
use crate::{
    config::{ConfigIo, ConfigStore, SecretStore},
    storage::{LocalStore, StoredSession},
};
use std::sync::Arc;
use uuid::Uuid;
pub(crate) struct IntelligenceHarness {
    pub store: Arc<LocalStore>,
    pub secrets: Arc<SecretStore>,
    pub config: Arc<ServiceConfigStore>,
    pub io: ConfigIo,
    pub temp_dir: tempfile::TempDir,
}
impl IntelligenceHarness {
    pub fn new() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(LocalStore::open(temp_dir.path()).unwrap());
        let secrets = Arc::new(SecretStore::for_tests(temp_dir.path()).unwrap());
        // Each temporary directory represents an independent injected app. Keep
        // the production bound/deadline, without sharing a process-global test gate.
        let io = ConfigIo::with_runtime_for_tests(
            ConfigStore::new(temp_dir.path(), secrets.clone()).unwrap(),
            Arc::new(tokio::sync::Semaphore::new(2)),
            std::time::Duration::from_secs(10),
        );
        let config = Arc::new(ServiceConfigStore::new(io.clone(), store.clone()));
        Self {
            store,
            secrets,
            config,
            io,
            temp_dir,
        }
    }
    pub fn session(&self) -> Uuid {
        let id = Uuid::new_v4();
        self.store
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
}
pub(crate) fn test_llm_profile() -> ModelProfile {
    ModelProfile {
        id: Uuid::new_v4(),
        kind: ServiceKind::Llm,
        name: "Fixture".into(),
        base_url: "http://127.0.0.1:12345/v1".into(),
        model: Some("fixture-model".into()),
        timeout_ms: 1000,
        max_context_tokens: Some(4096),
        max_output_tokens: Some(512),
        dimensions: None,
    }
}
pub(crate) fn payload(profiles: Vec<ModelProfile>) -> serde_json::Value {
    serde_json::json!({"version":1,"profiles":profiles,"input":{}})
}
pub(crate) fn manual(session: Uuid) -> JobDraft {
    JobDraft {
        id: Uuid::new_v4(),
        kind: JobKind::Summary,
        session_id: Some(session),
        object_id: Some(Uuid::new_v4()),
        admission: JobAdmission::Manual {
            exclusion_key: format!("summary:{session}"),
        },
        payload: payload(vec![]),
        initial_failure: None,
    }
}
pub(crate) fn automatic(session: Uuid) -> JobDraft {
    JobDraft {
        admission: JobAdmission::AutomaticSummary {
            session_id: session,
        },
        ..manual(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn independent_fixture_directories_do_not_share_config_admission_capacity() {
        let first = IntelligenceHarness::new();
        let second = IntelligenceHarness::new();
        let third = IntelligenceHarness::new();
        let (entered_one_tx, entered_one_rx) = tokio::sync::oneshot::channel();
        let (entered_two_tx, entered_two_rx) = tokio::sync::oneshot::channel();
        let (release_one_tx, release_one_rx) = std::sync::mpsc::channel::<()>();
        let (release_two_tx, release_two_rx) = std::sync::mpsc::channel::<()>();
        let io = first.io.clone();
        let one = tokio::spawn(async move {
            io.run(move |_| {
                let _ = entered_one_tx.send(());
                let _ = release_one_rx.recv();
                Ok(())
            })
            .await
        });
        let io = second.io.clone();
        let two = tokio::spawn(async move {
            io.run(move |_| {
                let _ = entered_two_tx.send(());
                let _ = release_two_rx.recv();
                Ok(())
            })
            .await
        });
        entered_one_rx.await.unwrap();
        entered_two_rx.await.unwrap();
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(1), third.config.settings()).await;
        // Release before asserting, so even the intentional RED cannot retain blocking work.
        drop(release_one_tx);
        drop(release_two_tx);
        one.await.unwrap().unwrap();
        two.await.unwrap().unwrap();
        assert!(
            result.is_ok(),
            "unrelated fixture blocked behind another directory's config work"
        );
        assert!(result.unwrap().unwrap().automatic_summary);
    }
}
