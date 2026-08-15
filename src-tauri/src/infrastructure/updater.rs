use crate::domain::{
    AppInstallType, UpdateAction, UpdateApplyResult, UpdateCheckResult, UpdateReleaseAsset,
    is_newer_version, parse_version,
};
use crate::error::AppError;
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

const DEFAULT_REPOSITORY: &str = "HosamaJJF/mpv-enjoy-home";
const DEFAULT_ASSET_PREFIX: &str = "mpv-enjoy-home";
const DEFAULT_DISTRIBUTION_NAME: &str = "mpv-enjoy Home";
const DEFAULT_PORTABLE_MARKER: &str = ".mpv-enjoy-home-portable";
const MAX_RELEASE_METADATA_BYTES: u64 = 2 * 1024 * 1024;
const MAX_UPDATE_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;

const BUILD_UPDATE_REPOSITORY: Option<&str> = option_env!("MPV_ENJOY_HOME_UPDATE_REPOSITORY");
const BUILD_ASSET_PREFIX: Option<&str> = option_env!("MPV_ENJOY_HOME_UPDATE_ASSET_PREFIX");
const BUILD_DISTRIBUTION_NAME: Option<&str> = option_env!("MPV_ENJOY_HOME_DISTRIBUTION_NAME");
const BUILD_DISTRIBUTION_VERSION: Option<&str> = option_env!("MPV_ENJOY_HOME_DISTRIBUTION_VERSION");
const BUILD_PORTABLE_MARKER: Option<&str> = option_env!("MPV_ENJOY_HOME_PORTABLE_MARKER");

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateSourceConfig {
    repo_owner: String,
    repo_name: String,
    asset_prefix: String,
    distribution_name: String,
    current_version: String,
    #[cfg(any(target_os = "windows", test))]
    portable_marker: String,
    api_endpoint: Option<Url>,
}

#[derive(Debug)]
struct ValidatedAsset {
    name: String,
    download_url: Url,
    size: u64,
    sha256: String,
}

pub struct UpdateManager {
    config: UpdateSourceConfig,
}

impl UpdateSourceConfig {
    fn from_build_metadata() -> Result<Self, AppError> {
        let configured_values = [
            BUILD_UPDATE_REPOSITORY,
            BUILD_ASSET_PREFIX,
            BUILD_DISTRIBUTION_NAME,
            BUILD_DISTRIBUTION_VERSION,
            BUILD_PORTABLE_MARKER,
        ];
        let has_any_override = configured_values.iter().any(Option::is_some);
        let has_all_overrides = configured_values.iter().all(Option::is_some);

        if has_any_override && !has_all_overrides {
            return Err(AppError(
                "发行版更新配置不完整：必须同时设置仓库、附件前缀、名称、版本和便携标记"
                    .to_string(),
            ));
        }

        let (repository, asset_prefix, distribution_name, current_version, portable_marker) =
            if has_all_overrides {
                (
                    BUILD_UPDATE_REPOSITORY.unwrap_or_default(),
                    BUILD_ASSET_PREFIX.unwrap_or_default(),
                    BUILD_DISTRIBUTION_NAME.unwrap_or_default(),
                    BUILD_DISTRIBUTION_VERSION.unwrap_or_default().to_string(),
                    BUILD_PORTABLE_MARKER.unwrap_or_default(),
                )
            } else {
                (
                    DEFAULT_REPOSITORY,
                    DEFAULT_ASSET_PREFIX,
                    DEFAULT_DISTRIBUTION_NAME,
                    env!("CARGO_PKG_VERSION").to_string(),
                    DEFAULT_PORTABLE_MARKER,
                )
            };

        #[cfg(debug_assertions)]
        let current_version =
            std::env::var("MPV_ENJOY_FORCE_CURRENT_VERSION").unwrap_or(current_version);

        let api_endpoint = debug_update_endpoint()?;
        let (repo_owner, repo_name) = parse_repository(repository)?;
        validate_file_token(asset_prefix, "附件前缀")?;
        validate_file_token(portable_marker, "便携标记")?;
        parse_version(&current_version).map_err(AppError)?;

        Ok(Self {
            repo_owner,
            repo_name,
            asset_prefix: asset_prefix.to_string(),
            distribution_name: distribution_name.to_string(),
            current_version,
            #[cfg(any(target_os = "windows", test))]
            portable_marker: portable_marker.to_string(),
            api_endpoint,
        })
    }
}

impl UpdateManager {
    pub fn new() -> Result<Self, AppError> {
        Ok(Self {
            config: UpdateSourceConfig::from_build_metadata()?,
        })
    }

    #[cfg(test)]
    fn with_config(config: UpdateSourceConfig) -> Self {
        Self { config }
    }

    pub fn check_for_updates(&self) -> Result<UpdateCheckResult, AppError> {
        let release = self.fetch_release()?;
        self.check_result(&release)
    }

    pub fn download_latest_update(&self) -> Result<UpdateApplyResult, AppError> {
        let release = self.fetch_release()?;
        if !is_newer_version(&release.tag_name, &self.config.current_version).map_err(AppError)? {
            return Err(AppError("当前已经是最新版本".to_string()));
        }

        let install_type = self.current_install_type();
        let asset = self
            .validated_asset(&release, install_type)?
            .ok_or_else(|| {
                AppError("没有找到经过校验且与当前安装类型严格匹配的更新包".to_string())
            })?;
        let download_dir = create_private_download_directory()?;
        let target_path = download_dir.join(&asset.name);

        if let Err(error) = self.download_asset(&asset, &target_path) {
            let _ = fs::remove_dir_all(&download_dir);
            return Err(error);
        }

        self.open_downloaded_asset(install_type, &target_path)
    }

    pub fn open_release_page(&self) -> Result<(), AppError> {
        let url = Url::parse(&format!(
            "https://github.com/{}/{}/releases/latest",
            self.config.repo_owner, self.config.repo_name
        ))
        .map_err(|error| AppError(format!("构造 Release 页面地址失败：{error}")))?;
        open_url(&url)
    }

    fn current_install_type(&self) -> AppInstallType {
        #[cfg(target_os = "windows")]
        {
            let executable_directory = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf));
            if executable_directory
                .as_ref()
                .is_some_and(|directory| directory.join(&self.config.portable_marker).is_file())
            {
                AppInstallType::WindowsPortable
            } else if executable_directory
                .is_some_and(|directory| directory.join("uninstall.exe").is_file())
            {
                AppInstallType::WindowsSetup
            } else {
                AppInstallType::WindowsInstalledUnknown
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            AppInstallType::MacApp
        }
    }

    fn fetch_release(&self) -> Result<GitHubRelease, AppError> {
        let endpoint = self.release_api_endpoint()?;
        validate_release_endpoint(&endpoint, self.config.api_endpoint.is_some())?;

        let client = Client::builder()
            .timeout(Duration::from_secs(12))
            .redirect(Policy::none())
            .build()
            .map_err(|error| AppError(format!("创建更新网络客户端失败：{error}")))?;

        let response = client
            .get(endpoint)
            .header(
                "User-Agent",
                format!("mpv-enjoy-home/{}", self.config.current_version),
            )
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .map_err(|error| AppError(format!("检查更新失败，请检查网络连接：{error}")))?;

        if !response.status().is_success() {
            return Err(AppError(format!(
                "获取更新信息失败，服务器返回 HTTP {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RELEASE_METADATA_BYTES)
        {
            return Err(AppError("更新信息响应过大".to_string()));
        }

        let mut body = Vec::new();
        response
            .take(MAX_RELEASE_METADATA_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| AppError(format!("读取更新信息失败：{error}")))?;
        if body.len() as u64 > MAX_RELEASE_METADATA_BYTES {
            return Err(AppError("更新信息响应过大".to_string()));
        }

        serde_json::from_slice(&body)
            .map_err(|error| AppError(format!("解析更新信息失败：{error}")))
    }

    fn release_api_endpoint(&self) -> Result<Url, AppError> {
        if let Some(endpoint) = &self.config.api_endpoint {
            return Ok(endpoint.clone());
        }
        Url::parse(&format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.config.repo_owner, self.config.repo_name
        ))
        .map_err(|error| AppError(format!("构造更新接口地址失败：{error}")))
    }

    fn check_result(&self, release: &GitHubRelease) -> Result<UpdateCheckResult, AppError> {
        let latest_version = parse_version(&release.tag_name)
            .map_err(AppError)?
            .to_string();
        let has_update =
            is_newer_version(&latest_version, &self.config.current_version).map_err(AppError)?;
        let install_type = self.current_install_type();
        let matched_asset = if has_update {
            self.validated_asset(release, install_type)?
                .map(|asset| UpdateReleaseAsset {
                    name: asset.name,
                    size: asset.size,
                })
        } else {
            None
        };
        let release_name = release
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&release.tag_name)
            .to_string();

        Ok(UpdateCheckResult {
            current_version: parse_version(&self.config.current_version)
                .map_err(AppError)?
                .to_string(),
            latest_version,
            has_update,
            release_name,
            release_notes: release.body.clone().unwrap_or_default(),
            published_at: release.published_at.clone(),
            install_type,
            matched_asset,
            distribution_name: self.config.distribution_name.clone(),
        })
    }

    fn validated_asset(
        &self,
        release: &GitHubRelease,
        install_type: AppInstallType,
    ) -> Result<Option<ValidatedAsset>, AppError> {
        let version = parse_version(&release.tag_name)
            .map_err(AppError)?
            .to_string();
        let Some(expected_name) = self.expected_asset_name(install_type, &version) else {
            return Ok(None);
        };
        let Some(asset) = release
            .assets
            .iter()
            .find(|asset| asset.name == expected_name)
        else {
            return Ok(None);
        };

        if asset.size == 0 || asset.size > MAX_UPDATE_DOWNLOAD_BYTES {
            return Ok(None);
        }
        let Some(sha256) = asset.digest.as_deref().and_then(parse_sha256_digest) else {
            return Ok(None);
        };
        let download_url = Url::parse(&asset.browser_download_url)
            .map_err(|error| AppError(format!("更新包下载地址无效：{error}")))?;
        if !self.is_exact_asset_url(&download_url, &release.tag_name, &expected_name) {
            return Ok(None);
        }

        Ok(Some(ValidatedAsset {
            name: asset.name.clone(),
            download_url,
            size: asset.size,
            sha256,
        }))
    }

    fn expected_asset_name(&self, install_type: AppInstallType, version: &str) -> Option<String> {
        let platform = match install_type {
            AppInstallType::MacApp if cfg!(target_arch = "aarch64") => "macos-arm64.dmg",
            AppInstallType::MacApp => "macos-x64.dmg",
            AppInstallType::WindowsSetup => "windows-x64-setup.exe",
            AppInstallType::WindowsPortable => "windows-x64.zip",
            AppInstallType::WindowsInstalledUnknown => return None,
        };
        Some(format!("{}-{version}-{platform}", self.config.asset_prefix))
    }

    fn is_exact_asset_url(&self, url: &Url, tag: &str, asset_name: &str) -> bool {
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path()
                == format!(
                    "/{}/{}/releases/download/{tag}/{asset_name}",
                    self.config.repo_owner, self.config.repo_name
                )
    }

    fn download_asset(&self, asset: &ValidatedAsset, target_path: &Path) -> Result<(), AppError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(600))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("更新包重定向次数过多");
                }
                if is_allowed_asset_transport_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("更新包重定向到了不受信任的地址")
                }
            }))
            .build()
            .map_err(|error| AppError(format!("创建下载客户端失败：{error}")))?;
        let response = client
            .get(asset.download_url.clone())
            .header(
                "User-Agent",
                format!("mpv-enjoy-home/{}", self.config.current_version),
            )
            .send()
            .map_err(|error| AppError(format!("下载更新失败：{error}")))?;

        if !response.status().is_success() {
            return Err(AppError(format!(
                "下载更新文件失败，服务器返回 HTTP {}",
                response.status()
            )));
        }
        if !is_allowed_asset_transport_url(response.url()) {
            return Err(AppError("更新包来自不受信任的地址".to_string()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_UPDATE_DOWNLOAD_BYTES || length > asset.size)
        {
            return Err(AppError("更新包响应大小超过 Release 声明".to_string()));
        }

        write_verified_download(response, target_path, asset.size, &asset.sha256)
    }

    fn open_downloaded_asset(
        &self,
        install_type: AppInstallType,
        target_path: &Path,
    ) -> Result<UpdateApplyResult, AppError> {
        match install_type {
            AppInstallType::MacApp => {
                open_macos_file(target_path)?;
                Ok(UpdateApplyResult {
                    action: UpdateAction::OpenedDmg,
                    message: "更新镜像已校验并打开，请将新版本拖入“应用程序”，然后重新打开应用。"
                        .to_string(),
                })
            }
            AppInstallType::WindowsSetup => {
                start_windows_installer(target_path)?;
                Ok(UpdateApplyResult {
                    action: UpdateAction::StartedInstaller,
                    message: "更新安装包已校验并启动，请按安装程序提示完成更新。".to_string(),
                })
            }
            AppInstallType::WindowsPortable => {
                reveal_windows_file(target_path)?;
                Ok(UpdateApplyResult {
                    action: UpdateAction::DownloadedPortableArchive,
                    message: "免安装版更新包已校验并定位。请退出应用后手动解压覆盖当前目录。"
                        .to_string(),
                })
            }
            AppInstallType::WindowsInstalledUnknown => Err(AppError(
                "无法确认当前 Windows 安装器类型，请前往 Release 页面手动更新".to_string(),
            )),
        }
    }
}

fn parse_repository(repository: &str) -> Result<(String, String), AppError> {
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if parts.next().is_some() || owner.is_empty() || name.is_empty() {
        return Err(AppError("更新仓库必须使用 owner/name 格式".to_string()));
    }
    validate_file_token(owner, "仓库所有者")?;
    validate_file_token(name, "仓库名称")?;
    Ok((owner.to_string(), name.to_string()))
}

fn validate_file_token(value: &str, label: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 128
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(AppError(format!("{label}包含不安全字符")));
    }
    Ok(())
}

fn debug_update_endpoint() -> Result<Option<Url>, AppError> {
    #[cfg(debug_assertions)]
    {
        std::env::var("MPV_ENJOY_UPDATE_URL")
            .ok()
            .map(|value| {
                Url::parse(&value)
                    .map_err(|error| AppError(format!("测试更新接口地址无效：{error}")))
            })
            .transpose()
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(None)
    }
}

fn validate_release_endpoint(url: &Url, is_debug_override: bool) -> Result<(), AppError> {
    if is_debug_override {
        let is_loopback = url
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());
        if is_loopback && url.scheme() == "http" {
            return Ok(());
        }
    }
    if url.scheme() == "https" && url.host_str() == Some("api.github.com") {
        return Ok(());
    }
    Err(AppError(
        "更新信息接口不是受信任的 GitHub API 地址".to_string(),
    ))
}

fn parse_sha256_digest(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:")?;
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(digest.to_ascii_lowercase())
    } else {
        None
    }
}

fn is_allowed_asset_transport_url(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

fn create_private_download_directory() -> Result<PathBuf, AppError> {
    let path =
        std::env::temp_dir().join(format!("mpv-enjoy-home-update-{}", Uuid::new_v4().simple()));
    fs::create_dir(&path).map_err(|error| AppError(format!("创建临时更新目录失败：{error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|error| AppError(format!("设置临时更新目录权限失败：{error}")))?;
    }
    Ok(path)
}

fn write_verified_download(
    mut reader: impl Read,
    target_path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), AppError> {
    if expected_size == 0 || expected_size > MAX_UPDATE_DOWNLOAD_BYTES {
        return Err(AppError("更新包声明大小无效".to_string()));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target_path)
        .map_err(|error| AppError(format!("创建临时更新文件失败：{error}")))?;
    let result = (|| {
        let mut buffer = vec![0_u8; DOWNLOAD_BUFFER_BYTES];
        let mut downloaded = 0_u64;
        let mut hasher = Sha256::new();
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|error| AppError(format!("读取更新文件失败：{error}")))?;
            if count == 0 {
                break;
            }
            downloaded = downloaded
                .checked_add(count as u64)
                .ok_or_else(|| AppError("更新包大小溢出".to_string()))?;
            if downloaded > expected_size || downloaded > MAX_UPDATE_DOWNLOAD_BYTES {
                return Err(AppError("更新包实际大小超过 Release 声明".to_string()));
            }
            hasher.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .map_err(|error| AppError(format!("保存更新文件失败：{error}")))?;
        }
        if downloaded != expected_size {
            return Err(AppError(format!(
                "更新包大小不匹配：声明 {expected_size} 字节，实际 {downloaded} 字节"
            )));
        }
        let actual_sha256 = format!("{:x}", hasher.finalize());
        if actual_sha256 != expected_sha256 {
            return Err(AppError("更新包 SHA-256 校验失败".to_string()));
        }
        file.sync_all()
            .map_err(|error| AppError(format!("同步更新文件失败：{error}")))?;
        Ok(())
    })();

    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(target_path);
    }
    result
}

#[cfg(target_os = "macos")]
fn open_macos_file(path: &Path) -> Result<(), AppError> {
    let status = Command::new("open")
        .arg(path)
        .status()
        .map_err(|error| AppError(format!("打开 DMG 更新镜像失败：{error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError("打开更新镜像未成功完成".to_string()))
    }
}

#[cfg(not(target_os = "macos"))]
fn open_macos_file(_path: &Path) -> Result<(), AppError> {
    Err(AppError("当前平台不支持打开 DMG".to_string()))
}

#[cfg(target_os = "windows")]
fn start_windows_installer(path: &Path) -> Result<(), AppError> {
    Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| AppError(format!("启动更新安装程序失败：{error}")))
}

#[cfg(not(target_os = "windows"))]
fn start_windows_installer(_path: &Path) -> Result<(), AppError> {
    Err(AppError("当前平台不支持启动 Windows 安装程序".to_string()))
}

#[cfg(target_os = "windows")]
fn reveal_windows_file(path: &Path) -> Result<(), AppError> {
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|error| AppError(format!("定位更新压缩包失败：{error}")))
}

#[cfg(not(target_os = "windows"))]
fn reveal_windows_file(_path: &Path) -> Result<(), AppError> {
    Err(AppError("当前平台不支持定位 Windows 更新包".to_string()))
}

fn open_url(url: &Url) -> Result<(), AppError> {
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err(AppError("只允许打开受信任的 GitHub HTTPS 页面".to_string()));
    }
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url.as_str());
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg(url.as_str());
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url.as_str());
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| AppError(format!("打开 Release 页面失败：{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn config() -> UpdateSourceConfig {
        UpdateSourceConfig {
            repo_owner: "HosamaJJF".to_string(),
            repo_name: "mpv-enjoy-home".to_string(),
            asset_prefix: "mpv-enjoy-home".to_string(),
            distribution_name: "mpv-enjoy Home".to_string(),
            current_version: "1.0.2".to_string(),
            portable_marker: DEFAULT_PORTABLE_MARKER.to_string(),
            api_endpoint: None,
        }
    }

    fn asset(name: &str, tag: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.to_string(),
            browser_download_url: format!(
                "https://github.com/HosamaJJF/mpv-enjoy-home/releases/download/{tag}/{name}"
            ),
            size: 1024,
            digest: Some(format!("sha256:{}", "a".repeat(64))),
        }
    }

    fn release(assets: Vec<GitHubAsset>) -> GitHubRelease {
        GitHubRelease {
            tag_name: "v1.0.3".to_string(),
            name: Some("1.0.3".to_string()),
            body: Some("更新说明".to_string()),
            published_at: Some("2026-08-14T12:00:00Z".to_string()),
            assets,
        }
    }

    #[test]
    fn matches_only_exact_asset_for_install_type() {
        let manager = UpdateManager::with_config(config());
        let release = release(vec![
            asset("mpv-enjoy-home-1.0.3-macos-arm64.dmg", "v1.0.3"),
            asset("mpv-enjoy-home-1.0.3-macos-x64.dmg", "v1.0.3"),
            asset("mpv-enjoy-home-1.0.3-windows-x64-setup.exe", "v1.0.3"),
            asset("mpv-enjoy-home-1.0.3-windows-x64.zip", "v1.0.3"),
            asset("unrelated-1.0.3-windows-x64.zip", "v1.0.3"),
        ]);

        assert_eq!(
            manager
                .validated_asset(&release, AppInstallType::WindowsSetup)
                .unwrap()
                .unwrap()
                .name,
            "mpv-enjoy-home-1.0.3-windows-x64-setup.exe"
        );
        assert_eq!(
            manager
                .validated_asset(&release, AppInstallType::WindowsPortable)
                .unwrap()
                .unwrap()
                .name,
            "mpv-enjoy-home-1.0.3-windows-x64.zip"
        );
    }

    #[test]
    fn refuses_cross_product_architecture_and_install_type_fallbacks() {
        let manager = UpdateManager::with_config(config());
        let release = release(vec![asset("unrelated-1.0.3-windows-x64.zip", "v1.0.3")]);
        assert!(
            manager
                .validated_asset(&release, AppInstallType::WindowsPortable)
                .unwrap()
                .is_none()
        );
        assert!(
            manager
                .validated_asset(&release, AppInstallType::WindowsSetup)
                .unwrap()
                .is_none()
        );
        assert!(
            manager
                .validated_asset(&release, AppInstallType::MacApp)
                .unwrap()
                .is_none()
        );
        assert!(
            manager
                .validated_asset(&release, AppInstallType::WindowsInstalledUnknown)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_assets_without_valid_digest_size_or_url() {
        let manager = UpdateManager::with_config(config());
        let expected = "mpv-enjoy-home-1.0.3-windows-x64.zip";

        let mut invalid_digest = asset(expected, "v1.0.3");
        invalid_digest.digest = None;
        assert!(
            manager
                .validated_asset(
                    &release(vec![invalid_digest]),
                    AppInstallType::WindowsPortable
                )
                .unwrap()
                .is_none()
        );

        let mut invalid_size = asset(expected, "v1.0.3");
        invalid_size.size = MAX_UPDATE_DOWNLOAD_BYTES + 1;
        assert!(
            manager
                .validated_asset(
                    &release(vec![invalid_size]),
                    AppInstallType::WindowsPortable
                )
                .unwrap()
                .is_none()
        );

        let mut invalid_url = asset(expected, "v1.0.3");
        invalid_url.browser_download_url = "https://example.com/update.zip".to_string();
        assert!(
            manager
                .validated_asset(&release(vec![invalid_url]), AppInstallType::WindowsPortable)
                .unwrap()
                .is_none()
        );

        let mut url_with_query = asset(expected, "v1.0.3");
        url_with_query.browser_download_url.push_str("?redirect=1");
        assert!(
            manager
                .validated_asset(
                    &release(vec![url_with_query]),
                    AppInstallType::WindowsPortable
                )
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn limits_download_redirects_to_exact_github_hosts() {
        assert!(is_allowed_asset_transport_url(
            &Url::parse("https://release-assets.githubusercontent.com/file").unwrap()
        ));
        assert!(!is_allowed_asset_transport_url(
            &Url::parse("http://release-assets.githubusercontent.com/file").unwrap()
        ));
        assert!(!is_allowed_asset_transport_url(
            &Url::parse("https://release-assets.githubusercontent.com.example.com/file").unwrap()
        ));
    }

    #[test]
    fn verifies_download_size_and_digest_before_keeping_file() {
        let body = b"verified update";
        let digest = format!("{:x}", Sha256::digest(body));
        let directory = std::env::temp_dir().join(format!("updater-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("update.bin");

        write_verified_download(Cursor::new(body), &target, body.len() as u64, &digest).unwrap();
        assert_eq!(fs::read(&target).unwrap(), body);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn removes_partial_file_when_download_validation_fails() {
        let body = b"tampered update";
        let directory = std::env::temp_dir().join(format!("updater-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let target = directory.join("update.bin");

        let error = write_verified_download(
            Cursor::new(body),
            &target,
            body.len() as u64,
            &"0".repeat(64),
        )
        .unwrap_err();
        assert!(error.0.contains("SHA-256"));
        assert!(!target.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parses_repository_and_rejects_path_tokens() {
        assert_eq!(
            parse_repository("HosamaJJF/mpv-enjoy-home").unwrap(),
            ("HosamaJJF".to_string(), "mpv-enjoy-home".to_string())
        );
        assert!(parse_repository("owner/repo/extra").is_err());
        assert!(parse_repository("../repo").is_err());
        assert!(validate_file_token("../../update", "附件前缀").is_err());
    }

    #[test]
    fn supports_explicit_mpv_enjoy_release_layout() {
        let mut integrated = config();
        integrated.repo_name = "mpv-enjoy".to_string();
        integrated.asset_prefix = "mpv-enjoy".to_string();
        integrated.distribution_name = "mpv-enjoy 整合包".to_string();
        integrated.current_version = "1.2.1".to_string();
        integrated.portable_marker = ".mpv-enjoy-portable".to_string();
        let manager = UpdateManager::with_config(integrated);
        let release = GitHubRelease {
            tag_name: "v1.2.2".to_string(),
            name: Some("mpv-enjoy 1.2.2".to_string()),
            body: None,
            published_at: None,
            assets: vec![GitHubAsset {
                name: "mpv-enjoy-1.2.2-windows-x64.zip".to_string(),
                browser_download_url:
                    "https://github.com/HosamaJJF/mpv-enjoy/releases/download/v1.2.2/mpv-enjoy-1.2.2-windows-x64.zip"
                        .to_string(),
                size: 106_819_218,
                digest: Some(format!("sha256:{}", "b".repeat(64))),
            }],
        };

        assert_eq!(
            manager
                .validated_asset(&release, AppInstallType::WindowsPortable)
                .unwrap()
                .unwrap()
                .name,
            "mpv-enjoy-1.2.2-windows-x64.zip"
        );
    }

    #[test]
    #[ignore = "需要在受限沙箱外绑定本机回环端口"]
    fn checks_updates_against_mock_endpoint() {
        use std::io::Write as _;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let asset_name = if cfg!(target_os = "windows") {
            "mpv-enjoy-home-9.9.9-windows-x64-setup.exe"
        } else if cfg!(target_arch = "aarch64") {
            "mpv-enjoy-home-9.9.9-macos-arm64.dmg"
        } else {
            "mpv-enjoy-home-9.9.9-macos-x64.dmg"
        };
        let body = format!(
            r##"{{
                "tag_name": "v9.9.9",
                "name": "9.9.9",
                "body": "新功能说明",
                "published_at": "2026-08-14T12:00:00Z",
                "assets": [{{
                    "name": "{asset_name}",
                    "browser_download_url": "https://github.com/HosamaJJF/mpv-enjoy-home/releases/download/v9.9.9/{asset_name}",
                    "size": 1024,
                    "digest": "sha256:{}"
                }}]
            }}"##,
            "c".repeat(64)
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });

        let mut test_config = config();
        test_config.api_endpoint =
            Some(Url::parse(&format!("http://127.0.0.1:{port}/releases/latest")).unwrap());
        let result = UpdateManager::with_config(test_config)
            .check_for_updates()
            .unwrap();
        assert_eq!(result.latest_version, "9.9.9");
        assert!(result.has_update);
        assert!(result.matched_asset.is_some());
        server.join().unwrap();
    }
}
