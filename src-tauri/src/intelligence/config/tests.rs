use super::*;
use crate::{app::CredentialChange, intelligence::test_support::*};

#[tokio::test]
async fn profile_round_trip_never_returns_saved_key_and_defaults_settings() {
    let h = IntelligenceHarness::new();
    let profile = test_llm_profile();
    let view = h
        .config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-model-key".into(),
            },
        })
        .await
        .unwrap();
    assert!(view.credential_configured);
    assert!(
        !serde_json::to_string(&view)
            .unwrap()
            .contains("synthetic-model-key")
    );
    assert!(
        !std::fs::read_to_string(h.config.path())
            .unwrap()
            .contains("synthetic-model-key")
    );
    assert_eq!(h.config.load(profile.id).await.unwrap().profile, profile);
    assert_eq!(h.config.list().await.unwrap(), vec![view]);
    assert!(h.config.settings().await.unwrap().automatic_summary);
}

async fn save(
    h: &IntelligenceHarness,
    profile: ModelProfile,
    credential: CredentialChange,
) -> ModelProfileView {
    h.config
        .save(SaveModelProfile {
            profile,
            credential,
        })
        .await
        .unwrap()
}
fn edit_reference(h: &IntelligenceHarness, reference: serde_json::Value) {
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.config.path()).unwrap()).unwrap();
    doc["profiles"][0]["credential_reference"] = reference;
    std::fs::write(h.config.path(), serde_json::to_vec(&doc).unwrap()).unwrap();
}
#[tokio::test]
async fn clear_cleanup_failure_and_keep_cannot_resurrect_an_orphan() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-old".into(),
        },
    )
    .await;
    h.io.fail_next_for_tests("delete_secret");
    assert!(
        h.config
            .save(SaveModelProfile {
                profile: p.clone(),
                credential: CredentialChange::Clear
            })
            .await
            .is_err()
    );
    assert!(!h.config.load(p.id).await.unwrap().credential_configured);
    assert!(
        !save(&h, p.clone(), CredentialChange::Keep)
            .await
            .credential_configured
    );
    assert!(h.config.resolve_credentials(p.id).await.is_err());
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-new".into(),
        },
    )
    .await;
    assert!(h.config.resolve_credentials(p.id).await.is_ok());
}
#[tokio::test]
async fn replace_restores_encrypted_before_image_when_profile_write_fails() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-old".into(),
        },
    )
    .await;
    let before = std::fs::read(h.temp_dir.path().join("secrets.json")).unwrap();
    h.io.fail_next_for_tests("save_service_config");
    let mut changed = p.clone();
    changed.name = "Changed".into();
    assert!(
        h.config
            .save(SaveModelProfile {
                profile: changed,
                credential: CredentialChange::Replace {
                    api_key: "synthetic-new".into()
                }
            })
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(h.temp_dir.path().join("secrets.json")).unwrap(),
        before
    );
    assert_eq!(h.config.load(p.id).await.unwrap().profile, p);
}
#[tokio::test]
async fn missing_foreign_and_malformed_reference_are_gated_before_secret_access() {
    for reference in [
        serde_json::Value::Null,
        serde_json::json!("garbage"),
        serde_json::json!(format!(
            "com.voxbridge.intelligence/{}/api-key",
            Uuid::new_v4()
        )),
    ] {
        let h = IntelligenceHarness::new();
        let p = test_llm_profile();
        save(
            &h,
            p.clone(),
            CredentialChange::Replace {
                api_key: "synthetic-key".into(),
            },
        )
        .await;
        edit_reference(&h, reference);
        // A secret read would fail; the reference gate must still yield a safe unconfigured view.
        std::fs::write(h.temp_dir.path().join("secrets.json"), "broken").unwrap();
        assert!(!h.config.load(p.id).await.unwrap().credential_configured);
        assert!(h.config.resolve_credentials(p.id).await.is_err());
        assert!(
            !save(&h, p, CredentialChange::Keep)
                .await
                .credential_configured
        );
    }
}
#[tokio::test]
async fn profile_validation_rejects_unsafe_urls_and_kind_limits_without_writing() {
    let h = IntelligenceHarness::new();
    let base = test_llm_profile();
    let mut invalid = Vec::new();
    for url in [
        "http://example.com/v1",
        "https://user:pass@example.com",
        "https://example.com/?key=synthetic",
        "https://example.com/#fragment",
        "file:///tmp/model",
        "http://127.0.0.2/v1",
        "http://localhost.evil/v1",
    ] {
        let mut p = base.clone();
        p.base_url = url.into();
        invalid.push(p);
    }
    for timeout in [999, 120001] {
        let mut p = base.clone();
        p.timeout_ms = timeout;
        invalid.push(p);
    }
    for name in ["".into(), "x".repeat(81)] {
        let mut p = base.clone();
        p.name = name;
        invalid.push(p);
    }
    for model in [None, Some("".into()), Some("x".repeat(201))] {
        let mut p = base.clone();
        p.model = model;
        invalid.push(p);
    }
    for ctx in [None, Some(2047), Some(131073)] {
        let mut p = base.clone();
        p.max_context_tokens = ctx;
        invalid.push(p);
    }
    for output in [None, Some(127), Some(16385), Some(2049)] {
        let mut p = base.clone();
        p.max_output_tokens = output;
        invalid.push(p);
    }
    let mut p = base.clone();
    p.dimensions = Some(128);
    invalid.push(p);
    for p in invalid {
        assert!(
            h.config
                .save(SaveModelProfile {
                    profile: p,
                    credential: CredentialChange::Keep
                })
                .await
                .is_err()
        );
    }
    assert!(!h.config.path().exists());
    for url in [
        "http://localhost/v1",
        "http://127.0.0.1/v1",
        "http://[::1]/v1",
        "https://api.example.com/v1",
    ] {
        let mut p = base.clone();
        p.base_url = url.into();
        save(&h, p, CredentialChange::Keep).await;
    }
}
#[tokio::test]
async fn settings_validate_kinds_and_selected_profiles_cannot_change_kind_or_delete() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(&h, p.clone(), CredentialChange::Keep).await;
    let mut settings = h.config.settings().await.unwrap();
    settings.embedding_profile_id = Some(p.id);
    assert!(h.config.save_settings(settings).await.is_err());
    let mut settings = h.config.settings().await.unwrap();
    settings.summary_profile_id = Some(p.id);
    assert_eq!(
        h.config.save_settings(settings.clone()).await.unwrap(),
        settings
    );
    assert!(h.config.delete(p.id).await.is_err());
    let mut changed = p.clone();
    changed.kind = ServiceKind::Embedding;
    changed.max_context_tokens = None;
    changed.max_output_tokens = None;
    assert!(
        h.config
            .save(SaveModelProfile {
                profile: changed,
                credential: CredentialChange::Keep
            })
            .await
            .is_err()
    );
    h.config
        .save_settings(IntelligenceSettings::default())
        .await
        .unwrap();
    h.config.delete(p.id).await.unwrap();
    assert!(h.config.list().await.unwrap().is_empty());
}
#[tokio::test]
async fn snapshot_admission_holds_config_lock_through_sqlite_commit_and_edits_keep_snapshot() {
    use crate::intelligence::store::admit_job;
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(&h, p.clone(), CredentialChange::Keep).await;
    let session = h.session();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let config = h.config.clone();
    let id = p.id;
    let admitted = tokio::spawn(async move {
        config
            .with_profile_snapshot(vec![id], move |tx, profiles| {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap();
                admit_job(
                    tx,
                    crate::intelligence::store::JobDraft {
                        payload: payload(profiles),
                        ..manual(session)
                    },
                )
            })
            .await
    });
    entered_rx.await.unwrap();
    let config = h.config.clone();
    let delete = tokio::spawn(async move { config.delete(id).await });
    tokio::task::yield_now().await;
    assert!(!delete.is_finished());
    release_tx.send(()).unwrap();
    let job = admitted.await.unwrap().unwrap().view().id;
    assert!(matches!(
        delete.await.unwrap(),
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    let mut changed = p.clone();
    changed.model = Some("future-model".into());
    save(&h, changed, CredentialChange::Keep).await;
    let stored: String = h
        .store
        .transaction(|tx| {
            Ok(tx.query_row(
                "SELECT payload_json FROM intelligence_jobs WHERE id=?1",
                [job.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<JobPayload>(&stored)
            .unwrap()
            .profiles,
        vec![p]
    );
}

#[tokio::test]
async fn missing_invalid_utf8_and_corrupt_encrypted_credentials_are_local_errors() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-key".into(),
        },
    )
    .await;
    for bytes in [vec![255], b"not allowed".to_vec(), Vec::new()] {
        h.secrets.set(SERVICE, &key(p.id), &bytes).unwrap();
        assert!(matches!(
            h.config.resolve_credentials(p.id).await,
            Err(AppError::Intelligence(IntelligenceErrorCode::Configuration))
        ));
    }
    h.secrets.delete(SERVICE, &key(p.id)).unwrap();
    assert!(!h.config.load(p.id).await.unwrap().credential_configured);
    assert!(h.config.resolve_credentials(p.id).await.is_err());
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-key".into(),
        },
    )
    .await;
    let path = h.temp_dir.path().join("secrets.json");
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for value in doc.as_object_mut().unwrap().values_mut() {
        value["ciphertext"] = serde_json::json!("invalid ciphertext synthetic-body");
    }
    std::fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
    let error = h.config.resolve_credentials(p.id).await.err().unwrap();
    assert_eq!(
        error.to_string(),
        "Service configuration is invalid or incomplete."
    );
}
#[tokio::test]
async fn service_file_reopens_and_preserves_legacy_provider_fields_byte_for_byte() {
    use crate::config::{EmbeddingConfig, LlmConfig, ProviderConfig, TavilyConfig};
    let h = IntelligenceHarness::new();
    let mut legacy = ProviderConfig::new(
        Uuid::new_v4(),
        "volcengine",
        "https://example.com",
        None,
        None,
        vec!["en".into()],
        None,
    )
    .unwrap();
    legacy.llm = Some(LlmConfig {
        model: "old-llm".into(),
        resource_id: Some("resource".into()),
    });
    legacy.embedding = Some(EmbeddingConfig {
        model: "old-embedding".into(),
        dimensions: Some(256),
    });
    legacy.tavily = Some(TavilyConfig {
        endpoint: "https://example.com/search".into(),
        search_depth: Some("basic".into()),
    });
    h.io.run(move |tx| tx.save_provider_config(legacy))
        .await
        .unwrap();
    let legacy_path = h.temp_dir.path().join("provider-configs.json");
    let before = std::fs::read(&legacy_path).unwrap();
    let p = test_llm_profile();
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-key".into(),
        },
    )
    .await;
    for _ in 0..2 {
        let reopened = ServiceConfigStore::new(h.io.clone(), h.store.clone());
        assert_eq!(reopened.load(p.id).await.unwrap().profile, p);
    }
    assert_eq!(std::fs::read(legacy_path).unwrap(), before);
    let config_before = std::fs::read(h.config.path()).unwrap();
    let mut doc: serde_json::Value = serde_json::from_slice(&config_before).unwrap();
    doc["version"] = serde_json::json!(2);
    let future = serde_json::to_vec(&doc).unwrap();
    std::fs::write(h.config.path(), &future).unwrap();
    assert!(
        h.config
            .save(SaveModelProfile {
                profile: p,
                credential: CredentialChange::Clear
            })
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(h.config.path()).unwrap(), future);
}
#[tokio::test]
async fn embedding_and_tavily_validate_fields_and_tavily_defaults_endpoint() {
    let h = IntelligenceHarness::new();
    let mut p = test_llm_profile();
    p.kind = ServiceKind::Embedding;
    p.max_context_tokens = None;
    p.max_output_tokens = None;
    for dimension in [None, Some(128), Some(4096)] {
        p.dimensions = dimension;
        save(&h, p.clone(), CredentialChange::Keep).await;
    }
    for dimension in [127, 4097] {
        p.dimensions = Some(dimension);
        assert!(
            h.config
                .save(SaveModelProfile {
                    profile: p.clone(),
                    credential: CredentialChange::Keep
                })
                .await
                .is_err()
        );
    }
    p.id = Uuid::new_v4();
    p.kind = ServiceKind::Tavily;
    p.model = None;
    p.dimensions = None;
    p.base_url = String::new();
    let view = save(&h, p.clone(), CredentialChange::Keep).await;
    assert_eq!(view.profile.base_url, "https://api.tavily.com/search");
    for field in [
        "model",
        "max_context_tokens",
        "max_output_tokens",
        "dimensions",
    ] {
        let mut value = serde_json::to_value(&view.profile).unwrap();
        value[field] = if field == "model" {
            serde_json::json!("model")
        } else {
            serde_json::json!(2048)
        };
        assert!(
            h.config
                .save(SaveModelProfile {
                    profile: serde_json::from_value(value).unwrap(),
                    credential: CredentialChange::Keep
                })
                .await
                .is_err()
        );
    }
}
#[test]
fn service_endpoint_preserves_base_path_and_appends_once() {
    let mut p = test_llm_profile();
    for (base, expected) in [
        (
            "https://example.com/v1",
            "https://example.com/v1/chat/completions",
        ),
        (
            "https://example.com/v1/",
            "https://example.com/v1/chat/completions",
        ),
        (
            "https://example.com/v1/chat/completions/",
            "https://example.com/v1/chat/completions",
        ),
        (
            "https://example.com/custom%2Fv1",
            "https://example.com/custom%2Fv1/chat/completions",
        ),
    ] {
        p.base_url = base.into();
        assert_eq!(endpoint(&p).unwrap().as_str(), expected);
    }
    p.kind = ServiceKind::Embedding;
    p.base_url = "https://example.com/v1/embeddings".into();
    assert_eq!(
        endpoint(&p).unwrap().as_str(),
        "https://example.com/v1/embeddings"
    );
}
#[tokio::test]
async fn compensation_failure_is_safe_and_clear_write_failure_preserves_reference() {
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(
        &h,
        p.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-before".into(),
        },
    )
    .await;
    h.io.fail_next_for_tests("save_service_config");
    assert!(
        h.config
            .save(SaveModelProfile {
                profile: p.clone(),
                credential: CredentialChange::Clear
            })
            .await
            .is_err()
    );
    assert!(h.config.load(p.id).await.unwrap().credential_configured);
    h.io.fail_next_for_tests("save_service_config");
    h.io.fail_next_for_tests("restore_secret");
    let error = h
        .config
        .save(SaveModelProfile {
            profile: p.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-after".into(),
            },
        })
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "Service configuration is invalid or incomplete."
    );
    // Partial failure is not success: encrypted replacement may remain, but no key
    // or native error is exposed. A later Clear still gates all credential access.
    save(&h, p.clone(), CredentialChange::Clear).await;
    assert!(h.config.resolve_credentials(p.id).await.is_err());
}

#[tokio::test]
async fn profile_file_count_and_bytes_are_bounded_without_partial_mutation() {
    let h = IntelligenceHarness::new();
    let base = test_llm_profile();
    save(&h, base.clone(), CredentialChange::Keep).await;
    let profiles: Vec<_> = (0..128)
        .map(|_| {
            let mut p = base.clone();
            p.id = Uuid::new_v4();
            serde_json::json!({"profile":p,"credential_reference":null})
        })
        .collect();
    let doc = serde_json::json!({"version":1,"profiles":profiles,"settings":IntelligenceSettings::default()});
    let before = serde_json::to_vec(&doc).unwrap();
    std::fs::write(h.config.path(), &before).unwrap();
    assert_eq!(h.config.list().await.unwrap().len(), 128);
    assert!(matches!(
        h.config
            .save(SaveModelProfile {
                profile: base.clone(),
                credential: CredentialChange::Keep
            })
            .await,
        Err(AppError::Intelligence(IntelligenceErrorCode::ResourceLimit))
    ));
    assert_eq!(std::fs::read(h.config.path()).unwrap(), before);
    let oversized = vec![b' '; 1024 * 1024 + 1];
    std::fs::write(h.config.path(), &oversized).unwrap();
    assert!(h.config.list().await.is_err());
    assert!(
        h.config
            .save(SaveModelProfile {
                profile: base,
                credential: CredentialChange::Clear
            })
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::metadata(h.config.path()).unwrap().len(),
        1024 * 1024 + 1
    );
}
#[tokio::test]
async fn terminal_jobs_release_profile_use_and_corrupt_unfinished_jobs_fail_closed() {
    use crate::intelligence::store::{JobDraft, admit_job};
    let h = IntelligenceHarness::new();
    let p = test_llm_profile();
    save(&h, p.clone(), CredentialChange::Keep).await;
    let session = h.session();
    h.config
        .with_profile_snapshot(vec![p.id], move |tx, profiles| {
            admit_job(
                tx,
                JobDraft {
                    payload: payload(profiles),
                    ..manual(session)
                },
            )
        })
        .await
        .unwrap();
    let mut changed = p.clone();
    changed.kind = ServiceKind::Embedding;
    changed.max_context_tokens = None;
    changed.max_output_tokens = None;
    assert!(matches!(
        h.config
            .save(SaveModelProfile {
                profile: changed,
                credential: CredentialChange::Keep
            })
            .await,
        Err(AppError::Intelligence(IntelligenceErrorCode::Busy))
    ));
    h.store
        .transaction(|tx| {
            tx.execute("UPDATE intelligence_jobs SET payload_json='corrupt'", [])?;
            Ok(())
        })
        .unwrap();
    assert!(h.config.delete(p.id).await.is_err());
    h.store
        .transaction(|tx| {
            tx.execute("UPDATE intelligence_jobs SET status='succeeded'", [])?;
            Ok(())
        })
        .unwrap();
    h.config.delete(p.id).await.unwrap();
    assert_eq!(h.store.list_jobs(None, 200).unwrap().rows.len(), 1);
}

#[tokio::test]
async fn intelligence_probe_revision_fences_edits_credential_changes_and_deletion() {
    for edit in 0..4 {
        let h = IntelligenceHarness::new();
        let profile = test_llm_profile();
        h.config
            .save(SaveModelProfile {
                profile: profile.clone(),
                credential: CredentialChange::Replace {
                    api_key: "synthetic-first".into(),
                },
            })
            .await
            .unwrap();
        let (snapshot, revision, _) = h.config.probe_snapshot(profile.id).await.unwrap();
        let mut changed = profile.clone();
        if edit == 0 {
            changed.name = "Edited".into();
        }
        if edit == 3 {
            h.config.delete(profile.id).await.unwrap();
        } else {
            h.config
                .save(SaveModelProfile {
                    profile: changed,
                    credential: match edit {
                        1 => CredentialChange::Replace {
                            api_key: "synthetic-second".into(),
                        },
                        2 => CredentialChange::Clear,
                        _ => CredentialChange::Keep,
                    },
                })
                .await
                .unwrap();
        }
        h.config
            .persist_probe(
                snapshot,
                revision,
                ConnectionProbe {
                    checked_at_ms: 1,
                    ok: true,
                    latency_ms: Some(1),
                    error: None,
                },
                None,
            )
            .await
            .unwrap();
        assert!(
            h.store
                .transaction(|tx| super::super::store::probe(tx, profile.id))
                .unwrap()
                .is_none()
        );
    }
}
#[tokio::test]
async fn intelligence_scoped_config_retains_owner_after_entered_observer_cancellation() {
    let h = IntelligenceHarness::new();
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    let scoped = h.config.scoped(owner);
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        scoped
            .io
            .run(move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
            .await
    });
    entered_rx.await.unwrap();
    task.abort();
    let _ = task.await;
    let retained = weak.upgrade().is_some();
    release_tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    assert!(retained, "entered config work lost scoped process owner");
}

#[tokio::test]
async fn intelligence_embedding_probe_cannot_change_an_admitted_dimension_identity() {
    let h = IntelligenceHarness::new();
    let mut profile = test_llm_profile();
    profile.kind = ServiceKind::Embedding;
    profile.max_context_tokens = None;
    profile.max_output_tokens = None;
    h.config
        .save(SaveModelProfile {
            profile: profile.clone(),
            credential: CredentialChange::Replace {
                api_key: "synthetic-embedding".into(),
            },
        })
        .await
        .unwrap();
    let (snapshot, revision, _) = h.config.probe_snapshot(profile.id).await.unwrap();
    let session = h.session();
    let mut draft = manual(session);
    draft.payload = payload(vec![profile.clone()]);
    h.store
        .transaction(|tx| crate::intelligence::store::admit_job(tx, draft))
        .unwrap();
    h.config
        .persist_probe(
            snapshot,
            revision,
            ConnectionProbe {
                checked_at_ms: 1,
                ok: true,
                latency_ms: Some(1),
                error: None,
            },
            Some(128),
        )
        .await
        .unwrap();
    assert_eq!(
        h.config.load(profile.id).await.unwrap().profile.dimensions,
        None
    );
}
