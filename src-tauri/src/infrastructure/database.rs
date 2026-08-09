use crate::domain::{
    DiscoveredMedia, FolderSummary, MediaItem, MediaServerConfig, MediaServerSummary, natural_cmp,
};
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: i64 = 3;

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
                     relative_path TEXT NOT NULL,
                     extension TEXT NOT NULL,
                     modified_at INTEGER NOT NULL
                 );
                 CREATE INDEX media_items_folder_id ON media_items(folder_id);
                 CREATE INDEX media_items_modified_at ON media_items(modified_at DESC);
                 CREATE TABLE settings (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL
                 );
                 CREATE TABLE media_servers (
                     id INTEGER PRIMARY KEY,
                     kind TEXT NOT NULL,
                     name TEXT NOT NULL,
                     base_url TEXT NOT NULL,
                     token TEXT NOT NULL,
                     user_id TEXT NOT NULL,
                     user_name TEXT NOT NULL,
                     password TEXT,
                     server_version TEXT,
                     added_at INTEGER NOT NULL,
                     last_connected_at INTEGER,
                     UNIQUE(kind, base_url, user_id)
                 );
                 PRAGMA user_version = 3;
                 COMMIT;",
            )?;
        }
        if version == 1 {
            connection.execute_batch(
                "BEGIN;
                 ALTER TABLE media_items ADD COLUMN relative_path TEXT NOT NULL DEFAULT '';
                 CREATE TABLE media_servers (
                     id INTEGER PRIMARY KEY,
                     kind TEXT NOT NULL,
                     name TEXT NOT NULL,
                     base_url TEXT NOT NULL,
                     token TEXT NOT NULL,
                     user_id TEXT NOT NULL,
                     user_name TEXT NOT NULL,
                     password TEXT,
                     server_version TEXT,
                     added_at INTEGER NOT NULL,
                     last_connected_at INTEGER,
                     UNIQUE(kind, base_url, user_id)
                 );
                 PRAGMA user_version = 3;
                 COMMIT;",
            )?;
        }
        if version == 2 {
            connection.execute_batch(
                "BEGIN;
                 ALTER TABLE media_servers ADD COLUMN password TEXT;
                 PRAGMA user_version = 3;
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
        restrict_database_file_permissions(&self.path)?;
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
                "INSERT INTO media_items(
                     folder_id, name, path, relative_path, extension, modified_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for media in discovered {
                insert.execute(params![
                    folder_id,
                    media.name,
                    media.path.to_string_lossy(),
                    media.relative_path,
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
            "SELECT id, folder_id, name, path, relative_path, extension, modified_at
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
                    relative_path: row.get(4)?,
                    extension: row.get(5)?,
                    modified_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if folder_id.is_some() {
            media.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        }
        Ok(media)
    }

    pub fn list_all_media(&self) -> AppResult<Vec<MediaItem>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, folder_id, name, path, relative_path, extension, modified_at
             FROM media_items ORDER BY modified_at DESC, id DESC",
        )?;
        let media = statement
            .query_map([], |row| {
                Ok(MediaItem {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    name: row.get(2)?,
                    path: row.get(3)?,
                    relative_path: row.get(4)?,
                    extension: row.get(5)?,
                    modified_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(media)
    }

    pub fn list_folder_media(&self, folder_id: i64) -> AppResult<Vec<MediaItem>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, folder_id, name, path, relative_path, extension, modified_at
             FROM media_items WHERE folder_id = ?1",
        )?;
        let mut media = statement
            .query_map([folder_id], |row| {
                Ok(MediaItem {
                    id: row.get(0)?,
                    folder_id: row.get(1)?,
                    name: row.get(2)?,
                    path: row.get(3)?,
                    relative_path: row.get(4)?,
                    extension: row.get(5)?,
                    modified_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        media.sort_by(|left, right| natural_cmp(&left.name, &right.name));
        Ok(media)
    }

    pub fn media_item(&self, media_id: i64) -> AppResult<MediaItem> {
        let connection = self.open()?;
        connection
            .query_row(
                "SELECT id, folder_id, name, path, relative_path, extension, modified_at
                 FROM media_items WHERE id = ?1",
                [media_id],
                |row| {
                    Ok(MediaItem {
                        id: row.get(0)?,
                        folder_id: row.get(1)?,
                        name: row.get(2)?,
                        path: row.get(3)?,
                        relative_path: row.get(4)?,
                        extension: row.get(5)?,
                        modified_at: row.get(6)?,
                    })
                },
            )
            .optional()?
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

    #[allow(clippy::too_many_arguments)]
    pub fn save_media_server(
        &self,
        kind: &str,
        name: &str,
        base_url: &str,
        token: &str,
        user_id: &str,
        user_name: &str,
        password: &str,
        server_version: Option<&str>,
    ) -> AppResult<MediaServerSummary> {
        let connection = self.open()?;
        let now = now_timestamp();
        connection.execute(
            "INSERT INTO media_servers(
                 kind, name, base_url, token, user_id, user_name, password,
                 server_version, added_at, last_connected_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(kind, base_url, user_id) DO UPDATE SET
                 name = excluded.name,
                 token = excluded.token,
                 user_name = excluded.user_name,
                 password = excluded.password,
                 server_version = excluded.server_version,
                 last_connected_at = excluded.last_connected_at",
            params![
                kind,
                name,
                base_url,
                token,
                user_id,
                user_name,
                password,
                server_version,
                now
            ],
        )?;
        let id = connection.query_row(
            "SELECT id FROM media_servers
             WHERE kind = ?1 AND base_url = ?2 AND user_id = ?3",
            params![kind, base_url, user_id],
            |row| row.get(0),
        )?;
        self.media_server(id).map(Into::into)
    }

    pub fn list_media_servers(&self) -> AppResult<Vec<MediaServerSummary>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, kind, name, base_url, user_id, user_name,
                    server_version, added_at, last_connected_at
             FROM media_servers ORDER BY name COLLATE NOCASE, id",
        )?;
        let servers = statement
            .query_map([], |row| {
                Ok(MediaServerSummary {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    name: row.get(2)?,
                    base_url: row.get(3)?,
                    user_id: row.get(4)?,
                    user_name: row.get(5)?,
                    server_version: row.get(6)?,
                    added_at: row.get(7)?,
                    last_connected_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(servers)
    }

    pub fn media_server(&self, server_id: i64) -> AppResult<MediaServerConfig> {
        let connection = self.open()?;
        connection
            .query_row(
                "SELECT id, kind, name, base_url, token, user_id, user_name,
                        password, server_version, added_at, last_connected_at
                 FROM media_servers WHERE id = ?1",
                [server_id],
                |row| {
                    Ok(MediaServerConfig {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        name: row.get(2)?,
                        base_url: row.get(3)?,
                        token: row.get(4)?,
                        user_id: row.get(5)?,
                        user_name: row.get(6)?,
                        password: row.get(7)?,
                        server_version: row.get(8)?,
                        added_at: row.get(9)?,
                        last_connected_at: row.get(10)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| AppError::message("媒体服务器不存在"))
    }

    pub fn mark_media_server_connected(&self, server_id: i64) -> AppResult<()> {
        let connection = self.open()?;
        connection.execute(
            "UPDATE media_servers SET last_connected_at = ?1 WHERE id = ?2",
            params![now_timestamp(), server_id],
        )?;
        Ok(())
    }

    pub fn update_media_server_session(
        &self,
        server_id: i64,
        token: &str,
        user_id: &str,
        user_name: &str,
        password: Option<&str>,
        server_version: Option<&str>,
    ) -> AppResult<MediaServerSummary> {
        let connection = self.open()?;
        if connection.execute(
            "UPDATE media_servers
             SET token = ?1, user_id = ?2, user_name = ?3,
                 password = COALESCE(?4, password), server_version = ?5,
                 last_connected_at = ?6
             WHERE id = ?7",
            params![
                token,
                user_id,
                user_name,
                password,
                server_version,
                now_timestamp(),
                server_id
            ],
        )? == 0
        {
            return Err(AppError::message("媒体服务器不存在"));
        }
        self.media_server(server_id).map(Into::into)
    }

    pub fn clear_media_server_password(&self, server_id: i64) -> AppResult<()> {
        let connection = self.open()?;
        if connection.execute(
            "UPDATE media_servers SET password = NULL WHERE id = ?1",
            [server_id],
        )? == 0
        {
            return Err(AppError::message("媒体服务器不存在"));
        }
        Ok(())
    }

    pub fn remove_media_server(&self, server_id: i64) -> AppResult<()> {
        let connection = self.open()?;
        if connection.execute("DELETE FROM media_servers WHERE id = ?1", [server_id])? == 0 {
            return Err(AppError::message("媒体服务器不存在"));
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

#[cfg(unix)]
fn restrict_database_file_permissions(path: &Path) -> AppResult<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        let mut permissions = metadata.permissions();
        if permissions.mode() & 0o077 != 0 {
            permissions.set_mode(0o600);
            fs::set_permissions(candidate, permissions)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_database_file_permissions(_path: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_without_dropping_existing_media() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-migration-{}-{unique}.sqlite3",
            std::process::id()
        ));
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE library_folders (
                         id INTEGER PRIMARY KEY,
                         name TEXT NOT NULL,
                         path TEXT NOT NULL UNIQUE,
                         media_count INTEGER NOT NULL DEFAULT 0,
                         added_at INTEGER NOT NULL,
                         last_scanned_at INTEGER
                     );
                     CREATE TABLE media_items (
                         id INTEGER PRIMARY KEY,
                         folder_id INTEGER NOT NULL REFERENCES library_folders(id),
                         name TEXT NOT NULL,
                         path TEXT NOT NULL UNIQUE,
                         extension TEXT NOT NULL,
                         modified_at INTEGER NOT NULL
                     );
                     CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO library_folders(id, name, path, added_at)
                     VALUES (1, 'Anime', '/media/anime', 1);
                     INSERT INTO media_items(
                         id, folder_id, name, path, extension, modified_at
                     ) VALUES (1, 1, 'episode.mkv', '/media/anime/episode.mkv', 'mkv', 1);
                     PRAGMA user_version = 1;",
                )
                .unwrap();
        }

        let database = Database::new(path.clone());
        database.initialize().unwrap();
        let media = database.list_all_media().unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].relative_path, "");
        assert!(database.list_media_servers().unwrap().is_empty());
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        drop(connection);

        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn refreshes_a_media_server_session_without_replacing_the_connection() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-server-session-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let database = Database::new(path.clone());
        database.initialize().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
                0
            );
        }
        let saved = database
            .save_media_server(
                "jellyfin",
                "HomeHub",
                "https://media.example",
                "old-token",
                "old-user-id",
                "Guest",
                "old-password",
                Some("10.10.0"),
            )
            .unwrap();

        let refreshed = database
            .update_media_server_session(
                saved.id,
                "new-token",
                "new-user-id",
                "Guest",
                Some("new-password"),
                Some("10.11.0"),
            )
            .unwrap();
        let config = database.media_server(saved.id).unwrap();

        assert_eq!(refreshed.id, saved.id);
        assert_eq!(refreshed.added_at, saved.added_at);
        assert_eq!(refreshed.user_id, "new-user-id");
        assert_eq!(refreshed.server_version.as_deref(), Some("10.11.0"));
        assert_eq!(config.token, "new-token");
        assert_eq!(config.password.as_deref(), Some("new-password"));
        database.clear_media_server_password(saved.id).unwrap();
        assert_eq!(database.media_server(saved.id).unwrap().password, None);

        drop(database);
        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn migrates_v2_servers_without_inventing_saved_passwords() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mpv-enjoy-home-password-migration-{}-{unique}.sqlite3",
            std::process::id()
        ));
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE media_servers (
                         id INTEGER PRIMARY KEY,
                         kind TEXT NOT NULL,
                         name TEXT NOT NULL,
                         base_url TEXT NOT NULL,
                         token TEXT NOT NULL,
                         user_id TEXT NOT NULL,
                         user_name TEXT NOT NULL,
                         server_version TEXT,
                         added_at INTEGER NOT NULL,
                         last_connected_at INTEGER,
                         UNIQUE(kind, base_url, user_id)
                     );
                     INSERT INTO media_servers(
                         id, kind, name, base_url, token, user_id, user_name, added_at
                     ) VALUES (
                         1, 'emby', 'HomeHub', 'https://media.example',
                         'token', 'user-id', 'Guest', 1
                     );
                     PRAGMA user_version = 2;",
                )
                .unwrap();
        }

        let database = Database::new(path.clone());
        database.initialize().unwrap();
        let server = database.media_server(1).unwrap();
        assert_eq!(server.password, None);
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        drop(connection);
        drop(database);

        for candidate in [
            path.clone(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }
}
