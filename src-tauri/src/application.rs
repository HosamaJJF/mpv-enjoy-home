use crate::domain::{
    AppearanceSettings, FolderSummary, LibraryEntry, MediaItem, MediaServerCredentials,
    MediaServerInput, MediaServerSummary, PlayerPreferences, PlayerStatus, RecentMediaItem,
    RemoteLibraryEntry, RemoteMediaDetail, UpdateApplyResult, UpdateCheckResult, natural_cmp,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::Database;
use crate::infrastructure::player::{
    PLAYER_SETTING, PlayerBackend, ProcessPlayerBackend, normalize_selected_player,
};
use crate::infrastructure::remote::{
    RemoteClient, authentication_required, requires_authentication,
};
use crate::infrastructure::scanner::scan_media;
use crate::infrastructure::updater::UpdateManager;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const APPEARANCE_SETTING: &str = "appearance";
const PLAYER_PREFERENCES_SETTING: &str = "player.preferences";
const REMOTE_DEVICE_ID_SETTING: &str = "remote.device-id";

#[derive(Debug, Clone)]
pub struct AppService {
    database: Database,
    remote_auth_lock: Arc<Mutex<()>>,
}

impl AppService {
    pub fn new(database_path: PathBuf) -> Self {
        Self {
            database: Database::new(database_path),
            remote_auth_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn initialize(&self) -> AppResult<()> {
        self.database.initialize()?;
        self.remote_device_id()?;
        Ok(())
    }

    pub fn list_folders(&self) -> AppResult<Vec<FolderSummary>> {
        self.database.list_folders()
    }

    pub fn add_folder(&self, path: &Path) -> AppResult<FolderSummary> {
        let path = path.canonicalize()?;
        if !path.is_dir() {
            return Err(AppError::message("所选路径不是目录"));
        }
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let folder_id = self.database.add_folder(&path, &name)?;
        self.rescan_folder(folder_id)
    }

    pub fn rescan_folder(&self, folder_id: i64) -> AppResult<FolderSummary> {
        let folder = self.database.folder(folder_id)?;
        let discovered = scan_media(Path::new(&folder.path))?;
        self.database.replace_media(folder_id, &discovered)
    }

    pub fn remove_folder(&self, folder_id: i64) -> AppResult<()> {
        self.database.remove_folder(folder_id)
    }

    pub fn list_media(
        &self,
        folder_id: Option<i64>,
        query: Option<&str>,
        limit: usize,
    ) -> AppResult<Vec<MediaItem>> {
        self.database.list_media(folder_id, query, limit)
    }

    pub fn list_library_entries(
        &self,
        folder_id: i64,
        parent: Option<&str>,
        query: Option<&str>,
    ) -> AppResult<Vec<LibraryEntry>> {
        let folder = self.database.folder(folder_id)?;
        let parent = normalize_relative_path(parent.unwrap_or(""))?;
        let search = query.unwrap_or("").trim().to_lowercase();
        let items = self.database.list_folder_media(folder_id)?;
        let mut entries = Vec::new();
        let mut directories: HashMap<String, LibraryEntry> = HashMap::new();

        for item in items {
            let relative_path = media_relative_path(&folder, &item);
            let Some(remainder) = strip_parent(&relative_path, &parent) else {
                continue;
            };
            if !search.is_empty() {
                if item.name.to_lowercase().contains(&search) {
                    entries.push(video_entry(item, relative_path));
                }
                continue;
            }

            if let Some((directory, _)) = remainder.split_once('/') {
                let path = join_relative(&parent, directory);
                let entry = directories
                    .entry(path.clone())
                    .or_insert_with(|| LibraryEntry {
                        key: format!("folder:{folder_id}:{path}"),
                        name: directory.to_string(),
                        relative_path: path,
                        kind: "folder".to_string(),
                        media_id: None,
                        extension: None,
                        modified_at: item.modified_at,
                        media_count: 0,
                    });
                entry.media_count += 1;
                entry.modified_at = entry.modified_at.max(item.modified_at);
            } else {
                entries.push(video_entry(item, relative_path));
            }
        }

        entries.extend(directories.into_values());
        entries.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| natural_cmp(&left.name, &right.name))
        });
        Ok(entries)
    }

    pub fn list_recent_media(&self, limit: usize) -> AppResult<Vec<RecentMediaItem>> {
        let folders = self
            .database
            .list_folders()?
            .into_iter()
            .map(|folder| (folder.id, folder))
            .collect::<HashMap<_, _>>();
        let mut recent = self
            .database
            .list_all_media()?
            .into_iter()
            .filter_map(|item| {
                let folder = folders.get(&item.folder_id)?;
                let relative_path = media_relative_path(folder, &item);
                let parent = relative_path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or("");
                Some(RecentMediaItem {
                    key: format!("local:{}:{}", item.folder_id, item.id),
                    source_kind: "local".to_string(),
                    source_id: item.folder_id,
                    source_name: folder.name.clone(),
                    target_id: parent.to_string(),
                    target_name: folder.name.clone(),
                    name: item.name,
                    context: if parent.is_empty() {
                        "根目录".to_string()
                    } else {
                        parent.replace('/', " / ")
                    },
                    item_type: item.extension,
                    updated_at: item.modified_at,
                })
            })
            .collect::<Vec<_>>();

        let servers = self
            .database
            .list_media_servers()?
            .into_iter()
            .map(|server| self.database.media_server(server.id))
            .collect::<AppResult<Vec<_>>>()?;
        let remote_batches = std::thread::scope(|scope| {
            let handles = servers
                .iter()
                .map(|server| {
                    scope.spawn(move || {
                        let result = self.with_remote_client(server.id, |client| {
                            client.list_recent_media(limit)
                        });
                        (server.id, server.name.clone(), result)
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .collect::<Vec<_>>()
        });

        for (server_id, server_name, batch) in remote_batches {
            let Ok(batch) = batch else {
                continue;
            };
            recent.extend(batch.into_iter().map(|item| RecentMediaItem {
                key: format!("remote:{server_id}:{}", item.item_id),
                source_kind: "remote".to_string(),
                source_id: server_id,
                source_name: server_name.clone(),
                target_id: item.target_id,
                target_name: item.target_name,
                name: item.name,
                context: item.context,
                item_type: item.item_type,
                updated_at: item.updated_at,
            }));
        }

        recent.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| natural_cmp(&left.name, &right.name))
        });
        recent.truncate(limit);
        Ok(recent)
    }

    pub fn list_media_servers(&self) -> AppResult<Vec<MediaServerSummary>> {
        self.database.list_media_servers()
    }

    pub fn add_media_server(&self, input: &MediaServerInput) -> AppResult<MediaServerSummary> {
        let device_id = self.remote_device_id()?;
        let server = RemoteClient::verify(input, &device_id)?;
        self.database.save_media_server(
            &server.kind,
            &server.name,
            &server.base_url,
            &server.token,
            &server.user_id,
            &server.user_name,
            &input.password,
            server.server_version.as_deref(),
        )
    }

    pub fn reauthenticate_media_server(
        &self,
        server_id: i64,
        credentials: &MediaServerCredentials,
    ) -> AppResult<MediaServerSummary> {
        let _guard = self
            .remote_auth_lock
            .lock()
            .map_err(|_| AppError::message("媒体服务器重新登录状态不可用"))?;
        let current = self.database.media_server(server_id)?;
        let device_id = self.remote_device_id()?;
        let verified = match RemoteClient::reauthenticate(&current, credentials, &device_id) {
            Ok(verified) => verified,
            Err(error) => {
                self.database.clear_media_server_password(server_id)?;
                return Err(error);
            }
        };
        self.database.update_media_server_session(
            server_id,
            &verified.token,
            &verified.user_id,
            &verified.user_name,
            Some(&credentials.password),
            verified.server_version.as_deref(),
        )
    }

    pub fn remove_media_server(&self, server_id: i64) -> AppResult<()> {
        self.database.remove_media_server(server_id)
    }

    pub fn list_remote_entries(
        &self,
        server_id: i64,
        parent_id: Option<&str>,
    ) -> AppResult<Vec<RemoteLibraryEntry>> {
        let entries =
            self.with_remote_client(server_id, |client| client.list_entries(parent_id))?;
        self.database.mark_media_server_connected(server_id)?;
        Ok(entries)
    }

    pub fn remote_media_detail(
        &self,
        server_id: i64,
        item_id: &str,
    ) -> AppResult<RemoteMediaDetail> {
        self.with_remote_client(server_id, |client| client.media_detail(item_id))
    }

    pub fn remote_image(
        &self,
        server_id: i64,
        item_id: &str,
        image_type: &str,
        max_width: u32,
    ) -> AppResult<Option<String>> {
        self.with_remote_client(server_id, |client| {
            client.image_data_url(item_id, image_type, max_width)
        })
    }

    pub fn player_status(&self) -> AppResult<PlayerStatus> {
        let configured = self.database.setting(PLAYER_SETTING)?;
        Ok(ProcessPlayerBackend.status(configured.as_deref()))
    }

    pub fn player_preferences(&self) -> AppResult<PlayerPreferences> {
        Ok(self
            .database
            .setting(PLAYER_PREFERENCES_SETTING)?
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_default())
    }

    pub fn set_player_preferences(
        &self,
        preferences: &PlayerPreferences,
    ) -> AppResult<PlayerPreferences> {
        if preferences
            .startup_volume
            .is_some_and(|volume| volume > 100)
        {
            return Err(AppError::message("播放器启动音量必须在 0 到 100 之间"));
        }
        let style = preferences.danmaku_style;
        if style
            .font_size
            .is_some_and(|value| !(10..=100).contains(&value))
        {
            return Err(AppError::message("弹幕字号必须在 10 到 100 之间"));
        }
        if style
            .outline
            .is_some_and(|value| !(0.0..=4.0).contains(&value))
        {
            return Err(AppError::message("弹幕描边必须在 0 到 4 之间"));
        }
        if style.shadow.is_some_and(|value| value > 10) {
            return Err(AppError::message("弹幕阴影必须在 0 到 10 之间"));
        }
        if style
            .scroll_time
            .is_some_and(|value| !(1..=60).contains(&value))
        {
            return Err(AppError::message("弹幕滚动时长必须在 1 到 60 秒之间"));
        }
        if style
            .opacity
            .is_some_and(|value| !(0.0..=1.0).contains(&value))
        {
            return Err(AppError::message("弹幕不透明度必须在 0 到 1 之间"));
        }
        if style
            .display_area
            .is_some_and(|value| !(0.0..=1.0).contains(&value))
        {
            return Err(AppError::message("弹幕显示区域必须在 0 到 1 之间"));
        }
        let value = serde_json::to_string(preferences)
            .map_err(|error| AppError::message(format!("播放器偏好无法保存：{error}")))?;
        self.database
            .set_setting(PLAYER_PREFERENCES_SETTING, Some(&value))?;
        Ok(*preferences)
    }

    pub fn appearance_settings(&self) -> AppResult<AppearanceSettings> {
        Ok(self
            .database
            .setting(APPEARANCE_SETTING)?
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_default())
    }

    pub fn set_appearance_settings(
        &self,
        settings: &AppearanceSettings,
    ) -> AppResult<AppearanceSettings> {
        let value = serde_json::to_string(settings)
            .map_err(|error| AppError::message(format!("外观设置无法保存：{error}")))?;
        self.database
            .set_setting(APPEARANCE_SETTING, Some(&value))?;
        Ok(*settings)
    }

    pub fn set_player(&self, path: Option<&Path>) -> AppResult<PlayerStatus> {
        let normalized = path.map(normalize_selected_player).transpose()?;
        self.database.set_setting(
            PLAYER_SETTING,
            normalized
                .as_ref()
                .map(|value| value.to_string_lossy())
                .as_deref(),
        )?;
        self.player_status()
    }

    pub fn play_media(&self, media_id: i64) -> AppResult<()> {
        let selected = self.database.media_item(media_id)?;
        let selected_path = Path::new(&selected.path);
        let selected_parent = selected_path.parent();
        let queue = self
            .database
            .list_folder_media(selected.folder_id)?
            .into_iter()
            .filter(|item| Path::new(&item.path).parent() == selected_parent)
            .collect::<Vec<_>>();
        let start_index = queue
            .iter()
            .position(|item| item.id == selected.id)
            .ok_or_else(|| AppError::message("媒体条目未出现在所属目录的播放列表中"))?;
        let media = queue
            .into_iter()
            .map(|item| PathBuf::from(item.path))
            .collect::<Vec<_>>();
        let configured = self.database.setting(PLAYER_SETTING)?;
        let preferences = self.player_preferences()?;
        ProcessPlayerBackend.play_local(configured.as_deref(), &preferences, &media, start_index)
    }

    pub fn play_remote_media(&self, server_id: i64, item_id: &str) -> AppResult<()> {
        let playback = self.with_remote_client(server_id, |client| client.playback(item_id))?;
        let configured = self.database.setting(PLAYER_SETTING)?;
        let preferences = self.player_preferences()?;
        ProcessPlayerBackend.play_remote(configured.as_deref(), &preferences, &playback)
    }

    fn with_remote_client<T>(
        &self,
        server_id: i64,
        action: impl Fn(&RemoteClient) -> AppResult<T>,
    ) -> AppResult<T> {
        let device_id = self.remote_device_id()?;
        let initial = self.database.media_server(server_id)?;
        let client = RemoteClient::from_config(&initial, &device_id)?;
        let first_error = match action(&client) {
            Ok(result) => return Ok(result),
            Err(error) if requires_authentication(&error) => error,
            Err(error) => return Err(error),
        };

        let _guard = self
            .remote_auth_lock
            .lock()
            .map_err(|_| AppError::message("媒体服务器重新登录状态不可用"))?;
        let current = self.database.media_server(server_id)?;
        if current.token != initial.token {
            let refreshed = RemoteClient::from_config(&current, &device_id)?;
            match action(&refreshed) {
                Ok(result) => return Ok(result),
                Err(error) if requires_authentication(&error) => {}
                Err(error) => return Err(error),
            }
        }

        let Some(password) = current.password.clone() else {
            return Err(first_error);
        };
        let credentials = MediaServerCredentials {
            username: current.user_name.clone(),
            password,
        };
        let verified = match RemoteClient::reauthenticate(&current, &credentials, &device_id) {
            Ok(verified) => verified,
            Err(_) => {
                self.database.clear_media_server_password(server_id)?;
                return Err(authentication_required(
                    "保存的密码无法重新登录，请手动输入最新密码",
                ));
            }
        };
        self.database.update_media_server_session(
            server_id,
            &verified.token,
            &verified.user_id,
            &verified.user_name,
            None,
            verified.server_version.as_deref(),
        )?;
        let updated = self.database.media_server(server_id)?;
        let result = action(&RemoteClient::from_config(&updated, &device_id)?);
        if result.as_ref().is_err_and(requires_authentication) {
            self.database.clear_media_server_password(server_id)?;
            return Err(authentication_required(
                "服务器没有接受新登录会话，请手动重新登录",
            ));
        }
        result
    }

    fn remote_device_id(&self) -> AppResult<String> {
        if let Some(device_id) = self.database.setting(REMOTE_DEVICE_ID_SETTING)? {
            if is_safe_remote_device_id(&device_id) {
                return Ok(device_id);
            }
        }
        let device_id = format!("mpv-enjoy-home-{}", Uuid::new_v4().simple());
        self.database
            .set_setting(REMOTE_DEVICE_ID_SETTING, Some(&device_id))?;
        Ok(device_id)
    }

    pub fn check_app_update(&self) -> AppResult<UpdateCheckResult> {
        let manager = UpdateManager::new()?;
        manager.check_for_updates()
    }

    pub fn download_and_apply_update(&self) -> AppResult<UpdateApplyResult> {
        let manager = UpdateManager::new()?;
        manager.download_latest_update()
    }

    pub fn open_update_release(&self) -> AppResult<()> {
        let manager = UpdateManager::new()?;
        manager.open_release_page()
    }
}

fn is_safe_remote_device_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn normalize_relative_path(value: &str) -> AppResult<String> {
    let path = Path::new(value);
    let mut normalized = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::message("媒体库路径不能越过已添加的目录"));
            }
        }
    }
    Ok(normalized.join("/"))
}

fn media_relative_path(folder: &FolderSummary, item: &MediaItem) -> String {
    if !item.relative_path.is_empty() {
        return item.relative_path.replace('\\', "/");
    }
    Path::new(&item.path)
        .strip_prefix(&folder.path)
        .unwrap_or_else(|_| Path::new(&item.name))
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn strip_parent<'a>(relative_path: &'a str, parent: &str) -> Option<&'a str> {
    if parent.is_empty() {
        return Some(relative_path);
    }
    relative_path
        .strip_prefix(parent)
        .and_then(|value| value.strip_prefix('/'))
}

fn join_relative(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}

fn video_entry(item: MediaItem, relative_path: String) -> LibraryEntry {
    LibraryEntry {
        key: format!("media:{}", item.id),
        name: item.name,
        relative_path,
        kind: "video".to_string(),
        media_id: Some(item.id),
        extension: Some(item.extension),
        modified_at: item.modified_at,
        media_count: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccentColor, DiscoveredMedia, PlayerToggleMode, ThemeMode};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn relative_paths_cannot_escape_the_library() {
        assert_eq!(
            normalize_relative_path("Series/Season 01").unwrap(),
            "Series/Season 01"
        );
        assert!(normalize_relative_path("../private").is_err());
        assert!(normalize_relative_path("/private").is_err());
    }

    #[test]
    fn local_directory_time_is_the_latest_video_at_every_level() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-library-entries-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let service = AppService::new(path.clone());
        service.initialize().unwrap();
        let folder_id = service
            .database
            .add_folder(Path::new("/media"), "media")
            .unwrap();
        service
            .database
            .replace_media(
                folder_id,
                &[
                    DiscoveredMedia {
                        name: "old.mkv".to_string(),
                        path: PathBuf::from("/media/Shows/Season 1/old.mkv"),
                        relative_path: "Shows/Season 1/old.mkv".to_string(),
                        extension: "mkv".to_string(),
                        modified_at: 10,
                    },
                    DiscoveredMedia {
                        name: "new.mkv".to_string(),
                        path: PathBuf::from("/media/Shows/Season 2/new.mkv"),
                        relative_path: "Shows/Season 2/new.mkv".to_string(),
                        extension: "mkv".to_string(),
                        modified_at: 30,
                    },
                    DiscoveredMedia {
                        name: "movie.mkv".to_string(),
                        path: PathBuf::from("/media/Movies/movie.mkv"),
                        relative_path: "Movies/movie.mkv".to_string(),
                        extension: "mkv".to_string(),
                        modified_at: 20,
                    },
                ],
            )
            .unwrap();

        let root = service.list_library_entries(folder_id, None, None).unwrap();
        assert_eq!(
            root.iter()
                .find(|entry| entry.name == "Shows")
                .map(|entry| entry.modified_at),
            Some(30)
        );
        let shows = service
            .list_library_entries(folder_id, Some("Shows"), None)
            .unwrap();
        assert_eq!(
            shows
                .iter()
                .map(|entry| (entry.name.as_str(), entry.modified_at))
                .collect::<Vec<_>>(),
            vec![("Season 1", 10), ("Season 2", 30)]
        );

        drop(service);
        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn rescanning_discovers_media_added_after_the_folder_was_indexed() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-auto-rescan-{}-{unique}",
            std::process::id()
        ));
        let media_root = root.join("library");
        std::fs::create_dir_all(&media_root).unwrap();
        let service = AppService::new(root.join("state.sqlite3"));
        service.initialize().unwrap();

        let folder = service.add_folder(&media_root).unwrap();
        assert_eq!(folder.media_count, 0);
        std::fs::write(media_root.join("new-episode.mkv"), []).unwrap();
        assert!(
            service
                .list_library_entries(folder.id, None, None)
                .unwrap()
                .is_empty()
        );

        let updated = service.rescan_folder(folder.id).unwrap();
        let entries = service.list_library_entries(folder.id, None, None).unwrap();
        assert_eq!(updated.media_count, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "new-episode.mkv");

        drop(service);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn appearance_settings_default_to_system_blue_and_persist() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-appearance-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let service = AppService::new(path.clone());
        service.initialize().unwrap();
        assert_eq!(
            service.appearance_settings().unwrap(),
            AppearanceSettings::default()
        );

        let selected = AppearanceSettings {
            theme_mode: ThemeMode::Dark,
            accent_color: AccentColor::Pink,
        };
        assert_eq!(
            service.set_appearance_settings(&selected).unwrap(),
            selected
        );
        assert_eq!(service.appearance_settings().unwrap(), selected);
        drop(service);

        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn player_preferences_default_to_inherit_and_persist() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-player-preferences-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let service = AppService::new(path.clone());
        service.initialize().unwrap();
        assert_eq!(
            service.player_preferences().unwrap(),
            PlayerPreferences::default()
        );

        let selected = PlayerPreferences {
            startup_volume: Some(72),
            fullscreen_mode: PlayerToggleMode::On,
            danmaku_mode: PlayerToggleMode::Off,
            danmaku_style: crate::domain::DanmakuStylePreferences {
                bold_mode: PlayerToggleMode::On,
                font_size: Some(42),
                outline: Some(1.5),
                shadow: Some(2),
                scroll_time: Some(12),
                opacity: Some(0.75),
                display_area: Some(0.8),
            },
        };
        assert_eq!(service.set_player_preferences(&selected).unwrap(), selected);
        assert_eq!(service.player_preferences().unwrap(), selected);

        let invalid = PlayerPreferences {
            startup_volume: Some(101),
            ..PlayerPreferences::default()
        };
        assert!(service.set_player_preferences(&invalid).is_err());
        let invalid_style = PlayerPreferences {
            danmaku_style: crate::domain::DanmakuStylePreferences {
                outline: Some(4.1),
                ..Default::default()
            },
            ..PlayerPreferences::default()
        };
        assert!(service.set_player_preferences(&invalid_style).is_err());
        drop(service);

        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn remote_device_id_is_generated_once_and_persisted() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-remote-device-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let service = AppService::new(path.clone());
        service.initialize().unwrap();

        let first = service.remote_device_id().unwrap();
        let second = service.remote_device_id().unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("mpv-enjoy-home-"));
        assert!(is_safe_remote_device_id(&first));

        drop(service);
        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    #[ignore = "需要绑定本机回环端口"]
    fn revoked_remote_token_is_refreshed_with_the_saved_password() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mock = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..5 {
                let (mut stream, _) = listener.accept().unwrap();
                requests.push(read_http_request(&mut stream));
                let (status, body) = match index {
                    0 => ("401 Unauthorized", ""),
                    1 => (
                        "200 OK",
                        r#"{"User":{"Id":"new-user-id","Name":"Guest"},"AccessToken":"new-token"}"#,
                    ),
                    2 => ("200 OK", r#"{"ServerName":"Mock Emby","Version":"4.9.0"}"#),
                    _ => ("200 OK", r#"{"Items":[]}"#),
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                stream.flush().unwrap();
            }
            requests
        });

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-auto-login-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let service = AppService::new(path.clone());
        service.initialize().unwrap();
        let saved = service
            .database
            .save_media_server(
                "emby",
                "Mock Emby",
                &format!("http://{address}"),
                "revoked-token",
                "old-user-id",
                "Guest",
                "saved-password",
                Some("4.8.0"),
            )
            .unwrap();

        assert!(
            service
                .list_remote_entries(saved.id, None)
                .unwrap()
                .is_empty()
        );
        let requests = mock.join().unwrap();
        assert!(requests[0].starts_with("GET /emby/Users/old-user-id/Views"));
        assert!(requests[1].starts_with("POST /emby/Users/AuthenticateByName"));
        assert!(requests[1].contains(r#""Pw":"saved-password""#));
        let refreshed = service.database.media_server(saved.id).unwrap();
        assert_eq!(refreshed.token, "new-token");
        assert_eq!(refreshed.user_id, "new-user-id");
        assert_eq!(refreshed.password.as_deref(), Some("saved-password"));

        drop(service);
        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn parent_matching_respects_path_boundaries() {
        assert_eq!(
            strip_parent("Show/Season/file.mkv", "Show"),
            Some("Season/file.mkv")
        );
        assert_eq!(strip_parent("Showcase/file.mkv", "Show"), None);
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 2048];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
            let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
                continue;
            };
            let header_end = header_end + 4;
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            if bytes.len() >= header_end + content_length {
                break;
            }
        }
        String::from_utf8(bytes).unwrap()
    }
}
