use super::AppState;

#[test]
fn app_state_initializes_the_event_bus_and_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(state.data_dir(), dir.path());
    assert!(state.event_bus().is_healthy());
}

#[test]
fn app_construction_recovers_only_after_process_ownership_without_opening_services() {
    use crate::storage::{LocalStore, StoredSession};
    let dir = tempfile::tempdir().unwrap();
    let store = LocalStore::open(dir.path()).unwrap();
    let id = uuid::Uuid::new_v4();
    store
        .create_session(&StoredSession {
            id,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    drop(store);
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        state
            .store()
            .get_session_lifecycle(id)
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
    assert!(!dir.path().join("secrets.json").exists());
    assert!(!dir.path().join("secrets.dek.lock").exists());
    assert!(!dir.path().join("provider-configs.json").exists());
}

#[test]
fn second_process_owner_cannot_touch_database_or_config() {
    let dir = tempfile::tempdir().unwrap();
    let first = AppState::new(dir.path().to_path_buf()).unwrap();
    let id = uuid::Uuid::new_v4();
    first
        .store()
        .create_session(&crate::storage::StoredSession {
            id,
            started_at: 1,
            mode: "live".into(),
            direction: "source_to_target".into(),
            source_language: "en".into(),
            target_language: "zh".into(),
        })
        .unwrap();
    let result = AppState::new(dir.path().to_path_buf());
    assert!(
        result.is_err(),
        "second owner must fail before recovery/migration"
    );
    assert_eq!(
        first
            .store()
            .get_session_lifecycle(id)
            .unwrap()
            .unwrap()
            .status,
        "created"
    );
    assert!(!dir.path().join("provider-configs.json").exists());
}

use crate::{
    audio::PcmSpec,
    config::SecretStore,
    domain::launch::DeviceSelection,
    providers::{
        ProviderEvent,
        factory::ProviderFactory,
        fixture::{FixtureScript, FixtureStep},
    },
    session::test_support::{RuntimeHarness, metadata},
};
use std::{sync::Arc, time::Duration};

pub(super) fn fixture() -> RuntimeHarness {
    RuntimeHarness::live(
        FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap(),
    )
    .unwrap()
}
pub(super) fn injected(
    dir: &std::path::Path,
    h: &RuntimeHarness,
    secrets: Arc<SecretStore>,
) -> AppState {
    AppState::with_dependencies(
        dir.to_path_buf(),
        secrets.clone(),
        h.audio.clone(),
        Arc::new(ProviderFactory::new(secrets).with_test_provider(h.provider.clone())),
    )
    .unwrap()
}
#[tokio::test]
async fn stale_endpoint_preflight_never_opens_devices_or_provider() {
    let h = fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.save_provider_config(super::SaveProviderRequest {
        config: h.saved.clone(),
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    let mut launch = h.launch.clone();
    launch.devices = DeviceSelection::Live {
        input_id: "stale-id".into(),
        m1_output_id: "m1-render".into(),
    };
    assert!(app.validate_session_launch(launch).await.is_err());
    assert_eq!(h.provider.connect_counts(), [0, 0]);
    assert_eq!(h.audio.open_ports(), 0);
}
#[tokio::test]
async fn close_cancels_pending_creation_without_opening_resources_and_allows_new_window() {
    let h = fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(super::SaveProviderRequest {
        config: h.saved.clone(),
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    let (entered, release) = h.audio.block_inventory();
    let owner = app.clone();
    let launch = h.launch.clone();
    let create = tokio::spawn(async move { owner.create_session(launch).await });
    entered.await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(2), app.close_all()).await;
    let _ = release.send(());
    closed.unwrap().unwrap();
    assert!(create.await.unwrap().is_err());
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.provider.connect_counts(), [0, 0]);
    app.open_window().unwrap();
    let record = app.create_session(h.launch.clone()).await.unwrap();
    app.stop_session(record.id).await.unwrap();
}

#[tokio::test]
async fn a_completed_window_close_fences_old_commands_until_host_opens_new_window() {
    let h = fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    app.save_provider_config(super::SaveProviderRequest {
        config: h.saved.clone(),
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    app.close_all().await.unwrap();
    assert!(
        app.create_session(h.launch.clone()).await.is_err(),
        "closing window cannot resurrect runtime"
    );
}

pub(super) async fn eventually(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn configured_launch_owns_ports_persists_finals_and_keeps_its_snapshot() {
    use crate::session::{SessionStatus, test_support::final_text};
    let h = RuntimeHarness::live(
        FixtureScript::with_output_audio(
            PcmSpec::pcm16_mono(24_000),
            [
                FixtureStep::Emit(ProviderEvent::SessionStarted {
                    metadata: metadata(),
                }),
                FixtureStep::Emit(ProviderEvent::TranslationFinal {
                    metadata: metadata(),
                    text: final_text("zh", "durable final"),
                }),
                FixtureStep::WaitForFinish,
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    let saved = h
        .saved
        .clone()
        .with_display_name("My saved profile".into())
        .unwrap();
    app.save_provider_config(super::SaveProviderRequest {
        config: saved,
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    let validation = app.validate_session_launch(h.launch.clone()).await.unwrap();
    assert_eq!(validation.context.provider_name, "My saved profile");
    assert_eq!(validation.input_audio, PcmSpec::pcm16_mono(16_000));
    assert_eq!(validation.output_audio, PcmSpec::pcm16_mono(24_000));
    let record = app.create_session(h.launch.clone()).await.unwrap();
    assert_eq!(record.status, SessionStatus::Starting);
    eventually(|| {
        app.get_session_transcript(record.id, None, None)
            .unwrap()
            .rows
            .len()
            == 1
    })
    .await;
    assert_eq!(h.audio.open_ports(), 1);
    let calls = h.audio.inventory_calls();
    assert!(app.create_session(h.launch.clone()).await.is_err());
    assert_eq!(h.audio.inventory_calls(), calls);
    assert_eq!(h.provider.connect_counts(), [1, 0]);
    assert!(app.delete_provider_config(h.saved.id()).await.is_err());
    app.save_provider_config(super::SaveProviderRequest {
        config: h
            .saved
            .clone()
            .with_display_name("Future name".into())
            .unwrap(),
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    assert_eq!(
        app.get_session(record.id)
            .await
            .unwrap()
            .unwrap()
            .context
            .unwrap()
            .provider_name,
        "My saved profile"
    );
    let terminal = app.stop_session(record.id).await.unwrap();
    assert_eq!(terminal.status, SessionStatus::Completed);
    assert_eq!(h.audio.open_ports(), 0);
    assert_eq!(
        app.get_session_transcript(record.id, None, None)
            .unwrap()
            .rows[0]
            .text,
        "durable final"
    );
    assert!(
        !app.get_session_diagnostics(record.id, None, None)
            .unwrap()
            .rows
            .is_empty()
    );
    app.delete_provider_config(h.saved.id()).await.unwrap();
}

#[tokio::test]
async fn disabled_unknown_and_wrong_role_launches_fail_locally() {
    use crate::config::ProviderConfig;
    let h = fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    );
    for provider in ["aliyun", "palabra", "unknown-provider"] {
        let config = ProviderConfig::new(
            h.saved.id(),
            provider,
            "wss://example.invalid",
            None,
            None,
            vec![],
            None,
        );
        // Generic providers use HTTPS; real websocket placeholders accept WSS.
        let config = config.unwrap_or_else(|_| {
            ProviderConfig::new(
                h.saved.id(),
                provider,
                "https://example.invalid",
                None,
                None,
                vec![],
                None,
            )
            .unwrap()
        });
        app.save_provider_config(super::SaveProviderRequest {
            config,
            credential: super::CredentialChange::Keep,
        })
        .await
        .unwrap();
        assert!(!app.provider_capabilities(h.saved.id()).await.unwrap().ready);
        assert!(app.create_session(h.launch.clone()).await.is_err());
    }
    app.save_provider_config(super::SaveProviderRequest {
        config: h.saved.clone(),
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    app.validate_session_launch(h.launch.clone()).await.unwrap();
    h.audio.set_role(
        "m1-render",
        crate::audio::platform::inventory::EndpointRole::PhysicalOutput,
    );
    assert!(app.create_session(h.launch.clone()).await.is_err());
    assert_eq!(h.audio.open_calls(), 0);
    assert_eq!(h.provider.connect_counts(), [0, 0]);
}

#[tokio::test]
async fn reserved_creation_wins_delete_race_until_its_session_cleanup() {
    let h = fixture();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(injected(
        dir.path(),
        &h,
        Arc::new(SecretStore::for_tests(dir.path()).unwrap()),
    ));
    app.save_provider_config(super::SaveProviderRequest {
        config: h.saved.clone(),
        credential: super::CredentialChange::Keep,
    })
    .await
    .unwrap();
    let (entered, release) = h.audio.block_inventory();
    let owner = app.clone();
    let launch = h.launch.clone();
    let create = tokio::spawn(async move { owner.create_session(launch).await });
    entered.await.unwrap();
    let owner = app.clone();
    let id = h.saved.id();
    let delete = tokio::spawn(async move { owner.delete_provider_config(id).await });
    tokio::task::yield_now().await;
    assert!(!delete.is_finished());
    let _ = release.send(());
    let session = create.await.unwrap().unwrap();
    assert!(delete.await.unwrap().is_err());
    app.stop_session(session.id).await.unwrap();
    app.delete_provider_config(id).await.unwrap();
}
