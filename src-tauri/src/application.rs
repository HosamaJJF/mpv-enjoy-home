use crate::domain::{FolderSummary, MediaItem, PlayerStatus};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::Database;
use crate::infrastructure::player::{
    PLAYER_SETTING, PlayerBackend, ProcessPlayerBackend, normalize_selected_player,
};
use crate::infrastructure::scanner::scan_media;
use std::path::{Path, PathBuf};

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
        let media = self.database.media_path(media_id)?;
        let configured = self.database.setting(PLAYER_SETTING)?;
        ProcessPlayerBackend.play(configured.as_deref(), &media)
    }
}
