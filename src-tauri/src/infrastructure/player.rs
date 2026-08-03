use crate::domain::PlayerStatus;
use crate::error::{AppError, AppResult};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PLAYER_SETTING: &str = "player.executable";

pub trait PlayerBackend {
    fn status(&self, configured: Option<&str>) -> PlayerStatus;
    fn play(&self, configured: Option<&str>, media: &Path) -> AppResult<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessPlayerBackend;

impl PlayerBackend for ProcessPlayerBackend {
    fn status(&self, configured: Option<&str>) -> PlayerStatus {
        discover_player(configured)
    }

    fn play(&self, configured: Option<&str>, media: &Path) -> AppResult<()> {
        if !media.is_file() {
            return Err(AppError::message("媒体文件已被移动、删除或暂时不可用"));
        }
        let status = discover_player(configured);
        let executable = status
            .executable
            .ok_or_else(|| AppError::message("尚未找到 mpv，请先在设置中选择播放器"))?;
        Command::new(executable)
            .arg("--")
            .arg(media)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| AppError::message(format!("无法启动播放器：{error}")))?;
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
