//! Controlled transport with the real App summary/config/job composition.
use super::*;
use crate::intelligence::{test_support::RecordingHttp, types::*};

pub(crate) fn with_runtime_http(
    path: &Path,
    runtime: &crate::session::test_support::RuntimeHarness,
    http: Arc<RecordingHttp>,
) -> AppState {
    let secrets = Arc::new(SecretStore::for_tests(path).unwrap());
    AppState::compose_with_embeddings(
        path.into(),
        AppState::lock_data_dir(path).unwrap(),
        secrets.clone(),
        runtime.audio.clone(),
        Arc::new(ProviderFactory::new(secrets).with_test_provider(runtime.provider.clone())),
        Arc::new(UnavailableDriverOperations),
        Arc::new(crate::knowledge::parser::ProcessDocumentParser::new().unwrap()),
        None,
        Some(http),
        Some(Arc::new(Semaphore::new(2))),
    )
    .unwrap()
}

pub(crate) async fn enable_automatic(app: &AppState) -> ModelProfile {
    let profile = crate::intelligence::test_support::test_llm_profile();
    app.save_model_profile(SaveModelProfile {
        profile: profile.clone(),
        credential: CredentialChange::Replace {
            api_key: "synthetic-summary-key".into(),
        },
    })
    .await
    .unwrap();
    app.save_intelligence_settings(IntelligenceSettings {
        automatic_summary: true,
        summary_profile_id: Some(profile.id),
        ..Default::default()
    })
    .await
    .unwrap();
    profile
}
