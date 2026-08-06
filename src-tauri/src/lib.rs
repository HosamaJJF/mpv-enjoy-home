mod application;
mod commands;
mod domain;
mod error;
mod infrastructure;

use application::AppService;
use commands::AppState;
use std::fs;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&data_directory)?;
            let service = AppService::new(data_directory.join("mpv-enjoy-home.sqlite3"));
            service.initialize()?;
            app.manage(AppState { service });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_library_folders,
            commands::add_library_folder,
            commands::rescan_library_folder,
            commands::remove_library_folder,
            commands::list_media,
            commands::list_library_entries,
            commands::list_recent_collections,
            commands::list_media_servers,
            commands::add_media_server,
            commands::remove_media_server,
            commands::list_remote_entries,
            commands::get_remote_image,
            commands::get_remote_media_detail,
            commands::get_player_status,
            commands::set_player_executable,
            commands::play_media,
            commands::play_remote_media,
        ])
        .run(tauri::generate_context!())
        .expect("error while running mpv-enjoy Home");
}
