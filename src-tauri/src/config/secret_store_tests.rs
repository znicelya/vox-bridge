use super::{ConfigStore, ProviderConfig, SecretStore, TavilyConfig};
use fs2::FileExt;
use std::sync::Arc;
use uuid::Uuid;

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
    let secrets = SecretStore::for_tests(dir.path()).unwrap();
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
