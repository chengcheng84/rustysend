pub mod commands;
pub mod config;
pub mod discovery;
pub mod state;
pub mod transfer;

use std::sync::Arc;

use tauri::Manager;

use crate::config::settings::Settings;
use crate::state::app_state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let settings = Settings::default();
            let app_state = Arc::new(AppState::new(settings));
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::receiver::start_receiver,
            commands::receiver::stop_receiver,
            commands::sender::send_file,
            commands::sender::get_active_transfers,
            commands::sender::cancel_transfer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
