use crate::domain::{
    FolderSummary, LibraryEntry, MediaItem, MediaServerInput, MediaServerSummary, PlayerStatus,
    RecentMediaItem, RemoteLibraryEntry, RemoteMediaDetail, natural_cmp,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::Database;
use crate::infrastructure::player::{
    PLAYER_SETTING, PlayerBackend, ProcessPlayerBackend, normalize_selected_player,
};
use crate::infrastructure::remote::RemoteClient;
use crate::infrastructure::scanner::scan_media;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppService {
    database: Database,
}

impl AppService {
    pub fn new(database_path: PathBuf) -> Self {
        Self {
            database: Database::new(database_path),
        }
    }

    pub fn initialize(&self) -> AppResult<()> {
        self.database.initialize()
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
                        let result = RemoteClient::from_config(server)
                            .and_then(|client| client.list_recent_media(limit));
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
        let server = RemoteClient::verify(input)?;
        self.database.save_media_server(
            &server.kind,
            &server.name,
            &server.base_url,
            &server.token,
            &server.user_id,
            &server.user_name,
            server.server_version.as_deref(),
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
        let server = self.database.media_server(server_id)?;
        let entries = RemoteClient::from_config(&server)?.list_entries(parent_id)?;
        self.database.mark_media_server_connected(server_id)?;
        Ok(entries)
    }

    pub fn remote_media_detail(
        &self,
        server_id: i64,
        item_id: &str,
    ) -> AppResult<RemoteMediaDetail> {
        let server = self.database.media_server(server_id)?;
        RemoteClient::from_config(&server)?.media_detail(item_id)
    }

    pub fn remote_image(
        &self,
        server_id: i64,
        item_id: &str,
        image_type: &str,
        max_width: u32,
    ) -> AppResult<Option<String>> {
        let server = self.database.media_server(server_id)?;
        RemoteClient::from_config(&server)?.image_data_url(item_id, image_type, max_width)
    }

    pub fn player_status(&self) -> AppResult<PlayerStatus> {
        let configured = self.database.setting(PLAYER_SETTING)?;
        Ok(ProcessPlayerBackend.status(configured.as_deref()))
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
        ProcessPlayerBackend.play_local(configured.as_deref(), &media, start_index)
    }

    pub fn play_remote_media(&self, server_id: i64, item_id: &str) -> AppResult<()> {
        let server = self.database.media_server(server_id)?;
        let playback = RemoteClient::from_config(&server)?.playback(item_id)?;
        let configured = self.database.setting(PLAYER_SETTING)?;
        ProcessPlayerBackend.play_remote(configured.as_deref(), &playback)
    }
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
    fn parent_matching_respects_path_boundaries() {
        assert_eq!(
            strip_parent("Show/Season/file.mkv", "Show"),
            Some("Season/file.mkv")
        );
        assert_eq!(strip_parent("Showcase/file.mkv", "Show"), None);
    }
}
