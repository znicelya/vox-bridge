use super::*;
use serde_json::{Value, json};
fn request(command: &str, body: Value) -> tauri::webview::InvokeRequest {
    tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap(),
        body: tauri::ipc::InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: tauri::test::INVOKE_KEY.into(),
    }
}
#[test]
fn intelligence_commands_dispatch_all_thirteen_and_project_injected_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(crate::config::SecretStore::for_tests(dir.path()).unwrap());
    let fixture = crate::session::test_support::RuntimeHarness::live(
        crate::providers::fixture::FixtureScript::with_output_audio(
            crate::audio::PcmSpec::pcm16_mono(24000),
            [],
        )
        .unwrap(),
    )
    .unwrap();
    let state = AppState::with_dependencies(
        dir.path().to_path_buf(),
        secrets.clone(),
        fixture.audio.clone(),
        Arc::new(crate::providers::factory::ProviderFactory::new(secrets)),
    )
    .unwrap();
    let app = tauri::test::mock_builder()
        .manage(state)
        .invoke_handler(handle)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let call = |cmd: &str, args: Value| {
        tauri::test::get_ipc_response(&window, request(cmd, args))
            .map(|v| v.deserialize::<Value>().unwrap())
    };
    assert_eq!(
        call("get_managed_data_location", json!({})).unwrap(),
        json!({"data_dir":dir.path()})
    );
    assert_eq!(call("list_model_profiles", json!({})).unwrap(), json!([]));
    let settings = call("get_intelligence_settings", json!({})).unwrap();
    assert_eq!(settings["automatic_summary"], true);
    assert_eq!(
        call("save_intelligence_settings", json!({"settings":settings})).unwrap(),
        settings
    );
    let profile = crate::intelligence::test_support::test_llm_profile();
    let id = profile.id;
    let view = call(
        "save_model_profile",
        json!({"request":{"profile":profile,"credential":{"action":"clear"}}}),
    )
    .unwrap();
    assert_eq!(view["credential_configured"], false);
    assert_eq!(
        call("test_model_profile", json!({"profileId":id})).unwrap()["error"],
        "configuration"
    );
    assert_eq!(
        call(
            "test_translation_provider",
            json!({"profileId":Uuid::new_v4()})
        )
        .unwrap()["error"],
        "configuration"
    );
    assert_eq!(
        call("get_job", json!({"jobId":Uuid::new_v4()})).unwrap(),
        Value::Null
    );
    assert_eq!(
        call("list_jobs", json!({"after":null,"limit":20})).unwrap(),
        json!({"rows":[],"next_cursor":null})
    );
    let error = call("cancel_job", json!({"jobId":Uuid::new_v4()})).unwrap_err();
    assert!(
        error
            .as_str()
            .unwrap()
            .starts_with("[INTELLIGENCE_ERROR][configuration] ")
    );
    let ticket = call("subscribe_intelligence", json!({"subscriberId":"fixture"})).unwrap();
    assert!(
        ticket["event_name"]
            .as_str()
            .unwrap()
            .starts_with("voxbridge://intelligence-subscription/")
    );
    assert_eq!(
        call(
            "unsubscribe_intelligence",
            json!({"subscriptionId":ticket["id"]})
        )
        .unwrap(),
        Value::Null
    );
    assert_eq!(
        call("delete_model_profile", json!({"profileId":id})).unwrap(),
        Value::Null
    );
    assert!(call("resolve_credentials", json!({"profileId":id})).is_err());
}
