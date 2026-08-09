use crate::domain::{PlayerPreferences, PlayerStatus, PlayerToggleMode};
use crate::error::{AppError, AppResult};
use crate::infrastructure::mpv_ipc::{
    MpvIpcEndpoint, monitor_local_playback, monitor_remote_playback,
};
use crate::infrastructure::remote::RemotePlayback;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PLAYER_SETTING: &str = "player.executable";

pub trait PlayerBackend {
    fn status(&self, configured: Option<&str>) -> PlayerStatus;
    fn play_local(
        &self,
        configured: Option<&str>,
        preferences: &PlayerPreferences,
        media: &[PathBuf],
        start_index: usize,
    ) -> AppResult<()>;
    fn play_remote(
        &self,
        configured: Option<&str>,
        preferences: &PlayerPreferences,
        playback: &RemotePlayback,
    ) -> AppResult<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessPlayerBackend;

impl PlayerBackend for ProcessPlayerBackend {
    fn status(&self, configured: Option<&str>) -> PlayerStatus {
        discover_player(configured)
    }

    fn play_local(
        &self,
        configured: Option<&str>,
        preferences: &PlayerPreferences,
        media: &[PathBuf],
        start_index: usize,
    ) -> AppResult<()> {
        let endpoint = (preferences.danmaku_mode != PlayerToggleMode::Inherit)
            .then(MpvIpcEndpoint::create)
            .transpose()
            .ok()
            .flatten();
        let arguments = local_playback_arguments(
            preferences,
            endpoint.as_ref().map(MpvIpcEndpoint::address),
            media,
            start_index,
        )?;
        let status = discover_player(configured);
        let executable = status
            .executable
            .ok_or_else(|| AppError::message("尚未找到 mpv，请先在设置中选择播放器"))?;
        let child = Command::new(executable)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| AppError::message(format!("无法启动播放器：{error}")))?;
        if let Some(endpoint) = endpoint {
            monitor_local_playback(child, endpoint, preferences.danmaku_mode);
        }
        Ok(())
    }

    fn play_remote(
        &self,
        configured: Option<&str>,
        preferences: &PlayerPreferences,
        playback: &RemotePlayback,
    ) -> AppResult<()> {
        let endpoint = MpvIpcEndpoint::create().ok();
        let arguments = remote_playback_arguments(
            preferences,
            playback,
            endpoint.as_ref().map(MpvIpcEndpoint::address),
        )?;
        let status = discover_player(configured);
        let executable = status
            .executable
            .ok_or_else(|| AppError::message("尚未找到 mpv，请先在设置中选择播放器"))?;
        let child = Command::new(executable)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| AppError::message(format!("无法启动播放器：{error}")))?;
        if let Some(endpoint) = endpoint {
            monitor_remote_playback(child, endpoint, playback.clone(), preferences.danmaku_mode);
        }
        Ok(())
    }
}

fn local_playback_arguments(
    preferences: &PlayerPreferences,
    ipc_address: Option<&OsStr>,
    media: &[PathBuf],
    start_index: usize,
) -> AppResult<Vec<OsString>> {
    validate_queue(media.len(), start_index)?;
    if media.iter().any(|path| !path.is_file()) {
        return Err(AppError::message(
            "播放列表中的媒体文件已被移动、删除或暂时不可用",
        ));
    }
    let mut arguments = preference_arguments(preferences);
    arguments.extend([
        OsString::from("--autoload-files=yes"),
        OsString::from("--sub-auto=exact"),
        OsString::from("--audio-file-auto=exact"),
        OsString::from(format!("--playlist-start={start_index}")),
    ]);
    push_ipc_argument(&mut arguments, ipc_address);
    arguments.push(OsString::from("--"));
    arguments.extend(media.iter().map(|path| path.as_os_str().to_os_string()));
    Ok(arguments)
}

fn remote_playback_arguments(
    preferences: &PlayerPreferences,
    playback: &RemotePlayback,
    ipc_address: Option<&OsStr>,
) -> AppResult<Vec<OsString>> {
    validate_queue(playback.items.len(), playback.start_index)?;
    for item in &playback.items {
        validate_remote_url(&item.url)?;
        validate_media_title(&item.title)?;
        for url in item.subtitle_urls.iter().chain(&item.audio_urls) {
            validate_remote_url(url)?;
        }
    }

    let mut arguments = preference_arguments(preferences);
    arguments.extend([
        OsString::from("--force-window=immediate"),
        // User mpv profiles may display `${filename}` or `${path}` when a
        // file starts. For remote playback that reveals the full URL,
        // including its access token, so suppress only this startup message.
        OsString::from("--osd-playing-msg="),
        OsString::from("--script-opts-append=autoload-disabled=yes"),
        OsString::from(format!("--playlist-start={}", playback.start_index)),
    ]);
    push_ipc_argument(&mut arguments, ipc_address);
    for item in &playback.items {
        // Remote URLs are constructed and validated by RemoteClient. mpv's
        // per-file markers keep each episode's sidecar tracks from leaking
        // into the next playlist entry.
        arguments.push(OsString::from("--{"));
        arguments.push(OsString::from(format!(
            "--force-media-title={}",
            item.title
        )));
        if item.start_position_ticks > 0 {
            arguments.push(OsString::from(format!(
                "--start={:.3}",
                item.start_position_ticks as f64 / 10_000_000.0
            )));
        }
        arguments.extend(
            item.subtitle_urls
                .iter()
                .map(|url| OsString::from(format!("--sub-file={url}"))),
        );
        arguments.extend(
            item.audio_urls
                .iter()
                .map(|url| OsString::from(format!("--audio-file={url}"))),
        );
        arguments.push(OsString::from(&item.url));
        arguments.push(OsString::from("--}"));
    }
    Ok(arguments)
}

fn preference_arguments(preferences: &PlayerPreferences) -> Vec<OsString> {
    let mut arguments = Vec::new();
    if let Some(volume) = preferences.startup_volume {
        arguments.push(OsString::from(format!("--volume={volume}")));
    }
    match preferences.fullscreen_mode {
        PlayerToggleMode::Inherit => {}
        PlayerToggleMode::On => arguments.push(OsString::from("--fullscreen=yes")),
        PlayerToggleMode::Off => arguments.push(OsString::from("--fullscreen=no")),
    }
    arguments
}

fn push_ipc_argument(arguments: &mut Vec<OsString>, ipc_address: Option<&OsStr>) {
    if let Some(address) = ipc_address {
        let mut argument = OsString::from("--input-ipc-server=");
        argument.push(address);
        arguments.push(argument);
    }
}

fn validate_queue(length: usize, start_index: usize) -> AppResult<()> {
    if length == 0 || start_index >= length {
        return Err(AppError::message("播放列表为空或起始位置无效"));
    }
    Ok(())
}

fn validate_remote_url(value: &str) -> AppResult<()> {
    if matches!(value.split(':').next(), Some("http" | "https")) {
        Ok(())
    } else {
        Err(AppError::message("远程播放地址只支持 HTTP 或 HTTPS"))
    }
}

fn validate_media_title(value: &str) -> AppResult<()> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        Err(AppError::message("远程媒体标题格式无效"))
    } else {
        Ok(())
    }
}

pub fn normalize_selected_player(path: &Path) -> AppResult<PathBuf> {
    let candidate = if path.is_dir() && path.extension().is_some_and(|value| value == "app") {
        let macos = path.join("Contents/MacOS");
        ["mpv", "mpv-player", "mpv-bin"]
            .iter()
            .map(|name| macos.join(name))
            .find(|value| value.is_file())
            .ok_or_else(|| AppError::message("所选 App 中没有可识别的 mpv 可执行文件"))?
    } else {
        path.to_path_buf()
    };
    if !candidate.is_file() {
        return Err(AppError::message("所选播放器不是可用的文件"));
    }
    candidate.canonicalize().map_err(Into::into)
}

fn discover_player(configured: Option<&str>) -> PlayerStatus {
    if let Some(path) = configured {
        return status_for_candidate(PathBuf::from(path), "configured");
    }
    if let Some(path) = env::var_os("MPV_ENJOY_HOME_PLAYER") {
        let status = status_for_candidate(PathBuf::from(path), "environment");
        if status.available {
            return status;
        }
    }
    if let Ok(current) = env::current_exe()
        && let Some(directory) = current.parent()
    {
        let names: &[&str] = if cfg!(windows) {
            &["mpv.exe"]
        } else {
            &["mpv-player", "mpv", "mpv-bin"]
        };
        for name in names {
            let status = status_for_candidate(directory.join(name), "bundled");
            if status.available {
                return status;
            }
        }
    }
    if let Some(path) = find_on_path(if cfg!(windows) { "mpv.exe" } else { "mpv" }) {
        return status_for_candidate(path, "path");
    }
    PlayerStatus {
        available: false,
        executable: None,
        source: "unavailable".into(),
    }
}

fn status_for_candidate(path: PathBuf, source: &str) -> PlayerStatus {
    let available = path.is_file();
    PlayerStatus {
        available,
        executable: Some(path.to_string_lossy().into_owned()),
        source: source.into(),
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| env::split_paths(&paths).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::remote::RemotePlaybackItem;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn local_queue_starts_at_selected_item_and_enables_sidecars() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir().join(format!("mpv-enjoy-player-{nonce}"));
        fs::create_dir_all(&directory).unwrap();
        let first = directory.join("S01E01.mkv");
        let second = directory.join("S01E02.mkv");
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();

        let arguments = local_playback_arguments(
            &PlayerPreferences::default(),
            None,
            &[first.clone(), second.clone()],
            1,
        )
        .unwrap();
        assert!(arguments.contains(&OsString::from("--sub-auto=exact")));
        assert!(arguments.contains(&OsString::from("--audio-file-auto=exact")));
        assert!(arguments.contains(&OsString::from("--playlist-start=1")));
        let separator = arguments
            .iter()
            .position(|argument| argument == "--")
            .unwrap();
        assert_eq!(arguments[separator + 1], first.as_os_str());
        assert_eq!(arguments[separator + 2], second.as_os_str());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn remote_queue_binds_external_tracks_to_each_episode() {
        let playback = RemotePlayback {
            items: vec![
                RemotePlaybackItem {
                    item_id: "1".to_string(),
                    media_source_id: Some("source".to_string()),
                    url: "https://media.example/Videos/1/stream.mkv?api_key=token".to_string(),
                    title: "Series S01E01 - First".to_string(),
                    subtitle_urls: vec![
                        "https://media.example/Videos/1/source/Subtitles/2/Stream.ass?api_key=token".to_string(),
                    ],
                    audio_urls: Vec::new(),
                    start_position_ticks: 0,
                    run_time_ticks: Some(1_400_000_000),
                },
                RemotePlaybackItem {
                    item_id: "2".to_string(),
                    media_source_id: Some("source".to_string()),
                    url: "https://media.example/Videos/2/stream.mkv?api_key=token".to_string(),
                    title: "Series S01E02 - Second".to_string(),
                    subtitle_urls: vec![
                        "https://media.example/Videos/2/source/Subtitles/3/Stream.ass?api_key=token".to_string(),
                    ],
                    audio_urls: vec![
                        "https://media.example/Audio/2/stream.aac?api_key=token".to_string(),
                    ],
                    start_position_ticks: 123_450_000,
                    run_time_ticks: Some(1_400_000_000),
                },
            ],
            start_index: 1,
            reporter: None,
        };

        let arguments = remote_playback_arguments(
            &PlayerPreferences::default(),
            &playback,
            Some(OsStr::new("/tmp/mpv.sock")),
        )
        .unwrap();
        assert!(arguments.contains(&OsString::from("--playlist-start=1")));
        assert!(arguments.contains(&OsString::from(
            "--script-opts-append=autoload-disabled=yes"
        )));
        assert!(arguments.contains(&OsString::from("--osd-playing-msg=")));
        assert!(!arguments.contains(&OsString::from("--osd-level=0")));
        assert!(!arguments.contains(&OsString::from("--autoload-files=no")));
        assert!(arguments.contains(&OsString::from("--input-ipc-server=/tmp/mpv.sock")));
        assert!(arguments.contains(&OsString::from("--start=12.345")));
        assert!(arguments.contains(&OsString::from(
            "--sub-file=https://media.example/Videos/2/source/Subtitles/3/Stream.ass?api_key=token"
        )));
        assert!(arguments.contains(&OsString::from(
            "--audio-file=https://media.example/Audio/2/stream.aac?api_key=token"
        )));
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| argument.as_os_str() == "--{")
                .count(),
            2
        );
        assert_eq!(
            arguments
                .iter()
                .filter(|argument| argument.as_os_str() == "--}")
                .count(),
            2
        );
        let first_group = arguments
            .iter()
            .position(|argument| argument.as_os_str() == "--{")
            .unwrap();
        assert_eq!(
            &arguments[first_group..],
            &[
                OsString::from("--{"),
                OsString::from("--force-media-title=Series S01E01 - First"),
                OsString::from(
                    "--sub-file=https://media.example/Videos/1/source/Subtitles/2/Stream.ass?api_key=token"
                ),
                OsString::from("https://media.example/Videos/1/stream.mkv?api_key=token"),
                OsString::from("--}"),
                OsString::from("--{"),
                OsString::from("--force-media-title=Series S01E02 - Second"),
                OsString::from("--start=12.345"),
                OsString::from(
                    "--sub-file=https://media.example/Videos/2/source/Subtitles/3/Stream.ass?api_key=token"
                ),
                OsString::from(
                    "--audio-file=https://media.example/Audio/2/stream.aac?api_key=token"
                ),
                OsString::from("https://media.example/Videos/2/stream.mkv?api_key=token"),
                OsString::from("--}"),
            ]
        );
    }

    #[test]
    fn typed_preferences_become_global_player_arguments() {
        let preferences = PlayerPreferences {
            startup_volume: Some(72),
            fullscreen_mode: PlayerToggleMode::On,
            danmaku_mode: PlayerToggleMode::Off,
        };
        assert_eq!(
            preference_arguments(&preferences),
            [
                OsString::from("--volume=72"),
                OsString::from("--fullscreen=yes"),
            ]
        );
        assert!(preference_arguments(&PlayerPreferences::default()).is_empty());

        let windowed = PlayerPreferences {
            fullscreen_mode: PlayerToggleMode::Off,
            ..PlayerPreferences::default()
        };
        assert_eq!(
            preference_arguments(&windowed),
            [OsString::from("--fullscreen=no")]
        );
    }
}
