use crate::domain::{DiscoveredMedia, is_media_extension, natural_cmp};
use crate::error::{AppError, AppResult};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn scan_media(root: &Path) -> AppResult<Vec<DiscoveredMedia>> {
    if !root.is_dir() {
        return Err(AppError::message("所选路径不是可读取的目录"));
    }

    let root = root.canonicalize()?;
    let mut pending = vec![root.clone()];
    let mut media = Vec::new();

    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) if directory != root => continue,
            Err(error) => return Err(error.into()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }

            let extension = match path.extension().and_then(|value| value.to_str()) {
                Some(value) => value.to_lowercase(),
                None => continue,
            };
            if !is_media_extension(&extension) {
                continue;
            }
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let relative_path = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let modified_at = metadata
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            media.push(DiscoveredMedia {
                name,
                path,
                relative_path,
                extension,
                modified_at,
            });
        }
    }

    media.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    Ok(media)
}
