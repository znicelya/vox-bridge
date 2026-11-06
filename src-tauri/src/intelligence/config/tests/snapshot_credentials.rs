use super::*;
use crate::config::{ConfigIo, ConfigStore};
use std::time::Duration;
use tokio::sync::Semaphore;

fn changes(profile: &ModelProfile) -> Vec<(&'static str, ModelProfile)> {
    let changes: [(&str, fn(&mut ModelProfile)); 9] = [
        ("id", |p| p.id = Uuid::new_v4()),
        ("kind", |p| p.kind = ServiceKind::Embedding),
        ("name", |p| p.name = "Different label".into()),
        ("base_url", |p| {
            p.base_url = "https://example.invalid/v1".into()
        }),
        ("model", |p| p.model = Some("another-model".into())),
        ("timeout_ms", |p| p.timeout_ms = 2000),
        ("max_context_tokens", |p| p.max_context_tokens = Some(8192)),
        ("max_output_tokens", |p| p.max_output_tokens = Some(1024)),
        ("dimensions", |p| p.dimensions = Some(128)),
    ];
    changes
        .into_iter()
        .map(|(name, change)| {
            let mut changed = profile.clone();
            change(&mut changed);
            (name, changed)
        })
        .collect()
}
fn configuration<T>(result: &AppResult<T>) -> bool {
    matches!(
        result,
        Err(AppError::Intelligence(IntelligenceErrorCode::Configuration))
    )
}
async fn saved(h: &IntelligenceHarness) -> ModelProfile {
    let profile = test_llm_profile();
    save(
        h,
        profile.clone(),
        CredentialChange::Replace {
            api_key: "synthetic-snapshot".into(),
        },
    )
    .await;
    profile
}

// Catches endpoint/model/dimension reinterpretation and intentionally includes
// non-vector fields: executing a job uses its whole admitted public snapshot.
#[tokio::test]
async fn snapshot_credentials_require_every_admitted_public_field() {
    let h = IntelligenceHarness::new();
    let profile = saved(&h).await;
    assert!(
        h.config
            .resolve_snapshot_credentials(profile.clone())
            .await
            .is_ok()
    );
    let mut accepted = Vec::new();
    for (name, changed) in changes(&profile) {
        if !configuration(&h.config.resolve_snapshot_credentials(changed).await) {
            accepted.push(name);
        }
    }
    assert!(
        accepted.is_empty(),
        "changed fields incorrectly admitted: {accepted:?}"
    );
}

// Catches reading the key first and only then rejecting a mismatched snapshot.
// A valid follow-up must consume the fault; the next valid call then succeeds.
#[tokio::test]
async fn snapshot_credentials_reject_mismatches_before_secret_entry() {
    let h = IntelligenceHarness::new();
    let profile = saved(&h).await;
    for (name, changed) in changes(&profile) {
        h.io.fail_next_for_tests("get_secret");
        assert!(
            configuration(&h.config.resolve_snapshot_credentials(changed).await),
            "mismatch did not fail for {name}"
        );
        assert!(
            configuration(&h.config.resolve_snapshot_credentials(profile.clone()).await),
            "mismatch consumed secret_get fault for {name}"
        );
        assert!(
            h.config
                .resolve_snapshot_credentials(profile.clone())
                .await
                .is_ok()
        );
    }
}

// Existing reference validation must also precede secret entry. Restoring the
// original document leaves the synthetic encrypted key unchanged throughout.
#[tokio::test]
async fn snapshot_credentials_missing_profile_and_invalid_references_never_enter_secret_get() {
    let h = IntelligenceHarness::new();
    let profile = saved(&h).await;
    let original = std::fs::read(h.config.path()).unwrap();
    for invalid in ["missing", "cleared", "foreign", "wrong-key"] {
        let mut doc: serde_json::Value = serde_json::from_slice(&original).unwrap();
        match invalid {
            "missing" => doc["profiles"] = serde_json::json!([]),
            "cleared" => doc["profiles"][0]["credential_reference"] = serde_json::Value::Null,
            "foreign" => {
                doc["profiles"][0]["credential_reference"] =
                    serde_json::json!(reference(Uuid::new_v4()))
            }
            _ => {
                doc["profiles"][0]["credential_reference"] =
                    serde_json::json!(format!("{SERVICE}/{}/other-key", profile.id))
            }
        }
        std::fs::write(h.config.path(), serde_json::to_vec(&doc).unwrap()).unwrap();
        h.io.fail_next_for_tests("get_secret");
        assert!(configuration(
            &h.config.resolve_snapshot_credentials(profile.clone()).await
        ));
        std::fs::write(h.config.path(), &original).unwrap();
        assert!(
            configuration(&h.config.resolve_snapshot_credentials(profile.clone()).await),
            "invalid {invalid} consumed the secret fault"
        );
        assert!(
            h.config
                .resolve_snapshot_credentials(profile.clone())
                .await
                .is_ok()
        );
    }
}

// Catches splitting snapshot validation from key resolution into separate
// ConfigIo runs: the already-queued clear writer would take the intervening slot.
#[tokio::test]
async fn snapshot_credentials_validate_and_read_before_an_already_queued_profile_clear() {
    let mut h = IntelligenceHarness::new();
    let gate = Arc::new(Semaphore::new(1));
    h.io = ConfigIo::with_runtime_for_tests(
        ConfigStore::new(h.temp_dir.path(), h.secrets.clone()).unwrap(),
        gate.clone(),
        Duration::from_secs(10),
    );
    h.config = Arc::new(ServiceConfigStore::new(h.io.clone(), h.store.clone()));
    let profile = saved(&h).await;
    let held = gate.acquire().await.unwrap();
    let mut resolver = Box::pin(h.config.resolve_snapshot_credentials(profile.clone()));
    assert!(futures_util::poll!(resolver.as_mut()).is_pending());
    let mut changed = profile.clone();
    changed.name = "Edited while resolving".into();
    let mut writer = Box::pin(h.config.save(SaveModelProfile {
        profile: changed,
        credential: CredentialChange::Clear,
    }));
    assert!(futures_util::poll!(writer.as_mut()).is_pending());
    drop(held);
    let (resolved, written) = tokio::join!(resolver, writer);
    assert!(
        resolved.is_ok(),
        "queued config mutation interposed between validation and key read"
    );
    assert!(!written.unwrap().credential_configured);
    assert!(configuration(
        &h.config.resolve_snapshot_credentials(profile).await
    ));
}
