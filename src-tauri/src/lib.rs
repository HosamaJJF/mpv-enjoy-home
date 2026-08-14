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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fs::metadata(&data_directory)?.permissions();
                permissions.set_mode(0o700);
                fs::set_permissions(&data_directory, permissions)?;
            }
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
            commands::list_recent_media,
            commands::list_media_servers,
            commands::add_media_server,
            commands::reauthenticate_media_server,
            commands::remove_media_server,
            commands::list_remote_entries,
            commands::get_remote_image,
            commands::get_remote_media_detail,
            commands::get_player_status,
            commands::get_player_preferences,
            commands::set_player_preferences,
            commands::get_appearance_settings,
            commands::set_appearance_settings,
            commands::set_player_executable,
            commands::play_media,
            commands::play_remote_media,
            commands::check_app_update,
            commands::download_and_apply_update,
            commands::open_external_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running mpv-enjoy Home");
}
