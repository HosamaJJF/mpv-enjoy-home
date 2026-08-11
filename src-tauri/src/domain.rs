use serde::{Deserialize, Serialize};
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
    pub relative_path: String,
    pub extension: String,
    pub modified_at: i64,
}

#[derive(Debug, Clone)]
pub struct DiscoveredMedia {
    pub name: String,
    pub path: PathBuf,
    pub relative_path: String,
    pub extension: String,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub key: String,
    pub name: String,
    pub relative_path: String,
    pub kind: String,
    pub media_id: Option<i64>,
    pub extension: Option<String>,
    pub modified_at: i64,
    pub media_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentMediaItem {
    pub key: String,
    pub source_kind: String,
    pub source_id: i64,
    pub source_name: String,
    pub target_id: String,
    pub target_name: String,
    pub name: String,
    pub context: String,
    pub item_type: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct RemoteRecentMedia {
    pub item_id: String,
    pub target_id: String,
    pub target_name: String,
    pub name: String,
    pub context: String,
    pub item_type: String,
    pub updated_at: i64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaServerInput {
    pub kind: String,
    pub name: String,
    pub base_url: String,
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaServerCredentials {
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaServerSummary {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub base_url: String,
    pub user_id: String,
    pub user_name: String,
    pub server_version: Option<String>,
    pub added_at: i64,
    pub last_connected_at: Option<i64>,
}

#[derive(Clone)]
pub struct MediaServerConfig {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub base_url: String,
    pub token: String,
    pub user_id: String,
    pub user_name: String,
    pub password: Option<String>,
    pub server_version: Option<String>,
    pub added_at: i64,
    pub last_connected_at: Option<i64>,
}

impl From<MediaServerConfig> for MediaServerSummary {
    fn from(server: MediaServerConfig) -> Self {
        Self {
            id: server.id,
            kind: server.kind,
            name: server.name,
            base_url: server.base_url,
            user_id: server.user_id,
            user_name: server.user_name,
            server_version: server.server_version,
            added_at: server.added_at,
            last_connected_at: server.last_connected_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteLibraryEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub item_type: String,
    pub subtitle: Option<String>,
    pub child_count: usize,
    pub has_image: bool,
    pub image_aspect_ratio: Option<f64>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMediaDetail {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub runtime_ticks: Option<i64>,
    pub community_rating: Option<f64>,
    pub official_rating: Option<String>,
    pub played: bool,
    pub playback_position_ticks: i64,
    pub played_percentage: Option<f64>,
    pub primary_image_id: Option<String>,
    pub backdrop_image_id: Option<String>,
    pub seasons: Vec<RemoteSeasonDetail>,
    pub episodes: Vec<RemoteEpisodeDetail>,
    pub people: Vec<RemotePersonDetail>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSeasonDetail {
    pub id: String,
    pub name: String,
    pub index_number: Option<i32>,
    pub overview: Option<String>,
    pub episode_count: usize,
    pub unplayed_count: Option<usize>,
    pub played: bool,
    pub primary_image_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEpisodeDetail {
    pub id: String,
    pub name: String,
    pub overview: Option<String>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub season_id: Option<String>,
    pub premiere_date: Option<String>,
    pub runtime_ticks: Option<i64>,
    pub played: bool,
    pub playback_position_ticks: i64,
    pub played_percentage: Option<f64>,
    pub primary_image_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePersonDetail {
    pub id: Option<String>,
    pub name: String,
    pub role: Option<String>,
    pub person_type: Option<String>,
    pub primary_image_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatus {
    pub available: bool,
    pub executable: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerToggleMode {
    #[default]
    Inherit,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DanmakuStylePreferences {
    pub bold_mode: PlayerToggleMode,
    pub font_size: Option<u8>,
    pub outline: Option<f64>,
    pub shadow: Option<u8>,
    pub scroll_time: Option<u8>,
    pub opacity: Option<f64>,
    pub display_area: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerPreferences {
    pub startup_volume: Option<u8>,
    pub fullscreen_mode: PlayerToggleMode,
    pub danmaku_mode: PlayerToggleMode,
    pub danmaku_style: DanmakuStylePreferences,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccentColor {
    #[default]
    Blue,
    Pink,
    Green,
    Yellow,
    Purple,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme_mode: ThemeMode,
    pub accent_color: AccentColor,
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

    #[test]
    fn older_player_preferences_default_new_danmaku_style_fields() {
        let preferences: PlayerPreferences = serde_json::from_str(
            r#"{"startupVolume":72,"fullscreenMode":"on","danmakuMode":"off"}"#,
        )
        .unwrap();
        assert_eq!(preferences.startup_volume, Some(72));
        assert_eq!(
            preferences.danmaku_style,
            DanmakuStylePreferences::default()
        );
    }
}
