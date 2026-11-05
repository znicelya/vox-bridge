pub mod app;
pub mod audio;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
pub mod events;
pub mod export;
pub mod session;
pub mod storage;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|error| {
                Box::<dyn std::error::Error>::from(format!(
                    "failed to resolve app data directory: {error}"
                ))
            })?;
            let app_state = app::AppState::new(data_dir).map_err(|error| {
                Box::<dyn std::error::Error>::from(format!(
                    "failed to initialize application state: {error}"
                ))
            })?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_session,
            commands::stop_session,
            commands::list_sessions,
            commands::get_session,
            commands::delete_session,
            commands::subscribe_session,
            commands::export_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
