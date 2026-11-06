pub mod app;
pub mod audio;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
pub mod events;
pub mod export;
pub mod intelligence;
pub mod providers;
pub mod session;
pub mod storage;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let exit_ready = Arc::new(AtomicBool::new(false));
    let app = tauri::Builder::default()
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
            tauri::async_runtime::block_on(app_state.start_intelligence())?;
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(|invoke| {
            if commands::intelligence::COMMANDS.contains(&invoke.message.command()) {
                return commands::intelligence::handle(invoke);
            }
            let runtime_handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
                #[cfg(debug_assertions)]
                commands::dev_audio_loopback,
                commands::list_audio_endpoints,
                commands::audio_driver_status,
                commands::driver::manage_audio_driver,
                commands::list_provider_configs,
                commands::save_provider_config,
                commands::delete_provider_config,
                commands::provider_capabilities,
                commands::validate_session_launch,
                commands::resume_session_direction,
                commands::get_session_transcript,
                commands::get_session_diagnostics,
                commands::unsubscribe_session,
                commands::create_session,
                commands::stop_session,
                commands::list_sessions,
                commands::get_session,
                commands::delete_session,
                commands::subscribe_session,
                commands::export_session,
            ];
            runtime_handler(invoke)
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let window = window.clone();
                let state = window.state::<app::AppState>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    match state.close_all().await {
                        Ok(()) => {
                            let _ = window.destroy();
                        }
                        Err(_) => {
                            let _ = window.emit(
                                "voxbridge://shutdown-error",
                                "Runtime cleanup could not finish. Retry closing the window.",
                            );
                        }
                    }
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(move |handle, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } = &event
        {
            let handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    return;
                }
                let state = handle.state::<app::AppState>().inner().clone();
                if state.reopen_window().await.is_ok() {
                    if let Some(config) = handle.config().app.windows.first() {
                        if tauri::WebviewWindowBuilder::from_config(&handle, config)
                            .and_then(|builder| builder.build())
                            .is_err()
                        {
                            let _ = state.close_all().await;
                        }
                    }
                }
            });
        }

        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if exit_ready.load(Ordering::Acquire) {
                return;
            }
            api.prevent_exit();
            let handle = handle.clone();
            let ready = exit_ready.clone();
            let state = handle.state::<app::AppState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                match state.close_all().await {
                    Ok(()) => {
                        ready.store(true, Ordering::Release);
                        handle.exit(0);
                    }
                    Err(_) => {
                        let _ = handle.emit(
                            "voxbridge://shutdown-error",
                            "Runtime cleanup could not finish. Retry exiting the application.",
                        );
                    }
                }
            });
        }
    });
}
