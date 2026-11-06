use super::*;
use crate::config::ProviderConfig;
use crate::providers::{
    factory::volcengine_secret_reference,
    volc::client::{ENDPOINT, SECRET_SERVICE, secret_key},
};

fn saved(id: uuid::Uuid) -> ProviderConfig {
    ProviderConfig::new(id, "volcengine", ENDPOINT, None, None, vec![], None).unwrap()
}

#[tokio::test]
async fn replace_keep_clear_reload_never_exposes_synthetic_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let backend = SecretStore::test_backend();
    let secrets = Arc::new(SecretStore::with_test_backend(dir.path(), backend.clone()).unwrap());
    let h = tests::fixture();
    let app = tests::injected(dir.path(), &h, secrets.clone());
    let id = uuid::Uuid::new_v4();
    let config =
        saved(id).with_secret_reference(Some(volcengine_secret_reference(uuid::Uuid::new_v4())));
    let first = app
        .save_provider_config(SaveProviderRequest {
            config: config.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-credential-one".into(),
            },
        })
        .await
        .unwrap();
    assert!(first.credential_configured);
    assert_eq!(
        first.config.secret_reference(),
        Some(volcengine_secret_reference(id).as_str())
    );
    let json = serde_json::to_string(&first).unwrap();
    assert!(!json.contains("synthetic-credential"));
    assert!(
        !std::fs::read_to_string(dir.path().join("provider-configs.json"))
            .unwrap()
            .contains("synthetic-credential")
    );
    assert!(
        !std::fs::read_to_string(dir.path().join("secrets.json"))
            .unwrap()
            .contains("synthetic-credential")
    );
    app.save_provider_config(SaveProviderRequest {
        config: saved(id),
        credential: CredentialChange::Keep,
    })
    .await
    .unwrap();
    assert!(app.list_provider_configs().await.unwrap()[0].credential_configured);
    app.save_provider_config(SaveProviderRequest {
        config: saved(id),
        credential: CredentialChange::Replace {
            api_key: "synthetic-credential-two".into(),
        },
    })
    .await
    .unwrap();
    drop(app);
    let reload = Arc::new(SecretStore::with_test_backend(dir.path(), backend).unwrap());
    let app = tests::injected(dir.path(), &h, reload.clone());
    assert_eq!(
        reload
            .get(SECRET_SERVICE, &secret_key(id))
            .unwrap()
            .unwrap(),
        b"synthetic-credential-two"
    );
    let cleared = app
        .save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Clear,
        })
        .await
        .unwrap();
    assert!(!cleared.credential_configured);
    assert_eq!(cleared.config.secret_reference(), None);
    assert!(!reload.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
}

#[tokio::test]
async fn invalid_credential_and_endpoint_make_no_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let h = tests::fixture();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = tests::injected(dir.path(), &h, secrets);
    for api_key in [String::new(), "invalid\r\nkey".into(), "x".repeat(4097)] {
        assert!(
            app.save_provider_config(SaveProviderRequest {
                config: saved(uuid::Uuid::new_v4()),
                credential: CredentialChange::Replace { api_key }
            })
            .await
            .is_err()
        );
    }
    let wrong_endpoint = ProviderConfig::new(
        uuid::Uuid::new_v4(),
        "volcengine",
        "wss://untrusted.example",
        None,
        None,
        vec![],
        None,
    )
    .unwrap();
    assert!(
        app.save_provider_config(SaveProviderRequest {
            config: wrong_endpoint,
            credential: CredentialChange::Replace {
                api_key: "synthetic-only".into()
            },
        })
        .await
        .is_err()
    );
    assert!(app.list_provider_configs().await.unwrap().is_empty());
    assert!(!dir.path().join("secrets.json").exists());
}

#[tokio::test]
async fn normalized_reference_without_encrypted_entry_is_not_configured() {
    let dir = tempfile::tempdir().unwrap();
    let h = tests::fixture();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = tests::injected(dir.path(), &h, secrets.clone());
    let id = uuid::Uuid::new_v4();
    app.save_provider_config(SaveProviderRequest {
        config: saved(id),
        credential: CredentialChange::Replace {
            api_key: "synthetic-only".into(),
        },
    })
    .await
    .unwrap();
    secrets.delete(SECRET_SERVICE, &secret_key(id)).unwrap();
    let views = app.list_provider_configs().await.unwrap();
    assert!(views[0].config.secret_reference().is_some());
    assert!(!views[0].credential_configured);
}

#[tokio::test]
async fn replacement_rolls_back_existing_and_absent_encrypted_before_images() {
    for had_key in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let h = tests::fixture();
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        let app = tests::injected(dir.path(), &h, secrets.clone());
        let id = uuid::Uuid::new_v4();
        app.save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: if had_key {
                CredentialChange::Replace {
                    api_key: "synthetic-prior".into(),
                }
            } else {
                CredentialChange::Keep
            },
        })
        .await
        .unwrap();
        let config_before = std::fs::read(dir.path().join("provider-configs.json")).unwrap();
        app.config_io.fail_next_for_tests("save_config");
        let result = app
            .save_provider_config(SaveProviderRequest {
                config: saved(id).with_display_name("Changed".into()).unwrap(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-new".into(),
                },
            })
            .await;
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(dir.path().join("provider-configs.json")).unwrap(),
            config_before
        );
        assert_eq!(
            secrets.get(SECRET_SERVICE, &secret_key(id)).unwrap(),
            had_key.then(|| b"synthetic-prior".to_vec())
        );
    }
}

#[tokio::test]
async fn clear_config_failure_preserves_key_and_cleanup_failure_leaves_unconfigured_orphan() {
    let dir = tempfile::tempdir().unwrap();
    let h = tests::fixture();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = tests::injected(dir.path(), &h, secrets.clone());
    let id = uuid::Uuid::new_v4();
    app.save_provider_config(SaveProviderRequest {
        config: saved(id),
        credential: CredentialChange::Replace {
            api_key: "synthetic-prior".into(),
        },
    })
    .await
    .unwrap();
    app.config_io.fail_next_for_tests("save_config");
    assert!(
        app.save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Clear
        })
        .await
        .is_err()
    );
    assert!(app.list_provider_configs().await.unwrap()[0].credential_configured);
    app.config_io.fail_next_for_tests("delete_secret");
    let error = app
        .save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Clear,
        })
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("partial failure"));
    assert!(!app.list_provider_configs().await.unwrap()[0].credential_configured);
    assert!(secrets.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
    // Keep must not turn a cleanup orphan into a configured credential.
    assert!(
        !app.save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Keep
        })
        .await
        .unwrap()
        .credential_configured
    );
    app.config_io.fail_next_for_tests("delete_secret");
    assert!(
        app.delete_provider_config(id)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("partial failure")
    );
    assert!(app.list_provider_configs().await.unwrap().is_empty());
    assert!(secrets.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
}

#[tokio::test]
async fn compensation_failure_is_explicit_and_never_returns_false_success() {
    let dir = tempfile::tempdir().unwrap();
    let h = tests::fixture();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let app = tests::injected(dir.path(), &h, secrets.clone());
    let id = uuid::Uuid::new_v4();
    app.config_io.fail_next_for_tests("save_config");
    app.config_io.fail_next_for_tests("restore_secret");
    let error = app
        .save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Replace {
                api_key: "synthetic-new".into(),
            },
        })
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("compensation failed (partial failure)"));
    assert!(!error.contains("synthetic-new"));
    assert!(app.list_provider_configs().await.unwrap().is_empty());
    assert!(secrets.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn final_fix_held_config_close_is_bounded_and_retains_one_cleanup_owner() {
    for cancel_caller in [false, true] {
        use fs2::FileExt;
        use std::{fs::OpenOptions, time::Duration};
        let dir = tempfile::tempdir().unwrap();
        let h = tests::fixture();
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        let mut app = tests::injected(dir.path(), &h, secrets.clone());
        let id = h.saved.id();
        app.save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Replace {
                api_key: "synthetic-prior".into(),
            },
        })
        .await
        .unwrap();
        app.config_io = ConfigIo::with_runtime_for_tests(
            ConfigStore::new(dir.path(), secrets.clone()).unwrap(),
            Arc::new(tokio::sync::Semaphore::new(2)),
            Duration::from_millis(500),
        );
        let secret_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.path().join("secrets.lock"))
            .unwrap();
        secret_lock.lock_exclusive().unwrap();
        let owner = app.clone();
        let delete = tokio::spawn(async move { owner.delete_provider_config(id).await });
        // The config disappears before secret cleanup attempts its blocked lock.
        tests::eventually(|| {
            std::fs::read_to_string(dir.path().join("provider-configs.json"))
                .ok()
                .is_some_and(|v| v.trim() == "[]")
        })
        .await;
        let owner = app.clone();
        let launch = h.launch.clone();
        let create = tokio::spawn(async move { owner.create_session(launch).await });
        if cancel_caller {
            delete.abort();
            assert!(delete.await.unwrap_err().is_cancelled());
        } else {
            let result = delete.await.unwrap();
            assert!(result.err().unwrap().to_string().contains("timed out"));
        }
        assert!(!create.is_finished());
        app.close_state.lock().unwrap().observation_timeout = Some(Duration::from_millis(40));
        let owner = app.clone();
        let cancelled_close = tokio::spawn(async move { owner.close_all().await });
        tests::eventually(|| app.admitted_epoch().is_err()).await;
        let epoch = *app.close_epoch.borrow();
        cancelled_close.abort();
        let _ = cancelled_close.await;
        let bounded = tokio::time::timeout(Duration::from_millis(250), app.close_all()).await;
        let denied_reopen = app.open_window().is_err();
        let same_owner_epoch = *app.close_epoch.borrow() == epoch;
        assert_eq!(h.audio.inventory_calls(), 0);
        assert!(AppState::new(dir.path().to_path_buf()).is_err());
        // Native transaction ownership remains intact after caller cancellation/timeout.
        secret_lock.unlock().unwrap();
        app.close_all().await.unwrap();
        assert!(app.create_session(h.launch.clone()).await.is_err());
        assert!(create.await.unwrap().is_err());
        assert_eq!(h.audio.open_calls(), 0);
        assert!(
            bounded.is_ok(),
            "close observation must include the held app admission gate"
        );
        assert!(
            bounded
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("cleanup remains pending")
        );
        assert!(denied_reopen);
        assert!(
            same_owner_epoch,
            "timeout observers must not spawn duplicate close owners"
        );
        app.open_window().unwrap();
        drop(app);
        assert_eq!(h.audio.inventory_calls(), 0);
        assert_eq!(h.audio.open_calls(), 0);
        tests::eventually(|| AppState::new(dir.path().to_path_buf()).is_ok()).await;
        assert!(!secrets.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
    }
}

#[tokio::test]
async fn partial_clear_then_keep_orphan_cannot_authenticate_fresh_volc_launch() {
    use crate::providers::volc::client::VolcengineProvider;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = tempfile::tempdir().unwrap();
    let h = tests::fixture();
    let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
    let lookups = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(AtomicUsize::new(0));
    let calls = lookups.clone();
    let credentials = observed.clone();
    let provider = VolcengineProvider::new(secrets.clone()).with_test_credential_lookup(
        Arc::new(tokio::sync::Semaphore::new(2)),
        Arc::new(move |secrets, key| {
            calls.fetch_add(1, Ordering::SeqCst);
            if secrets.get(SECRET_SERVICE, &key)?.as_deref()
                == Some(b"synthetic-orphan-key".as_slice())
            {
                credentials.fetch_add(1, Ordering::SeqCst);
            }
            // Stop the real connector before any network/TLS work.
            Ok(None)
        }),
    );
    let factory =
        Arc::new(ProviderFactory::new(secrets.clone()).with_test_provider(Arc::new(provider)));
    let app = AppState::with_dependencies(
        dir.path().to_path_buf(),
        secrets.clone(),
        h.audio.clone(),
        factory,
    )
    .unwrap();
    let id = h.saved.id();
    app.save_provider_config(SaveProviderRequest {
        config: saved(id),
        credential: CredentialChange::Replace {
            api_key: "synthetic-orphan-key".into(),
        },
    })
    .await
    .unwrap();
    app.config_io.fail_next_for_tests("delete_secret");
    assert!(
        app.save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Clear
        })
        .await
        .is_err()
    );
    let view = app
        .save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Keep,
        })
        .await
        .unwrap();
    assert!(!view.credential_configured);
    assert_eq!(view.config.secret_reference(), None);
    assert!(secrets.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
    assert!(app.provider_capabilities(id).await.unwrap().ready);
    let validation = app.validate_session_launch(h.launch.clone()).await;
    let creation = app.create_session(h.launch.clone()).await;
    let creation_rejected = creation.is_err();
    // Clean up a buggy admitted launch before asserting the boundary.
    if let Ok(session) = creation {
        tests::eventually(|| lookups.load(Ordering::SeqCst) != 0).await;
        app.stop_session(session.id).await.unwrap();
    }
    assert!(
        validation.is_err(),
        "unreferenced encrypted orphan must fail fresh validation"
    );
    assert!(
        creation_rejected,
        "unreferenced encrypted orphan must fail fresh create"
    );
    assert_eq!(lookups.load(Ordering::SeqCst), 0);
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.audio.open_ports(), 0);

    app.save_provider_config(SaveProviderRequest {
        config: saved(id),
        credential: CredentialChange::Replace {
            api_key: "synthetic-orphan-key".into(),
        },
    })
    .await
    .unwrap();
    assert!(app.validate_session_launch(h.launch.clone()).await.is_ok());
    assert_eq!(lookups.load(Ordering::SeqCst), 0);
    let session = app.create_session(h.launch.clone()).await.unwrap();
    tests::eventually(|| {
        app.store
            .get_session_lifecycle(session.id)
            .unwrap()
            .unwrap()
            .status
            == "failed"
    })
    .await;
    let terminal = app.stop_session(session.id).await.unwrap();
    assert_eq!(terminal.status, crate::session::SessionStatus::Failed);
    assert_eq!(lookups.load(Ordering::SeqCst), 1);
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(h.audio.open_ports(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn entered_cancelled_delete_retains_admission_and_process_owner_until_native_cleanup() {
    for cancel_caller in [false, true] {
        use fs2::FileExt;
        use std::{fs::OpenOptions, time::Duration};
        let dir = tempfile::tempdir().unwrap();
        let h = tests::fixture();
        let secrets = Arc::new(SecretStore::for_tests(dir.path()).unwrap());
        let mut app = tests::injected(dir.path(), &h, secrets.clone());
        let id = h.saved.id();
        app.save_provider_config(SaveProviderRequest {
            config: saved(id),
            credential: CredentialChange::Replace {
                api_key: "synthetic-prior".into(),
            },
        })
        .await
        .unwrap();
        app.config_io = ConfigIo::with_runtime_for_tests(
            ConfigStore::new(dir.path(), secrets.clone()).unwrap(),
            Arc::new(tokio::sync::Semaphore::new(2)),
            Duration::from_millis(500),
        );
        let secret_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.path().join("secrets.lock"))
            .unwrap();
        secret_lock.lock_exclusive().unwrap();
        let owner = app.clone();
        let delete = tokio::spawn(async move { owner.delete_provider_config(id).await });
        // The config disappears before secret cleanup attempts its blocked lock.
        tests::eventually(|| {
            std::fs::read_to_string(dir.path().join("provider-configs.json"))
                .ok()
                .is_some_and(|v| v.trim() == "[]")
        })
        .await;
        let owner = app.clone();
        let launch = h.launch.clone();
        let create = tokio::spawn(async move { owner.create_session(launch).await });
        if cancel_caller {
            delete.abort();
            assert!(delete.await.unwrap_err().is_cancelled());
        } else {
            let result = delete.await.unwrap();
            assert!(result.err().unwrap().to_string().contains("timed out"));
        }
        assert!(!create.is_finished());
        assert_eq!(h.audio.inventory_calls(), 0);
        drop(app);
        assert!(AppState::new(dir.path().to_path_buf()).is_err());
        // Release even if a later assertion fails; no leaked native latch.
        secret_lock.unlock().unwrap();
        assert!(create.await.unwrap().is_err());
        assert_eq!(h.audio.inventory_calls(), 0);
        assert_eq!(h.audio.open_calls(), 0);
        tests::eventually(|| AppState::new(dir.path().to_path_buf()).is_ok()).await;
        assert!(!secrets.contains(SECRET_SERVICE, &secret_key(id)).unwrap());
    }
}
