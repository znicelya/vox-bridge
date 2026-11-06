use super::{ConfigStore, ProviderConfig, SecretStore, TavilyConfig};
use crate::{config::secret_store::KeyBackend, error::AppResult};
use fs2::FileExt;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

struct CountingKeyring {
    key: Vec<u8>,
    calls: AtomicUsize,
}

impl CountingKeyring {
    fn new() -> Self {
        Self {
            key: vec![7; 32],
            calls: AtomicUsize::new(0),
        }
    }
}

impl KeyBackend for CountingKeyring {
    fn load_or_create(&self, _lock_path: &Path) -> AppResult<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.key.clone())
    }
}

fn provider(id: Uuid, suffix: impl std::fmt::Display) -> ProviderConfig {
    ProviderConfig::new(
        id,
        "volcengine",
        format!("https://api{suffix}.example.com"),
        None,
        None,
        vec!["zh".into(), "en".into()],
        None,
    )
    .unwrap()
}

#[test]
fn secret_round_trip_uses_encrypted_file_with_injected_key_backend() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::for_tests(dir.path()).unwrap();
    store.set("volcengine", "api_key", b"secret").unwrap();
    assert_eq!(
        store.get("volcengine", "api_key").unwrap().unwrap(),
        b"secret"
    );
    assert!(
        !std::fs::read_to_string(store.config_path())
            .unwrap()
            .contains("secret")
    );
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")
    }));
}

#[test]
fn malformed_nonce_is_rejected_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::for_tests(dir.path()).unwrap();
    store.set("service", "key", b"secret").unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.config_path()).unwrap()).unwrap();
    json["service/key"]["nonce"] = serde_json::Value::String("c2hvcnQ=".into());
    std::fs::write(store.config_path(), serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(store.get("service", "key").is_err());
}

#[test]
fn concurrent_first_use_shares_one_dek() {
    let dir = tempfile::tempdir().unwrap();
    let backend = SecretStore::test_backend();
    let first = SecretStore::with_test_backend(dir.path(), Arc::clone(&backend)).unwrap();
    let second = SecretStore::with_test_backend(dir.path(), Arc::clone(&backend)).unwrap();
    let a = std::thread::spawn(move || first.backend_key_for_tests());
    let b = std::thread::spawn(move || second.backend_key_for_tests());
    assert_eq!(a.join().unwrap().unwrap(), b.join().unwrap().unwrap());
}

#[test]
fn advisory_dek_lock_blocks_second_handle() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secrets.dek.lock");
    let first = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    first.lock_exclusive().unwrap();
    let second = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    assert!(second.try_lock_exclusive().is_err());
}

#[test]
fn provider_config_round_trip_keeps_secret_out_of_public_view() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let mut configs = ConfigStore::new(dir.path(), secrets).unwrap();
    let id = Uuid::new_v4();
    let config = ProviderConfig::new(
        id,
        "volcengine",
        "https://api.example.com",
        Some("general".to_owned()),
        Some("model-x".to_owned()),
        vec!["zh".to_owned(), "en".to_owned()],
        Some("secret-ref".to_owned()),
    )
    .unwrap();
    configs.save_provider_config(config).unwrap();
    let loaded = configs.load_provider_config(id).unwrap();
    assert_eq!(loaded.endpoint(), "https://api.example.com");
    assert_eq!(loaded.public_view().secret_reference(), Some("secret-ref"));
    assert!(
        !serde_json::to_string(&loaded.public_view())
            .unwrap()
            .contains("SENSITIVE_KEY_MATERIAL")
    );
}

#[test]
fn constructors_are_lazy_and_provider_crud_uses_arc_secret_store() {
    let root = tempfile::tempdir().unwrap();
    let data_dir = root.path().join("not-created-yet");
    let secrets = Arc::new(SecretStore::new(&data_dir).unwrap());
    let mut configs = ConfigStore::new(&data_dir, Arc::clone(&secrets)).unwrap();
    let io = super::ConfigIo::new(ConfigStore::new(&data_dir, Arc::clone(&secrets)).unwrap());
    assert!(!data_dir.exists(), "constructors must not perform OS work");
    assert_eq!(io.data_dir(), data_dir.as_path());

    let first = provider(Uuid::new_v4(), 1)
        .with_display_name(" First profile ".into())
        .unwrap();
    let second = provider(Uuid::new_v4(), 2);
    configs.save_provider_config(first.clone()).unwrap();
    configs.save_provider_config(second.clone()).unwrap();

    let listed = configs.list_provider_configs().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(
        configs
            .load_provider_config(first.id())
            .unwrap()
            .display_name(),
        "First profile"
    );
    configs.delete_provider_config(first.id()).unwrap();
    assert!(configs.load_provider_config(first.id()).is_err());
    assert_eq!(configs.list_provider_configs().unwrap(), vec![second]);
}

#[test]
fn legacy_provider_json_defaults_display_name_to_provider_id() {
    let id = Uuid::new_v4();
    let legacy = serde_json::json!({
        "id": id,
        "provider": "volcengine",
        "endpoint": "https://api.example.com",
        "llm": null,
        "embedding": null,
        "tavily": null,
        "language_capabilities": ["zh", "en"],
        "secret_reference": "credential-a"
    });
    let config: ProviderConfig = serde_json::from_value(legacy).unwrap();
    assert_eq!(config.display_name(), "volcengine");
    assert_eq!(config.public_view().display_name(), "volcengine");
    assert_eq!(
        config
            .clone()
            .with_secret_reference(Some("credential-b".into()))
            .secret_reference(),
        Some("credential-b")
    );

    for invalid in [String::new(), "   ".into(), "x".repeat(81)] {
        assert!(config.clone().with_display_name(invalid).is_err());
    }
}

#[test]
fn provider_json_rejects_invalid_display_names_during_deserialization() {
    let valid = serde_json::json!({
        "id": Uuid::new_v4(),
        "provider": "volcengine",
        "display_name": "valid profile",
        "endpoint": "https://api.example.com",
        "llm": null,
        "embedding": null,
        "tavily": null,
        "language_capabilities": ["zh", "en"],
        "secret_reference": null
    });

    for invalid in ["   ".to_owned(), "x".repeat(81)] {
        let mut malformed = valid.clone();
        malformed["display_name"] = serde_json::Value::String(invalid);
        assert!(serde_json::from_value::<ProviderConfig>(malformed).is_err());
    }
}

#[test]
fn concurrent_config_stores_preserve_every_read_modify_write() {
    let dir = tempfile::tempdir().unwrap();
    let backend = SecretStore::test_backend();
    let first = ConfigStore::new(
        dir.path(),
        Arc::new(SecretStore::with_test_backend(dir.path(), Arc::clone(&backend)).unwrap()),
    )
    .unwrap();
    let second = ConfigStore::new(
        dir.path(),
        Arc::new(SecretStore::with_test_backend(dir.path(), backend).unwrap()),
    )
    .unwrap();
    let start = Arc::new(std::sync::Barrier::new(2));
    let run = |mut store: ConfigStore, offset: u128, start: Arc<std::sync::Barrier>| {
        std::thread::spawn(move || {
            start.wait();
            for value in 0..32_u128 {
                store
                    .save_provider_config(provider(Uuid::from_u128(offset + value), offset + value))
                    .unwrap();
            }
        })
    };
    let a = run(first, 1, Arc::clone(&start));
    let b = run(second, 1_000, start);
    a.join().unwrap();
    b.join().unwrap();

    let store = ConfigStore::new(
        dir.path(),
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    )
    .unwrap();
    assert_eq!(store.list_provider_configs().unwrap().len(), 64);
}

#[test]
fn contains_and_delete_do_not_access_the_native_key_backend() {
    let dir = tempfile::tempdir().unwrap();
    let backend = Arc::new(CountingKeyring::new());
    let backend_trait: Arc<dyn KeyBackend> = backend.clone();
    let store = SecretStore::with_test_backend(dir.path(), backend_trait).unwrap();
    store
        .set("provider", "api-key", b"private-fixture-value")
        .unwrap();
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);

    assert!(store.contains("provider", "api-key").unwrap());
    store.delete("provider", "api-key").unwrap();
    assert!(!store.contains("provider", "api-key").unwrap());
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert!(
        !std::fs::read_to_string(store.config_path())
            .unwrap()
            .contains("private-fixture-value")
    );
}

#[test]
fn concurrent_secret_stores_preserve_every_read_modify_write() {
    let dir = tempfile::tempdir().unwrap();
    let backend = SecretStore::test_backend();
    let first = SecretStore::with_test_backend(dir.path(), Arc::clone(&backend)).unwrap();
    let second = SecretStore::with_test_backend(dir.path(), Arc::clone(&backend)).unwrap();
    let start = Arc::new(std::sync::Barrier::new(2));
    let run = |store: SecretStore, prefix: &'static str, start: Arc<std::sync::Barrier>| {
        std::thread::spawn(move || {
            start.wait();
            for value in 0..24 {
                store
                    .set("provider", &format!("{prefix}-{value}"), prefix.as_bytes())
                    .unwrap();
            }
        })
    };
    let a = run(first, "first", Arc::clone(&start));
    let b = run(second, "second", start);
    a.join().unwrap();
    b.join().unwrap();

    let store = SecretStore::with_test_backend(dir.path(), backend).unwrap();
    for prefix in ["first", "second"] {
        for value in 0..24 {
            assert!(
                store
                    .contains("provider", &format!("{prefix}-{value}"))
                    .unwrap()
            );
        }
    }
}

#[test]
fn failed_atomic_write_preserves_destination_and_cleans_unique_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("provider-configs.json");
    std::fs::write(&destination, b"prior").unwrap();
    let opened_temp = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&opened_temp);

    let result = super::atomic_write_with(&destination, b"replacement", move |path, _bytes| {
        *capture.lock().unwrap() = Some(path.to_path_buf());
        std::fs::write(path, b"partial")?;
        Err(std::io::Error::other("injected pre-replace failure"))
    });

    assert!(result.is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"prior");
    let temp = opened_temp.lock().unwrap().clone().unwrap();
    assert!(!temp.exists());
    assert!(
        temp.file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".tmp-")
    );
}

#[test]
fn provider_config_rejects_non_https_remote_endpoint() {
    assert!(
        ProviderConfig::new(
            Uuid::new_v4(),
            "provider",
            "http://api.example.com",
            None,
            None,
            vec![],
            None
        )
        .is_err()
    );
}

#[test]
fn provider_config_rejects_invalid_or_hostless_urls() {
    for endpoint in ["not a url", "file:///tmp/provider"] {
        assert!(
            ProviderConfig::new(
                Uuid::new_v4(),
                "provider",
                endpoint,
                None,
                None,
                vec![],
                None
            )
            .is_err()
        );
    }
    let mut config = ProviderConfig::new(
        Uuid::new_v4(),
        "provider",
        "https://api.example.com",
        None,
        None,
        vec![],
        None,
    )
    .unwrap();
    config.tavily = Some(TavilyConfig {
        endpoint: "http://search.example.com".into(),
        search_depth: None,
    });
    assert!(config.validate().is_err());
}

#[test]
fn provider_config_allows_wss_for_translation_but_not_http_model_endpoints() {
    for provider in ["volcengine", "aliyun", "palabra"] {
        assert!(
            ProviderConfig::new(
                Uuid::new_v4(),
                provider,
                "wss://translation.example.com/stream",
                None,
                None,
                vec![],
                None,
            )
            .is_ok()
        );
    }
    for provider in ["llm", "embedding", "tavily"] {
        assert!(
            ProviderConfig::new(
                Uuid::new_v4(),
                provider,
                "wss://translation.example.com/stream",
                None,
                None,
                vec![],
                None,
            )
            .is_err()
        );
    }
}

#[test]
fn provider_config_rejects_url_credentials_without_echoing_them() {
    for endpoint in [
        "https://operator:private-fixture-value@api.example.com",
        "https://api.example.com/#private-fixture-value",
        "https://api.example.com/?api_key=private-fixture-value",
        "https://api.example.com/?access_token=private-fixture-value",
        "wss://api.example.com/?token=private-fixture-value",
    ] {
        let result = ProviderConfig::new(
            Uuid::new_v4(),
            "volcengine",
            endpoint,
            None,
            None,
            vec![],
            None,
        );
        let error = result.expect_err("credentials must not be accepted in endpoint URLs");
        assert!(!error.to_string().contains("private-fixture-value"));
    }
    assert!(
        ProviderConfig::new(
            Uuid::new_v4(),
            "aliyun",
            "wss://workspace.example.com/realtime?model=qwen-audio",
            None,
            None,
            vec![],
            None,
        )
        .is_ok()
    );
}
