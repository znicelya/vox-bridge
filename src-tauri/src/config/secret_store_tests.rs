use super::{ConfigStore, ProviderConfig, SecretStore};
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
