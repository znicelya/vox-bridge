//! Explicit driver actions; the runtime inventory remains the readiness authority.
#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod lease_tests;
#[cfg(test)]
mod runner_tests;
#[cfg(test)]
mod tests;

use crate::{
    app::AppState,
    audio::platform::{DriverStatus, driver::DriverOperation},
};
use tauri::State;
#[tauri::command]
pub async fn manage_audio_driver(
    state: State<'_, AppState>,
    operation: DriverOperation,
) -> Result<DriverStatus, String> {
    state
        .manage_audio_driver(operation)
        .await
        .map_err(super::command_error)
}
#[cfg(test)]
mod script_tests;
