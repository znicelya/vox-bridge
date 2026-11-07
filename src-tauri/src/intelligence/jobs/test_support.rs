//! Test construction only: replace an occupied handler before the first start.
use super::*;
pub(crate) fn replace_pre_start_handler(
    scheduler: &JobScheduler,
    kind: JobKind,
    handler: Arc<dyn JobHandler>,
) -> AppResult<()> {
    let mut state = scheduler.state.lock().unwrap();
    if state.frozen || state.generation.is_some() || !state.handlers.contains_key(&kind) {
        return Err(error(IntelligenceErrorCode::Configuration));
    }
    state.handlers.insert(kind, handler);
    Ok(())
}
