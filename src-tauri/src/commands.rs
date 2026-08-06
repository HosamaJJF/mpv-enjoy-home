use crate::application::AppService;
use crate::domain::{
    FolderSummary, LibraryEntry, MediaItem, MediaServerInput, MediaServerSummary, PlayerStatus,
    RecentCollection, RemoteLibraryEntry, RemoteMediaDetail,
};
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
pub fn list_library_entries(
    state: State<'_, AppState>,
    folder_id: i64,
    parent: Option<String>,
    query: Option<String>,
) -> Result<Vec<LibraryEntry>, String> {
    state
        .service
        .list_library_entries(folder_id, parent.as_deref(), query.as_deref())
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn list_recent_collections(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<RecentCollection>, String> {
    state
        .service
        .list_recent_collections(limit.unwrap_or(8).min(24))
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn list_media_servers(state: State<'_, AppState>) -> Result<Vec<MediaServerSummary>, String> {
    state.service.list_media_servers().map_err(|error| error.0)
}

#[tauri::command]
pub async fn add_media_server(
    state: State<'_, AppState>,
    input: MediaServerInput,
) -> Result<MediaServerSummary, String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || service.add_media_server(&input))
        .await
        .map_err(|error| format!("媒体服务器连接任务失败：{error}"))?
        .map_err(|error| error.0)
}

#[tauri::command]
pub fn remove_media_server(state: State<'_, AppState>, server_id: i64) -> Result<(), String> {
    state
        .service
        .remove_media_server(server_id)
        .map_err(|error| error.0)
}

#[tauri::command]
pub async fn list_remote_entries(
    state: State<'_, AppState>,
    server_id: i64,
    parent_id: Option<String>,
) -> Result<Vec<RemoteLibraryEntry>, String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.list_remote_entries(server_id, parent_id.as_deref())
    })
    .await
    .map_err(|error| format!("媒体服务器浏览任务失败：{error}"))?
    .map_err(|error| error.0)
}

#[tauri::command]
pub async fn get_remote_image(
    state: State<'_, AppState>,
    server_id: i64,
    item_id: String,
    image_type: Option<String>,
    max_width: Option<u32>,
) -> Result<Option<String>, String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.remote_image(
            server_id,
            &item_id,
            image_type.as_deref().unwrap_or("Primary"),
            max_width.unwrap_or(360),
        )
    })
    .await
    .map_err(|error| format!("远程封面任务失败：{error}"))?
    .map_err(|error| error.0)
}

#[tauri::command]
pub async fn get_remote_media_detail(
    state: State<'_, AppState>,
    server_id: i64,
    item_id: String,
) -> Result<RemoteMediaDetail, String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || service.remote_media_detail(server_id, &item_id))
        .await
        .map_err(|error| format!("远程媒体详情任务失败：{error}"))?
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

#[tauri::command]
pub async fn play_remote_media(
    state: State<'_, AppState>,
    server_id: i64,
    item_id: String,
) -> Result<(), String> {
    let service = state.service.clone();
    tauri::async_runtime::spawn_blocking(move || service.play_remote_media(server_id, &item_id))
        .await
        .map_err(|error| format!("远程播放任务失败：{error}"))?
        .map_err(|error| error.0)
}
