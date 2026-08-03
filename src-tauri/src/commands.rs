use crate::application::AppService;
use crate::domain::{FolderSummary, MediaItem, PlayerStatus};
use std::path::PathBuf;
use tauri::State;

#[derive(Debug)]
pub struct AppState {
    pub service: AppService,
}

#[tauri::command]
pub fn list_library_folders(state: State<'_, AppState>) -> Result<Vec<FolderSummary>, String> {
    state.service.list_folders().map_err(|error| error.0)
}

#[tauri::command]
pub async fn add_library_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<FolderSummary, String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || service.add_folder(&PathBuf::from(path)))
        .await
        .map_err(|error| format!("目录扫描任务失败：{error}"))?
        .map_err(|error| error.0)
}

#[tauri::command]
pub async fn rescan_library_folder(
    state: State<'_, AppState>,
    folder_id: i64,
) -> Result<FolderSummary, String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || service.rescan_folder(folder_id))
        .await
        .map_err(|error| format!("目录扫描任务失败：{error}"))?
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn remove_library_folder(state: State<'_, AppState>, folder_id: i64) -> Result<(), String> {
    state
        .service
        .remove_folder(folder_id)
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn list_media(
    state: State<'_, AppState>,
    folder_id: Option<i64>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<MediaItem>, String> {
    state
        .service
        .list_media(folder_id, query.as_deref(), limit.unwrap_or(500).min(2_000))
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn get_player_status(state: State<'_, AppState>) -> Result<PlayerStatus, String> {
    state.service.player_status().map_err(|error| error.0)
}

#[tauri::command]
pub fn set_player_executable(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<PlayerStatus, String> {
    state
        .service
        .set_player(path.as_ref().map(PathBuf::from).as_deref())
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn play_media(state: State<'_, AppState>, media_id: i64) -> Result<(), String> {
    state.service.play_media(media_id).map_err(|error| error.0)
}
