use serde::Serialize;
use std::cmp::Ordering;
use std::path::PathBuf;

pub const MEDIA_EXTENSIONS: &[&str] = &[
    "3g2", "3gp", "asf", "avi", "flv", "m2ts", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "mts",
    "ogm", "ogv", "rm", "rmvb", "ts", "vob", "webm", "wmv",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub media_count: i64,
    pub added_at: i64,
    pub last_scanned_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: i64,
    pub folder_id: i64,
    pub name: String,
    pub path: String,
    pub extension: String,
    pub modified_at: i64,
}

#[derive(Debug, Clone)]
pub struct DiscoveredMedia {
    pub name: String,
    pub path: PathBuf,
    pub extension: String,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatus {
    pub available: bool,
    pub executable: Option<String>,
    pub source: String,
}

pub fn is_media_extension(extension: &str) -> bool {
    MEDIA_EXTENSIONS.binary_search(&extension).is_ok()
}

pub fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left_bytes.len() && right_index < right_bytes.len() {
        if left_bytes[left_index].is_ascii_digit() && right_bytes[right_index].is_ascii_digit() {
            let left_start = left_index;
            let right_start = right_index;
            while left_index < left_bytes.len() && left_bytes[left_index].is_ascii_digit() {
                left_index += 1;
            }
            while right_index < right_bytes.len() && right_bytes[right_index].is_ascii_digit() {
                right_index += 1;
            }
            let left_number = left[left_start..left_index]
                .trim_start_matches('0')
                .to_string();
            let right_number = right[right_start..right_index]
                .trim_start_matches('0')
                .to_string();
            let length_order = left_number.len().cmp(&right_number.len());
            if length_order != Ordering::Equal {
                return length_order;
            }
            let number_order = left_number.cmp(&right_number);
            if number_order != Ordering::Equal {
                return number_order;
            }
            continue;
        }

        let character_order = left_bytes[left_index].cmp(&right_bytes[right_index]);
        if character_order != Ordering::Equal {
            return character_order;
        }
        left_index += 1;
        right_index += 1;
    }

    left_bytes.len().cmp(&right_bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_extensions() {
        assert!(is_media_extension("mkv"));
        assert!(is_media_extension("mp4"));
        assert!(!is_media_extension("srt"));
    }

    #[test]
    fn sorts_episode_numbers_naturally() {
        let mut names = vec!["Episode 10", "Episode 2", "Episode 01"];
        names.sort_by(|left, right| natural_cmp(left, right));
        assert_eq!(names, vec!["Episode 01", "Episode 2", "Episode 10"]);
    }
}
