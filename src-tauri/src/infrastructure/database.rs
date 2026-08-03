use crate::domain::{DiscoveredMedia, FolderSummary, MediaItem, natural_cmp};
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn initialize(&self) -> AppResult<()> {
        let connection = self.open()?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(AppError::message(format!(
                "数据库版本 {version} 高于当前程序支持的版本 {SCHEMA_VERSION}"
            )));
        }
        if version == 0 {
            connection.execute_batch(
                "BEGIN;
                 CREATE TABLE library_folders (
                     id INTEGER PRIMARY KEY,
                     name TEXT NOT NULL,
                     path TEXT NOT NULL UNIQUE,
                     media_count INTEGER NOT NULL DEFAULT 0,
                     added_at INTEGER NOT NULL,
                     last_scanned_at INTEGER
                 );
                 CREATE TABLE media_items (
                     id INTEGER PRIMARY KEY,
                     folder_id INTEGER NOT NULL REFERENCES library_folders(id) ON DELETE CASCADE,
                     name TEXT NOT NULL,
                     path TEXT NOT NULL UNIQUE,
                     extension TEXT NOT NULL,
                     modified_at INTEGER NOT NULL
                 );
                 CREATE INDEX media_items_folder_id ON media_items(folder_id);
                 CREATE INDEX media_items_modified_at ON media_items(modified_at DESC);
                 CREATE TABLE settings (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        Ok(())
    }

    fn open(&self) -> AppResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(connection)
    }

    pub fn list_folders(&self) -> AppResult<Vec<FolderSummary>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, name, path, media_count, added_at, last_scanned_at
             FROM library_folders ORDER BY name COLLATE NOCASE, id",
        )?;
        let folders = statement
            .query_map([], |row| {
                Ok(FolderSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    media_count: row.get(3)?,
                    added_at: row.get(4)?,
                    last_scanned_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(folders)
    }

    pub fn add_folder(&self, path: &Path, name: &str) -> AppResult<i64> {
        let connection = self.open()?;
        let path = path.to_string_lossy();
        connection.execute(
            "INSERT INTO library_folders(name, path, added_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET name = excluded.name",
            params![name, path, now_timestamp()],
        )?;
        connection
            .query_row(
                "SELECT id FROM library_folders WHERE path = ?1",
                [path.as_ref()],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn replace_media(
        &self,
        folder_id: i64,
        discovered: &[DiscoveredMedia],
    ) -> AppResult<FolderSummary> {
        let mut connection = self.open()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM media_items WHERE folder_id = ?1", [folder_id])?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO media_items(folder_id, name, path, extension, modified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for media in discovered {
                insert.execute(params![
                    folder_id,
                    media.name,
                    media.path.to_string_lossy(),
                    media.extension,
                    media.modified_at,
                ])?;
            }
        }
        transaction.execute(
            "UPDATE library_folders
             SET media_count = ?1, last_scanned_at = ?2
             WHERE id = ?3",
            params![discovered.len() as i64, now_timestamp(), folder_id],
        )?;
        transaction.commit()?;
        self.folder(folder_id)
    }

    pub fn folder(&self, folder_id: i64) -> AppResult<FolderSummary> {
        let connection = self.open()?;
        connection
            .query_row(
                "SELECT id, name, path, media_count, added_at, last_scanned_at
                 FROM library_folders WHERE id = ?1",
                [folder_id],
                |row| {
                    Ok(FolderSummary {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        path: row.get(2)?,
                        media_count: row.get(3)?,
                        added_at: row.get(4)?,
                        last_scanned_at: row.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::message("媒体目录不存在"))
    }

    pub fn remove_folder(&self, folder_id: i64) -> AppResult<()> {
        let connection = self.open()?;
        if connection.execute("DELETE FROM library_folders WHERE id = ?1", [folder_id])? == 0 {
            return Err(AppError::message("媒体目录不存在"));
        }
        Ok(())
    }

    pub fn list_media(
        &self,
        folder_id: Option<i64>,
        query: Option<&str>,
        limit: usize,
    ) -> AppResult<Vec<MediaItem>> {
        let connection = self.open()?;
        let query = query.unwrap_or("").trim();
        let like = format!("%{query}%");
        let mut statement = connection.prepare(
            "SELECT id, folder_id, name, path, extension, modified_at
             FROM media_items
             WHERE (?1 IS NULL OR folder_id = ?1)
               AND (?2 = '' OR name LIKE ?3)
             ORDER BY modified_at DESC
             LIMIT ?4",
        )?;
        let mut media = statement
            .query_map(params![folder_id, query, like, limit as i64], |row| {
                Ok(MediaItem {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    name: row.get(2)?,
                    path: row.get(3)?,
                    extension: row.get(4)?,
                    modified_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if folder_id.is_some() {
            media.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        }
        Ok(media)
    }

    pub fn media_path(&self, media_id: i64) -> AppResult<PathBuf> {
        let connection = self.open()?;
        connection
            .query_row(
                "SELECT path FROM media_items WHERE id = ?1",
                [media_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(PathBuf::from)
            .ok_or_else(|| AppError::message("媒体条目不存在"))
    }

    pub fn setting(&self, key: &str) -> AppResult<Option<String>> {
        let connection = self.open()?;
        connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(Into::into)
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> AppResult<()> {
        let connection = self.open()?;
        if let Some(value) = value {
            connection.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        } else {
            connection.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        }
        Ok(())
    }
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
