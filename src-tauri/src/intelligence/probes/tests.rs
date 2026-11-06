use super::*;
use crate::{
    app::CredentialChange,
    intelligence::{http::HttpRetryPolicy, test_support::*},
};
use std::time::Duration;
async fn save(h: &IntelligenceHarness, p: ModelProfile) {
    h.config
        .save(SaveModelProfile {
            profile: p,
            credential: CredentialChange::Replace {
                api_key: "synthetic-probe-key".into(),
            },
        })
        .await
        .unwrap();
}
#[tokio::test]
async fn intelligence_probes_use_minimal_kind_specific_requests_and_persist_safe_results() {
    let h = IntelligenceHarness::new();
    let owner = ProbeOwner::new();
    for kind in [
        ServiceKind::Llm,
        ServiceKind::Embedding,
        ServiceKind::Tavily,
    ] {
        let mut p = test_llm_profile();
        p.kind = kind;
        if kind != ServiceKind::Llm {
            p.max_context_tokens = None;
            p.max_output_tokens = None;
        }
        if kind == ServiceKind::Tavily {
            p.model = None;
        }
        save(&h, p.clone()).await;
        h.http.responses.lock().unwrap().push_back(Ok(match kind {
            ServiceKind::Llm => serde_json::json!({"choices":[{"message":{"content":"OK"}}]}),
            ServiceKind::Embedding => serde_json::json!({"data":[{"embedding":vec![0.1;128]}]}),
            ServiceKind::Tavily => serde_json::json!({"results":[]}),
        }));
        let result = owner
            .model(h.config.clone(), h.http.clone(), p.id, Arc::new(()))
            .await
            .unwrap();
        assert!(result.ok);
        assert!(result.checked_at_ms > 0);
        assert_eq!(h.config.load(p.id).await.unwrap().last_probe, Some(result));
        let requests = h.http.requests.lock().unwrap();
        let r = requests.last().unwrap();
        assert!(
            !serde_json::to_string(&r.body)
                .unwrap()
                .contains("synthetic-probe-key")
        );
        match kind {
            ServiceKind::Llm => {
                assert_eq!(r.body["messages"][0]["content"], "Reply with OK.");
                assert_eq!(r.body["max_tokens"], 8);
                assert!(r.retry == HttpRetryPolicy::Never);
                assert_eq!(r.limit, 2 * 1024 * 1024);
                assert!(r.endpoint.path().ends_with("/v1/chat/completions"));
            }
            ServiceKind::Embedding => {
                assert_eq!(r.body["input"], "connection test");
                assert!(r.retry == HttpRetryPolicy::OnceTransient);
                assert_eq!(r.limit, 8 * 1024 * 1024);
            }
            ServiceKind::Tavily => {
                assert_eq!(r.body["max_results"], 1);
                assert_eq!(r.body["include_answer"], false);
                assert_eq!(r.body["include_raw_content"], false);
                assert_eq!(r.limit, 1024 * 1024);
            }
        }
    }
}
#[tokio::test]
async fn intelligence_probes_bad_credentials_make_zero_requests_and_invalid_embedding_fails() {
    let h = IntelligenceHarness::new();
    let owner = ProbeOwner::new();
    let p = test_llm_profile();
    h.config
        .save(SaveModelProfile {
            profile: p.clone(),
            credential: CredentialChange::Clear,
        })
        .await
        .unwrap();
    assert!(
        !owner
            .model(h.config.clone(), h.http.clone(), p.id, Arc::new(()))
            .await
            .unwrap()
            .ok
    );
    assert!(h.http.requests.lock().unwrap().is_empty());
    let mut e = p;
    e.kind = ServiceKind::Embedding;
    e.max_context_tokens = None;
    e.max_output_tokens = None;
    save(&h, e.clone()).await;
    h.http
        .responses
        .lock()
        .unwrap()
        .push_back(Ok(serde_json::json!({"data":[{"embedding":[1,2,3]}]})));
    assert_eq!(
        owner
            .model(h.config.clone(), h.http.clone(), e.id, Arc::new(()))
            .await
            .unwrap()
            .error,
        Some(IntelligenceErrorCode::InvalidResponse)
    );
}
#[tokio::test]
async fn intelligence_probes_global_exclusion_observer_drop_close_and_reopen() {
    let h = IntelligenceHarness::new();
    let owner = Arc::new(ProbeOwner::new());
    let p = test_llm_profile();
    save(&h, p.clone()).await;
    h.http
        .block
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let task = {
        let owner = owner.clone();
        let config = h.config.clone();
        let http = h.http.clone();
        tokio::spawn(async move { owner.model(config, http, p.id, Arc::new(())).await })
    };
    tokio::time::timeout(Duration::from_secs(1), h.http.entered.notified())
        .await
        .unwrap();
    assert!(
        owner
            .model(h.config.clone(), h.http.clone(), p.id, Arc::new(()))
            .await
            .is_err()
    );
    task.abort();
    let _ = task.await;
    owner.shutdown().await.unwrap();
    assert!(
        owner
            .model(h.config.clone(), h.http.clone(), p.id, Arc::new(()))
            .await
            .is_err()
    );
    owner.reopen().unwrap();
    h.http
        .block
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(
        owner
            .model(h.config.clone(), h.http.clone(), p.id, Arc::new(()))
            .await
            .unwrap()
            .ok
    );
}

use crate::providers::*;
use std::sync::atomic::{AtomicUsize, Ordering};
#[derive(Default)]
struct TranslationFixture {
    blocked_phase: AtomicUsize,
    connected: AtomicUsize,
    audio: AtomicUsize,
    finished: AtomicUsize,
    joined: AtomicUsize,
    hold_cleanup: std::sync::atomic::AtomicBool,
    release: tokio::sync::Notify,
    cleanup_entered: tokio::sync::Notify,
}
struct TestProvider(Arc<TranslationFixture>);
struct TestConnection {
    fixture: Arc<TranslationFixture>,
    metadata: ProviderEventMetadata,
    stage: AtomicUsize,
}
#[async_trait::async_trait]
impl TranslationProvider for TestProvider {
    fn id(&self) -> crate::domain::session::ProviderId {
        crate::domain::session::ProviderId::new("volcengine").unwrap()
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            language_pairs: vec![crate::domain::session::LanguagePair::new("en", "zh").unwrap()],
            input_audio: crate::audio::PcmSpec::pcm16_mono(16000),
            output_audio: crate::audio::PcmSpec::pcm16_mono(24000),
            native_chunk_duration: Duration::from_millis(20),
            supports_tts: true,
            supports_bidirectional: true,
            ready: true,
            unavailable_reason: None,
        }
    }
    fn supports_owned_cleanup(&self) -> bool {
        true
    }
    async fn connect(&self, config: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        self.0.connected.fetch_add(1, Ordering::SeqCst);
        if self.0.blocked_phase.load(Ordering::SeqCst) == 1 {
            std::future::pending::<()>().await;
        }
        Ok(Box::new(TestConnection {
            fixture: self.0.clone(),
            metadata: ProviderEventMetadata {
                session_id: config.session_id,
                direction: config.direction,
                provider_event_type: None,
                timestamp: None,
                sentence_id: None,
                local_correlation_id: None,
                log_id: None,
            },
            stage: AtomicUsize::new(0),
        }))
    }
}
#[async_trait::async_trait]
impl ProviderConnection for TestConnection {
    async fn send_audio(&self, _: crate::audio::frame::AudioFrame) -> ProviderResult<()> {
        self.fixture.audio.fetch_add(1, Ordering::SeqCst);
        Err(ProviderError::InvalidAudioFrame)
    }
    async fn finish(&self) -> ProviderResult<()> {
        self.fixture.finished.fetch_add(1, Ordering::SeqCst);
        if self.fixture.blocked_phase.load(Ordering::SeqCst) == 3 {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
    async fn cancel(&self) -> ProviderResult<()> {
        Ok(())
    }
    async fn next_event(&self) -> ProviderResult<Option<ProviderEvent>> {
        if self.fixture.blocked_phase.load(Ordering::SeqCst) == 2 {
            std::future::pending::<()>().await;
        }
        Ok(Some(if self.stage.fetch_add(1, Ordering::SeqCst) == 0 {
            ProviderEvent::SessionStarted {
                metadata: self.metadata.clone(),
            }
        } else {
            ProviderEvent::Finished {
                metadata: self.metadata.clone(),
            }
        }))
    }
    async fn wait_closed(&self) -> ProviderResult<()> {
        self.fixture.cleanup_entered.notify_one();
        if self.fixture.hold_cleanup.load(Ordering::SeqCst) {
            self.fixture.release.notified().await;
        }
        self.fixture.joined.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
async fn translation_saved(h: &IntelligenceHarness, reference: bool) -> Uuid {
    let id = Uuid::new_v4();
    h.io.run(move |tx| {
        tx.secret_set(
            crate::providers::volc::client::SECRET_SERVICE,
            &crate::providers::volc::client::secret_key(id),
            b"synthetic-translation",
        )?;
        tx.save_provider_config(crate::config::ProviderConfig::new(
            id,
            "volcengine",
            crate::providers::volc::client::ENDPOINT,
            None,
            None,
            vec![],
            reference.then(|| crate::providers::factory::volcengine_secret_reference(id)),
        )?)
    })
    .await
    .unwrap();
    id
}
#[tokio::test]
async fn intelligence_translation_probe_handshakes_finishes_joins_without_audio_and_rejects_orphan()
{
    let h = IntelligenceHarness::new();
    let fixture = Arc::new(TranslationFixture::default());
    let factory = Arc::new(
        crate::providers::factory::ProviderFactory::new(h.secrets.clone())
            .with_test_provider(Arc::new(TestProvider(fixture.clone()))),
    );
    let owner = ProbeOwner::new();
    let id = translation_saved(&h, true).await;
    assert!(
        owner
            .translation(
                h.io.clone(),
                factory.clone(),
                h.store.clone(),
                id,
                Arc::new(())
            )
            .await
            .unwrap()
            .ok
    );
    assert_eq!(fixture.connected.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.finished.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.joined.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.audio.load(Ordering::SeqCst), 0);
    let orphan = translation_saved(&h, false).await;
    assert!(
        !owner
            .translation(h.io.clone(), factory, h.store.clone(), orphan, Arc::new(()))
            .await
            .unwrap()
            .ok
    );
    assert_eq!(fixture.connected.load(Ordering::SeqCst), 1);
    assert!(h.store.list_sessions(10, None).unwrap().is_empty());
}
#[tokio::test]
async fn intelligence_translation_probe_timeout_retains_global_and_process_owner_until_joined() {
    let h = IntelligenceHarness::new();
    let fixture = Arc::new(TranslationFixture::default());
    fixture.hold_cleanup.store(true, Ordering::SeqCst);
    let factory = Arc::new(
        crate::providers::factory::ProviderFactory::new(h.secrets.clone())
            .with_test_provider(Arc::new(TestProvider(fixture.clone()))),
    );
    let mut owner = ProbeOwner::new();
    owner.timeout = Duration::from_millis(40);
    let id = translation_saved(&h, true).await;
    let lease = Arc::new(());
    let weak = Arc::downgrade(&lease);
    let result = owner
        .translation(h.io.clone(), factory.clone(), h.store.clone(), id, lease)
        .await
        .unwrap();
    assert_eq!(result.error, Some(IntelligenceErrorCode::Timeout));
    assert!(weak.upgrade().is_some());
    assert!(
        owner
            .translation(h.io.clone(), factory, h.store.clone(), id, Arc::new(()))
            .await
            .is_err()
    );
    fixture.release.notify_one();
    owner.shutdown().await.unwrap();
    assert!(weak.upgrade().is_none());
    assert_eq!(fixture.joined.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn intelligence_probes_foreign_missing_or_invalid_secrets_never_reach_http() {
    for mode in 0..3 {
        let h = IntelligenceHarness::new();
        let p = test_llm_profile();
        save(&h, p.clone()).await;
        match mode {
            0 => {
                let mut doc: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(h.config.path()).unwrap())
                        .unwrap();
                doc["profiles"][0]["credential_reference"] = serde_json::json!(format!(
                    "com.voxbridge.intelligence/{}/api-key",
                    Uuid::new_v4()
                ));
                std::fs::write(h.config.path(), serde_json::to_vec(&doc).unwrap()).unwrap();
            }
            1 => {
                h.secrets
                    .delete("com.voxbridge.intelligence", &format!("{}/api-key", p.id))
                    .unwrap();
            }
            _ => {
                h.secrets
                    .set(
                        "com.voxbridge.intelligence",
                        &format!("{}/api-key", p.id),
                        b"invalid\nkey",
                    )
                    .unwrap();
            }
        }
        assert_eq!(
            ProbeOwner::new()
                .model(h.config.clone(), h.http.clone(), p.id, Arc::new(()))
                .await
                .unwrap()
                .error,
            Some(IntelligenceErrorCode::Configuration)
        );
        assert!(h.http.requests.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn intelligence_translation_probe_bounds_connect_handshake_and_finish() {
    for phase in 1..=3 {
        let h = IntelligenceHarness::new();
        let fixture = Arc::new(TranslationFixture::default());
        fixture.blocked_phase.store(phase, Ordering::SeqCst);
        let factory = Arc::new(
            crate::providers::factory::ProviderFactory::new(h.secrets.clone())
                .with_test_provider(Arc::new(TestProvider(fixture.clone()))),
        );
        let mut owner = ProbeOwner::new();
        owner.timeout = Duration::from_millis(30);
        let id = translation_saved(&h, true).await;
        assert_eq!(
            owner
                .translation(h.io.clone(), factory, h.store.clone(), id, Arc::new(()))
                .await
                .unwrap()
                .error,
            Some(IntelligenceErrorCode::Timeout)
        );
        owner.shutdown().await.unwrap();
        assert_eq!(fixture.audio.load(Ordering::SeqCst), 0);
        assert_eq!(
            fixture.joined.load(Ordering::SeqCst),
            if phase == 1 { 0 } else { 1 }
        );
    }
}
#[tokio::test]
async fn intelligence_translation_actual_native_lookup_retains_global_probe_slot_after_timeout() {
    let h = IntelligenceHarness::new();
    let id = translation_saved(&h, true).await;
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release = Arc::new(std::sync::Mutex::new(release_rx));
    let provider = crate::providers::volc::client::VolcengineProvider::new(h.secrets.clone())
        .with_test_credential_lookup(
            Arc::new(tokio::sync::Semaphore::new(1)),
            Arc::new(move |_, _| {
                entered_tx.send(()).unwrap();
                release.lock().unwrap().recv().unwrap();
                Ok(None)
            }),
        );
    let factory = Arc::new(
        crate::providers::factory::ProviderFactory::new(h.secrets.clone())
            .with_test_provider(Arc::new(provider)),
    );
    let mut owner = ProbeOwner::new();
    owner.timeout = Duration::from_millis(50);
    let lease = Arc::new(());
    let weak = Arc::downgrade(&lease);
    let result = owner
        .translation(h.io.clone(), factory.clone(), h.store.clone(), id, lease)
        .await
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let retained = weak.upgrade().is_some();
    let blocked = owner
        .translation(h.io.clone(), factory, h.store.clone(), id, Arc::new(()))
        .await
        .is_err();
    release_tx.send(()).unwrap();
    owner.shutdown().await.unwrap();
    assert_eq!(result.error, Some(IntelligenceErrorCode::Timeout));
    assert!(retained && blocked);
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn intelligence_probe_late_http_result_cannot_overwrite_edited_profile() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(&h, p.clone()).await;
    h.http.block.store(true, Ordering::SeqCst);
    let config = h.config.clone();
    let http = h.http.clone();
    let id = p.id;
    let task = tokio::spawn(async move {
        ProbeOwner::new()
            .model(config, http, id, Arc::new(()))
            .await
    });
    h.http.entered.notified().await;
    let mut edited = p;
    edited.base_url = "http://127.0.0.1:54321/new".into();
    h.config
        .save(SaveModelProfile {
            profile: edited.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-replaced".into(),
            },
        })
        .await
        .unwrap();
    h.http.resume.notify_one();
    assert!(task.await.unwrap().unwrap().ok);
    let view = h.config.load(id).await.unwrap();
    assert_eq!(view.profile, edited);
    assert!(view.last_probe.is_none());
    assert!(
        h.http.requests.lock().unwrap()[0]
            .endpoint
            .as_str()
            .contains("12345/v1")
    );
}
