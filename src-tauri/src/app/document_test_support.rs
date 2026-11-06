//! Test-only construction utilities; no test methods on the production service.
use super::*;
pub(crate) fn with_parser(
    path: &Path,
    parser: Arc<dyn crate::knowledge::parser::DocumentParser>,
) -> AppState {
    let secrets = Arc::new(SecretStore::for_tests(path).unwrap());
    let fixture = crate::app::tests::fixture();
    AppState::compose(
        path.into(),
        AppState::lock_data_dir(path).unwrap(),
        secrets.clone(),
        fixture.audio.clone(),
        Arc::new(ProviderFactory::new(secrets)),
        Arc::new(UnavailableDriverOperations),
        parser,
    )
    .unwrap()
}
