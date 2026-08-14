use crate::domain::{
    AppInstallType, UpdateApplyResult, UpdateCheckResult, UpdateReleaseAsset, is_newer_version,
};
use crate::error::AppError;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Clone)]
pub struct UpdateSourceConfig {
    pub repo_owner: String,
    pub repo_name: String,
    pub asset_prefix: String,
    pub distribution_name: String,
    pub current_version: String,
    pub api_endpoint: Option<String>,
}

pub struct UpdateManager {
    config: UpdateSourceConfig,
}

impl UpdateManager {
    pub fn new() -> Self {
        Self {
            config: Self::detect_distribution(),
        }
    }

    #[cfg(test)]
    pub fn with_config(config: UpdateSourceConfig) -> Self {
        Self { config }
    }

    pub fn detect_distribution() -> UpdateSourceConfig {
        let current_version = std::env::var("MPV_ENJOY_FORCE_CURRENT_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

        let is_integrated = if std::env::var_os("MPV_ENJOY_DISTRIBUTION").is_some() {
            true
        } else if let Ok(current_exe) = std::env::current_exe() {
            let exe_str = current_exe.to_string_lossy().to_lowercase();
            let parent_dir = current_exe.parent();

            exe_str.contains("mpv-enjoy.app")
                || parent_dir
                    .map(|dir| {
                        dir.join("portable_config").exists() || dir.join("mpv-player.exe").exists()
                    })
                    .unwrap_or(false)
        } else {
            false
        };

        if is_integrated {
            UpdateSourceConfig {
                repo_owner: "HosamaJJF".to_string(),
                repo_name: "mpv-enjoy".to_string(),
                asset_prefix: "mpv-enjoy".to_string(),
                distribution_name: "mpv-enjoy 整合包".to_string(),
                current_version,
                api_endpoint: None,
            }
        } else {
            UpdateSourceConfig {
                repo_owner: "HosamaJJF".to_string(),
                repo_name: "mpv-enjoy-home".to_string(),
                asset_prefix: "mpv-enjoy-home".to_string(),
                distribution_name: "mpv-enjoy Home".to_string(),
                current_version,
                api_endpoint: None,
            }
        }
    }

    pub fn current_install_type() -> AppInstallType {
        #[cfg(target_os = "windows")]
        {
            if let Ok(current_exe) = std::env::current_exe() {
                if let Some(parent) = current_exe.parent() {
                    if parent.join("uninstall.exe").exists() {
                        return AppInstallType::WindowsSetup;
                    }
                }
            }
            AppInstallType::WindowsPortable
        }
        #[cfg(not(target_os = "windows"))]
        {
            AppInstallType::MacApp
        }
    }

    pub fn check_for_updates(&self) -> Result<UpdateCheckResult, AppError> {
        let url = self.config.api_endpoint.clone().unwrap_or_else(|| {
            std::env::var("MPV_ENJOY_UPDATE_URL").unwrap_or_else(|_| {
                format!(
                    "https://api.github.com/repos/{}/{}/releases/latest",
                    self.config.repo_owner, self.config.repo_name
                )
            })
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|err| AppError(format!("创建更新网络客户端失败：{err}")))?;

        let response = client
            .get(&url)
            .header(
                "User-Agent",
                format!("mpv-enjoy-home/{}", self.config.current_version),
            )
            .header("Accept", "application/vnd.github+json")
            .send()
            .map_err(|err| AppError(format!("检查更新失败，请检查网络连接：{err}")))?;

        if !response.status().is_success() {
            return Err(AppError(format!(
                "获取更新信息失败，服务器返回 HTTP {}",
                response.status()
            )));
        }

        let release: GitHubRelease = response
            .json()
            .map_err(|err| AppError(format!("解析更新信息失败：{err}")))?;

        let latest_version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
        let has_update = is_newer_version(&latest_version, &self.config.current_version);
        let install_type = Self::current_install_type();

        let matched_asset = self.match_release_asset(&release.assets, install_type);

        let release_name = release
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| release.tag_name.clone());

        Ok(UpdateCheckResult {
            current_version: self.config.current_version.clone(),
            latest_version,
            has_update,
            release_name,
            release_notes: release.body.unwrap_or_default(),
            published_at: release.published_at,
            release_url: release.html_url,
            install_type,
            matched_asset,
            distribution_name: self.config.distribution_name.clone(),
        })
    }

    fn match_release_asset(
        &self,
        assets: &[GitHubAsset],
        install_type: AppInstallType,
    ) -> Option<UpdateReleaseAsset> {
        if assets.is_empty() {
            return None;
        }

        let target_platform_suffix = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "macos-arm64"
        } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            "macos-x64"
        } else if cfg!(target_os = "windows") {
            "windows-x64"
        } else {
            ""
        };
        let prefix = self.config.asset_prefix.to_lowercase();

        let found = match install_type {
            AppInstallType::MacApp => assets
                .iter()
                .find(|asset| {
                    let name = asset.name.to_lowercase();
                    name.starts_with(&prefix)
                        && name.ends_with(".dmg")
                        && (name.contains(target_platform_suffix) || name.contains("darwin"))
                })
                .or_else(|| {
                    assets.iter().find(|asset| {
                        let name = asset.name.to_lowercase();
                        name.ends_with(".dmg")
                            && (name.contains(target_platform_suffix) || name.contains("darwin"))
                    })
                })
                .or_else(|| {
                    assets
                        .iter()
                        .find(|asset| asset.name.to_lowercase().ends_with(".dmg"))
                }),
            AppInstallType::WindowsSetup => assets
                .iter()
                .find(|asset| {
                    let name = asset.name.to_lowercase();
                    name.starts_with(&prefix)
                        && name.contains("windows-x64")
                        && (name.ends_with("-setup.exe") || name.ends_with(".msi"))
                })
                .or_else(|| {
                    assets.iter().find(|asset| {
                        let name = asset.name.to_lowercase();
                        name.contains("windows-x64")
                            && (name.ends_with("-setup.exe") || name.ends_with(".msi"))
                    })
                })
                .or_else(|| {
                    assets.iter().find(|asset| {
                        let name = asset.name.to_lowercase();
                        name.ends_with("-setup.exe")
                            || (name.ends_with(".exe") && !name.ends_with(".zip"))
                    })
                }),
            AppInstallType::WindowsPortable => assets
                .iter()
                .find(|asset| {
                    let name = asset.name.to_lowercase();
                    name.starts_with(&prefix)
                        && name.contains("windows-x64")
                        && name.ends_with(".zip")
                })
                .or_else(|| {
                    assets.iter().find(|asset| {
                        let name = asset.name.to_lowercase();
                        name.contains("windows-x64") && name.ends_with(".zip")
                    })
                })
                .or_else(|| {
                    assets
                        .iter()
                        .find(|asset| asset.name.to_lowercase().ends_with(".zip"))
                }),
        };

        found.map(|asset| UpdateReleaseAsset {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
            size: asset.size,
        })
    }

    pub fn download_and_apply(
        &self,
        download_url: &str,
        file_name: &str,
    ) -> Result<UpdateApplyResult, AppError> {
        let temp_dir = std::env::temp_dir().join("mpv-enjoy-home-updater");
        fs::create_dir_all(&temp_dir)
            .map_err(|err| AppError(format!("创建临时更新目录失败：{err}")))?;

        let target_file_path = temp_dir.join(file_name);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|err| AppError(format!("创建下载客户端失败：{err}")))?;

        let mut response = client
            .get(download_url)
            .header(
                "User-Agent",
                format!("mpv-enjoy-home/{}", self.config.current_version),
            )
            .send()
            .map_err(|err| AppError(format!("下载更新失败：{err}")))?;

        if !response.status().is_success() {
            return Err(AppError(format!(
                "下载更新文件失败，服务器返回 HTTP {}",
                response.status()
            )));
        }

        let mut dest_file = File::create(&target_file_path)
            .map_err(|err| AppError(format!("创建临时文件失败：{err}")))?;

        io::copy(&mut response, &mut dest_file)
            .map_err(|err| AppError(format!("保存更新文件失败：{err}")))?;

        let install_type = Self::current_install_type();

        match install_type {
            AppInstallType::MacApp => {
                let status = Command::new("open")
                    .arg(&target_file_path)
                    .status()
                    .map_err(|err| AppError(format!("打开 DMG 更新镜像失败：{err}")))?;

                if !status.success() {
                    return Err(AppError("打开更新镜像未成功完成".to_string()));
                }

                Ok(UpdateApplyResult {
                    action: "opened_dmg".to_string(),
                    message:
                        "已下载并打开更新镜像，请将新版本拖入“应用程序”完成覆盖，然后重新打开应用。"
                            .to_string(),
                    requires_restart: false,
                })
            }
            AppInstallType::WindowsSetup => {
                Command::new(&target_file_path)
                    .spawn()
                    .map_err(|err| AppError(format!("启动更新安装程序失败：{err}")))?;

                Ok(UpdateApplyResult {
                    action: "started_installer".to_string(),
                    message: "已启动安装程序，请按提示完成安装。".to_string(),
                    requires_restart: true,
                })
            }
            AppInstallType::WindowsPortable => {
                self.apply_windows_portable_update(&target_file_path)
            }
        }
    }

    fn apply_windows_portable_update(
        &self,
        zip_path: &Path,
    ) -> Result<UpdateApplyResult, AppError> {
        let extract_dir = zip_path.with_extension("extracted");
        if extract_dir.exists() {
            let _ = fs::remove_dir_all(&extract_dir);
        }
        fs::create_dir_all(&extract_dir)
            .map_err(|err| AppError(format!("创建解压目录失败：{err}")))?;

        let zip_file =
            File::open(zip_path).map_err(|err| AppError(format!("打开更新压缩包失败：{err}")))?;
        let mut archive = zip::ZipArchive::new(BufReader::new(zip_file))
            .map_err(|err| AppError(format!("解析更新压缩包失败：{err}")))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|err| AppError(format!("读取压缩包条目失败：{err}")))?;
            let outpath = match file.enclosed_name() {
                Some(path) => extract_dir.join(path),
                None => continue,
            };

            if file.is_dir() {
                fs::create_dir_all(&outpath)
                    .map_err(|err| AppError(format!("创建解压子目录失败：{err}")))?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)
                            .map_err(|err| AppError(format!("创建解压子目录失败：{err}")))?;
                    }
                }
                let mut outfile = File::create(&outpath)
                    .map_err(|err| AppError(format!("释放更新文件失败：{err}")))?;
                io::copy(&mut file, &mut outfile)
                    .map_err(|err| AppError(format!("写入更新文件失败：{err}")))?;
            }
        }

        let current_exe = std::env::current_exe()
            .map_err(|err| AppError(format!("定位当前程序路径失败：{err}")))?;
        let current_dir = current_exe
            .parent()
            .ok_or_else(|| AppError("定位当前程序目录失败".to_string()))?;

        let old_exe = current_exe.with_extension("exe.old");
        if old_exe.exists() {
            let _ = fs::remove_file(&old_exe);
        }

        fs::rename(&current_exe, &old_exe)
            .map_err(|err| AppError(format!("准备覆盖当前程序失败：{err}")))?;

        let source_root = if let Ok(mut entries) = fs::read_dir(&extract_dir) {
            let entries_vec: Vec<_> = entries.by_ref().filter_map(|e| e.ok()).collect();
            if entries_vec.len() == 1 && entries_vec[0].path().is_dir() {
                entries_vec[0].path()
            } else {
                extract_dir.clone()
            }
        } else {
            extract_dir.clone()
        };

        Self::copy_dir_recursive(&source_root, current_dir)?;

        Command::new(&current_exe)
            .current_dir(current_dir)
            .spawn()
            .map_err(|err| AppError(format!("启动新版本程序失败：{err}")))?;

        Ok(UpdateApplyResult {
            action: "replaced_portable".to_string(),
            message: "已完成免安装版覆盖更新，正在重启新版本。".to_string(),
            requires_restart: true,
        })
    }

    fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
        if !dst.exists() {
            fs::create_dir_all(dst).map_err(|err| AppError(format!("创建目录失败：{err}")))?;
        }

        for entry in fs::read_dir(src).map_err(|err| AppError(format!("读取目录失败：{err}")))?
        {
            let entry = entry.map_err(|err| AppError(format!("读取文件项失败：{err}")))?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                Self::copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)
                    .map_err(|err| AppError(format!("复制更新文件失败：{err}")))?;
            }
        }
        Ok(())
    }

    pub fn cleanup_old_files() {
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                let old_exe = current_exe.with_extension("exe.old");
                if old_exe.exists() {
                    let _ = fs::remove_file(old_exe);
                }
                if let Ok(entries) = fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(ext) = path.extension() {
                            if ext == "old" || ext == "pending_delete" {
                                let _ = fs::remove_file(path);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_install_type() {
        let install_type = UpdateManager::current_install_type();
        #[cfg(target_os = "macos")]
        assert_eq!(install_type, AppInstallType::MacApp);
        #[cfg(target_os = "windows")]
        assert!(matches!(
            install_type,
            AppInstallType::WindowsSetup | AppInstallType::WindowsPortable
        ));
    }

    #[test]
    fn matches_correct_asset_for_platform() {
        let manager = UpdateManager::with_config(UpdateSourceConfig {
            repo_owner: "HosamaJJF".to_string(),
            repo_name: "mpv-enjoy-home".to_string(),
            asset_prefix: "mpv-enjoy-home".to_string(),
            distribution_name: "mpv-enjoy Home".to_string(),
            current_version: "1.0.2".to_string(),
            api_endpoint: None,
        });

        let assets = vec![
            GitHubAsset {
                name: "mpv-enjoy-home-1.0.3-macos-arm64.dmg".to_string(),
                browser_download_url: "https://example.com/arm64.dmg".to_string(),
                size: 1024,
            },
            GitHubAsset {
                name: "mpv-enjoy-home-1.0.3-macos-x64.dmg".to_string(),
                browser_download_url: "https://example.com/x64.dmg".to_string(),
                size: 1024,
            },
            GitHubAsset {
                name: "mpv-enjoy-home-1.0.3-windows-x64-setup.exe".to_string(),
                browser_download_url: "https://example.com/setup.exe".to_string(),
                size: 2048,
            },
            GitHubAsset {
                name: "mpv-enjoy-home-1.0.3-windows-x64.zip".to_string(),
                browser_download_url: "https://example.com/portable.zip".to_string(),
                size: 2048,
            },
        ];

        let mac_asset = manager.match_release_asset(&assets, AppInstallType::MacApp);
        assert!(mac_asset.is_some());
        assert!(mac_asset.unwrap().name.ends_with(".dmg"));

        let win_setup_asset = manager.match_release_asset(&assets, AppInstallType::WindowsSetup);
        assert!(win_setup_asset.is_some());
        assert_eq!(
            win_setup_asset.unwrap().name,
            "mpv-enjoy-home-1.0.3-windows-x64-setup.exe"
        );

        let win_portable_asset =
            manager.match_release_asset(&assets, AppInstallType::WindowsPortable);
        assert!(win_portable_asset.is_some());
        assert_eq!(
            win_portable_asset.unwrap().name,
            "mpv-enjoy-home-1.0.3-windows-x64.zip"
        );
    }

    #[test]
    #[ignore = "需要绑定本机回环端口"]
    fn checks_updates_against_mock_endpoint() {
        use std::io::Write;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let mock_server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let body = r##"{
                "tag_name": "v9.9.9",
                "name": "v9.9.9 带来更强大的功能",
                "body": "新功能说明：支持多平台更新",
                "html_url": "https://github.com/HosamaJJF/mpv-enjoy-home/releases/tag/v9.9.9",
                "published_at": "2026-08-14T12:00:00Z",
                "assets": [
                    {
                        "name": "mpv-enjoy-home-9.9.9-macos-arm64.dmg",
                        "browser_download_url": "https://github.com/example/arm64.dmg",
                        "size": 42000000
                    },
                    {
                        "name": "mpv-enjoy-home-9.9.9-macos-x64.dmg",
                        "browser_download_url": "https://github.com/example/x64.dmg",
                        "size": 42000000
                    },
                    {
                        "name": "mpv-enjoy-home-9.9.9-windows-x64-setup.exe",
                        "browser_download_url": "https://github.com/example/setup.exe",
                        "size": 45000000
                    },
                    {
                        "name": "mpv-enjoy-home-9.9.9-windows-x64.zip",
                        "browser_download_url": "https://github.com/example/portable.zip",
                        "size": 35000000
                    }
                ]
            }"##;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let test_url = format!("http://127.0.0.1:{port}/releases/latest");
        let manager = UpdateManager::with_config(UpdateSourceConfig {
            repo_owner: "HosamaJJF".to_string(),
            repo_name: "mpv-enjoy-home".to_string(),
            asset_prefix: "mpv-enjoy-home".to_string(),
            distribution_name: "mpv-enjoy Home".to_string(),
            current_version: "1.0.2".to_string(),
            api_endpoint: Some(test_url),
        });

        let result = manager.check_for_updates().unwrap();

        assert_eq!(result.current_version, "1.0.2");
        assert_eq!(result.latest_version, "9.9.9");
        assert!(result.has_update);
        assert_eq!(result.release_name, "v9.9.9 带来更强大的功能");
        assert!(result.matched_asset.is_some());

        mock_server.join().unwrap();
    }

    #[test]
    fn parses_and_matches_mock_release_json() {
        let body = r##"{
            "tag_name": "v9.9.9",
            "name": "v9.9.9 带来更强大的功能",
            "body": "新功能说明：支持多平台更新",
            "html_url": "https://github.com/HosamaJJF/mpv-enjoy-home/releases/tag/v9.9.9",
            "published_at": "2026-08-14T12:00:00Z",
            "assets": [
                {
                    "name": "mpv-enjoy-home-9.9.9-macos-arm64.dmg",
                    "browser_download_url": "https://github.com/example/arm64.dmg",
                    "size": 42000000
                },
                {
                    "name": "mpv-enjoy-home-9.9.9-macos-x64.dmg",
                    "browser_download_url": "https://github.com/example/x64.dmg",
                    "size": 42000000
                },
                {
                    "name": "mpv-enjoy-home-9.9.9-windows-x64-setup.exe",
                    "browser_download_url": "https://github.com/example/setup.exe",
                    "size": 45000000
                },
                {
                    "name": "mpv-enjoy-home-9.9.9-windows-x64.zip",
                    "browser_download_url": "https://github.com/example/portable.zip",
                    "size": 35000000
                }
            ]
        }"##;

        let release: GitHubRelease = serde_json::from_str(body).unwrap();
        let latest_version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
        assert!(is_newer_version(&latest_version, "1.0.2"));

        let manager = UpdateManager::with_config(UpdateSourceConfig {
            repo_owner: "HosamaJJF".to_string(),
            repo_name: "mpv-enjoy-home".to_string(),
            asset_prefix: "mpv-enjoy-home".to_string(),
            distribution_name: "mpv-enjoy Home".to_string(),
            current_version: "1.0.2".to_string(),
            api_endpoint: None,
        });

        let win_zip = manager.match_release_asset(&release.assets, AppInstallType::WindowsPortable);
        assert!(win_zip.is_some());
        assert_eq!(win_zip.unwrap().name, "mpv-enjoy-home-9.9.9-windows-x64.zip");
    }

    #[test]
    fn matches_mpv_enjoy_distribution_releases_correctly() {
        let manager = UpdateManager::with_config(UpdateSourceConfig {
            repo_owner: "HosamaJJF".to_string(),
            repo_name: "mpv-enjoy".to_string(),
            asset_prefix: "mpv-enjoy".to_string(),
            distribution_name: "mpv-enjoy 整合包".to_string(),
            current_version: "1.0.2".to_string(),
            api_endpoint: None,
        });

        let assets = vec![
            GitHubAsset {
                name: "mpv-enjoy-1.2.2-macos-arm64.dmg".to_string(),
                browser_download_url: "https://example.com/mpv-enjoy-arm64.dmg".to_string(),
                size: 99721028,
            },
            GitHubAsset {
                name: "mpv-enjoy-1.2.2-macos-x64.dmg".to_string(),
                browser_download_url: "https://example.com/mpv-enjoy-x64.dmg".to_string(),
                size: 68479745,
            },
            GitHubAsset {
                name: "mpv-enjoy-1.2.2-windows-x64.zip".to_string(),
                browser_download_url: "https://example.com/mpv-enjoy-win-x64.zip".to_string(),
                size: 106819218,
            },
            GitHubAsset {
                name: "SHA256SUMS".to_string(),
                browser_download_url: "https://example.com/SHA256SUMS".to_string(),
                size: 292,
            },
        ];

        let mac_asset = manager.match_release_asset(&assets, AppInstallType::MacApp);
        assert!(mac_asset.is_some());
        assert!(mac_asset.unwrap().name.starts_with("mpv-enjoy-"));

        let win_portable_asset =
            manager.match_release_asset(&assets, AppInstallType::WindowsPortable);
        assert!(win_portable_asset.is_some());
        assert_eq!(
            win_portable_asset.unwrap().name,
            "mpv-enjoy-1.2.2-windows-x64.zip"
        );
    }
}
