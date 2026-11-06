use super::*;
use crate::audio::AudioFrame;
use crate::domain::session::{LanguagePair, ProviderId, SessionDirection, SessionMode};
use async_trait::async_trait;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const INPUT: PcmSpec = PcmSpec::pcm16_mono(16_000);
const OUTPUT: PcmSpec = PcmSpec::pcm16_mono(24_000);
fn saved(provider: &str, endpoint: &str) -> ProviderConfig {
    ProviderConfig::new(Uuid::new_v4(), provider, endpoint, None, None, vec![], None).unwrap()
}
fn live(direction: SessionDirection) -> SessionConfig {
    SessionConfig::live(LanguagePair::new("en", "zh").unwrap(), direction).unwrap()
}
fn factory() -> (tempfile::TempDir, ProviderFactory) {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    (dir, ProviderFactory::new(secrets))
}
#[derive(Default)]
struct Probe {
    opened: Mutex<Vec<ProviderSessionConfig>>,
    cancelled: AtomicBool,
}
struct ControlledProvider {
    caps: ProviderCapabilities,
    probe: Arc<Probe>,
    fail_second: bool,
}
impl ControlledProvider {
    fn new() -> Self {
        Self {
            caps: ProviderCapabilities {
                language_pairs: vec![
                    LanguagePair::new("en", "zh").unwrap(),
                    LanguagePair::new("zh", "en").unwrap(),
                ],
                input_audio: INPUT,
                output_audio: OUTPUT,
                native_chunk_duration: Duration::from_millis(80),
                supports_tts: true,
                supports_bidirectional: true,
                ready: true,
                unavailable_reason: None,
            },
            probe: Arc::new(Probe::default()),
            fail_second: false,
        }
    }
}
#[async_trait]
impl TranslationProvider for ControlledProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("controlled").unwrap()
    }
    fn capabilities(&self) -> ProviderCapabilities {
        self.caps.clone()
    }
    async fn connect(&self, config: ProviderSessionConfig) -> ProviderResult<ProviderStream> {
        let mut opened = self.probe.opened.lock().unwrap();
        opened.push(config.clone());
        if self.fail_second && opened.len() == 2 {
            return Err(ProviderError::Authentication);
        }
        Ok(Box::new(ControlledConnection {
            config,
            probe: self.probe.clone(),
            emitted: AtomicBool::new(false),
        }))
    }
}
struct ControlledConnection {
    config: ProviderSessionConfig,
    probe: Arc<Probe>,
    emitted: AtomicBool,
}
#[async_trait]
impl ProviderConnection for ControlledConnection {
    async fn send_audio(&self, _: AudioFrame) -> ProviderResult<()> {
        Err(ProviderError::NotStarted)
    }
    async fn finish(&self) -> ProviderResult<()> {
        Ok(())
    }
    async fn cancel(&self) -> ProviderResult<()> {
        self.probe.cancelled.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn next_event(&self) -> ProviderResult<Option<ProviderEvent>> {
        if self.emitted.swap(true, Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(Some(ProviderEvent::SessionStarted {
            metadata: ProviderEventMetadata {
                session_id: self.config.session_id,
                direction: self.config.direction,
                provider_event_type: None,
                timestamp: None,
                sentence_id: None,
                local_correlation_id: None,
                log_id: None,
            },
        }))
    }
}

#[test]
fn factory_selects_documented_providers_and_keeps_unknown_and_secondary_unavailable() {
    let (_dir, f) = factory();
    let session = live(SessionDirection::SourceToTarget);
    let volc = saved("volcengine", super::super::volc::client::ENDPOINT);
    assert!(f.capabilities(&volc).unwrap().ready);
    let prepared = f
        .prepare(&volc, Uuid::new_v4(), &session, INPUT, OUTPUT)
        .unwrap();
    assert_eq!(prepared.configs()[0].provider_config_id, volc.id());
    for id in ["aliyun", "palabra", "unknown", "fixture", "controlled"] {
        let settings = saved(id, "https://api.example.com");
        assert!(!f.capabilities(&settings).unwrap().ready);
        assert!(matches!(
            f.prepare(&settings, Uuid::new_v4(), &session, INPUT, OUTPUT),
            Err(ProviderError::Unavailable)
        ));
    }
}

#[tokio::test]
async fn factory_validates_both_call_directions_before_connect_and_aligns_metadata() {
    let p = Arc::new(ControlledProvider::new());
    let probe = p.probe.clone();
    let (_dir, f) = factory();
    let f = f.with_test_provider(p);
    let settings = saved("controlled", "https://api.example.com");
    let session_id = Uuid::new_v4();
    let call = SessionConfig::call(LanguagePair::new("en", "zh").unwrap()).unwrap();
    let prepared = f
        .prepare(&settings, session_id, &call, INPUT, OUTPUT)
        .unwrap();
    assert!(probe.opened.lock().unwrap().is_empty());
    let configs = prepared.configs();
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].direction, SessionDirection::SourceToTarget);
    assert_eq!(configs[0].languages, LanguagePair::new("en", "zh").unwrap());
    assert_eq!(configs[1].direction, SessionDirection::TargetToSource);
    assert_eq!(configs[1].languages, LanguagePair::new("zh", "en").unwrap());
    for config in configs {
        assert_eq!(config.session_id, session_id);
        assert_eq!(config.mode, SessionMode::Call);
    }
    let streams = prepared.connect().await.unwrap();
    assert_eq!(probe.opened.lock().unwrap().len(), 2);
    for stream in streams {
        let event = stream.stream.next_event().await.unwrap().unwrap();
        assert_eq!(event.metadata().direction, stream.config.direction);
        assert_eq!(event.metadata().session_id, session_id);
    }
}

#[tokio::test]
async fn cloned_preparation_connects_only_the_requested_reverse_direction() {
    let p = Arc::new(ControlledProvider::new());
    let probe = p.probe.clone();
    let (_dir, f) = factory();
    let f = f.with_test_provider(p);
    let call = SessionConfig::call(LanguagePair::new("en", "zh").unwrap()).unwrap();
    let prepared = f
        .prepare(
            &saved("controlled", "https://api.example.com"),
            Uuid::new_v4(),
            &call,
            INPUT,
            OUTPUT,
        )
        .unwrap();

    let reverse = prepared
        .clone()
        .connect_direction(SessionDirection::TargetToSource)
        .await
        .unwrap();

    assert_eq!(reverse.config.direction, SessionDirection::TargetToSource);
    assert_eq!(
        reverse.config.languages,
        LanguagePair::new("zh", "en").unwrap()
    );
    assert_eq!(probe.opened.lock().unwrap().as_slice(), &[reverse.config]);
}

#[test]
fn reverse_live_swaps_languages_without_creating_a_second_stream() {
    let (_dir, f) = factory();
    let f = f.with_test_provider(Arc::new(ControlledProvider::new()));
    let prepared = f
        .prepare(
            &saved("controlled", "https://api.example.com"),
            Uuid::new_v4(),
            &live(SessionDirection::TargetToSource),
            INPUT,
            OUTPUT,
        )
        .unwrap();
    assert_eq!(prepared.configs().len(), 1);
    assert_eq!(
        prepared.configs()[0].languages,
        LanguagePair::new("zh", "en").unwrap()
    );
    assert_eq!(
        prepared.configs()[0].direction,
        SessionDirection::TargetToSource
    );
}

#[test]
fn missing_reverse_pair_prevents_any_partial_opening() {
    let mut p = ControlledProvider::new();
    p.caps.language_pairs.truncate(1);
    let probe = p.probe.clone();
    let (_dir, f) = factory();
    let f = f.with_test_provider(Arc::new(p));
    let call = SessionConfig::call(LanguagePair::new("en", "zh").unwrap()).unwrap();
    assert!(matches!(
        f.prepare(
            &saved("controlled", "https://api.example.com"),
            Uuid::new_v4(),
            &call,
            INPUT,
            OUTPUT
        ),
        Err(ProviderError::UnsupportedLanguagePair)
    ));
    assert!(probe.opened.lock().unwrap().is_empty());
}

#[test]
fn preflight_rejects_non_tts_and_wrong_audio_before_opening() {
    let mut p = ControlledProvider::new();
    p.caps.supports_tts = false;
    let probe = p.probe.clone();
    let (_dir, f) = factory();
    let f = f.with_test_provider(Arc::new(p));
    let settings = saved("controlled", "https://api.example.com");
    let session = live(SessionDirection::SourceToTarget);
    assert!(matches!(
        f.prepare(&settings, Uuid::new_v4(), &session, INPUT, OUTPUT),
        Err(ProviderError::UnsupportedTts)
    ));
    assert!(probe.opened.lock().unwrap().is_empty());
    let p = Arc::new(ControlledProvider::new());
    let probe = p.probe.clone();
    let (_dir, f) = factory();
    let f = f.with_test_provider(p);
    for (input, output) in [
        (PcmSpec::pcm16_mono(48_000), OUTPUT),
        (INPUT, PcmSpec::pcm16_mono(16_000)),
    ] {
        assert!(matches!(
            f.prepare(&settings, Uuid::new_v4(), &session, input, output),
            Err(ProviderError::UnsupportedAudioFormat)
        ));
    }
    assert!(probe.opened.lock().unwrap().is_empty());
}

#[tokio::test]
async fn second_connection_error_cancels_already_open_first_stream() {
    let mut p = ControlledProvider::new();
    p.fail_second = true;
    let probe = p.probe.clone();
    let (_dir, f) = factory();
    let f = f.with_test_provider(Arc::new(p));
    let call = SessionConfig::call(LanguagePair::new("en", "zh").unwrap()).unwrap();
    let prepared = f
        .prepare(
            &saved("controlled", "https://api.example.com"),
            Uuid::new_v4(),
            &call,
            INPUT,
            OUTPUT,
        )
        .unwrap();
    assert!(matches!(
        prepared.connect().await,
        Err(ProviderError::Authentication)
    ));
    assert!(probe.cancelled.load(Ordering::SeqCst));
}

#[test]
fn saved_volc_endpoint_and_credential_reference_cannot_be_silently_ignored() {
    let (_dir, f) = factory();
    let session = live(SessionDirection::SourceToTarget);
    for endpoint in [
        "wss://elsewhere.example/translate",
        "https://openspeech.bytedance.com/api/v4/ast/v2/translate",
    ] {
        assert!(matches!(
            f.prepare(
                &saved("volcengine", endpoint),
                Uuid::new_v4(),
                &session,
                INPUT,
                OUTPUT
            ),
            Err(ProviderError::InvalidConfiguration)
        ));
    }
    let id = Uuid::new_v4();
    let make = |reference| {
        ProviderConfig::new(
            id,
            "volcengine",
            super::super::volc::client::ENDPOINT,
            None,
            None,
            vec![],
            Some(reference),
        )
        .unwrap()
    };
    let invalid = make("other-profile/api-key".to_owned());
    assert!(matches!(
        f.prepare(&invalid, Uuid::new_v4(), &session, INPUT, OUTPUT),
        Err(ProviderError::InvalidConfiguration)
    ));
    let reference = volcengine_secret_reference(id);
    assert_eq!(
        reference,
        format!("com.voxbridge.providers.volcengine/{id}/api-key")
    );
    assert!(
        f.prepare(&make(reference), Uuid::new_v4(), &session, INPUT, OUTPUT)
            .is_ok()
    );
}

#[test]
fn deserialized_configuration_is_revalidated_before_selection() {
    let (_dir, f) = factory();
    let initial = saved("volcengine", super::super::volc::client::ENDPOINT);
    let mut value = serde_json::to_value(initial).unwrap();
    value["endpoint"] = serde_json::json!("ws://untrusted.example/stream");
    let malformed: ProviderConfig = serde_json::from_value(value).unwrap();
    assert!(matches!(
        f.prepare(
            &malformed,
            Uuid::new_v4(),
            &live(SessionDirection::SourceToTarget),
            INPUT,
            OUTPUT
        ),
        Err(ProviderError::InvalidConfiguration)
    ));
}
