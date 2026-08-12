# mpv-enjoy Home

一个轻量、跨平台的 mpv 媒体首页，为[mpv-enjoy](https://github.com/HosamaJJF/mpv-enjoy)制作，理论也可用于其他的mpv系播放器。它负责管理本地媒体目录和 Emby/Jellyfin服务器、建立轻量索引并通过独立进程启动用户选择的 mpv 发行版，视频播放仍然交由mpv本身。

这是一个过渡阶段的项目，随着[mpv-enjoy](https://github.com/HosamaJJF/mpv-enjoy)的进展，未来可能会嵌入libmpv并停止对单独的首页前端和拉起外部mpv功能的支持。

## 本地开发

需要 Node.js 22、Rust 1.92 和当前平台的 Tauri 系统依赖。

```sh
npm ci
npm run check
npm run tauri dev
```

仅构建前端静态资源：

```sh
npm run build
```

Rust 检查与测试：

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## 发布产物

Release 附件的架构名称由项目的上传脚本统一，不直接沿用 Rust target triple。对外使用与
mpv-enjoy 一致的 `windows-x64`、`macos-arm64` 和 `macos-x64`：

- `mpv-enjoy-home-<版本>-windows-x64-setup.exe`
- `mpv-enjoy-home-<版本>-windows-x64.msi`
- `mpv-enjoy-home-<版本>-windows-x64.zip`
- `mpv-enjoy-home-<版本>-macos-arm64.dmg`
- `mpv-enjoy-home-<版本>-macos-x64.dmg`

Windows ZIP 是无需安装的独立程序包，不包含 mpv；应用数据仍写入系统应用数据目录，不会
改为写在程序旁。macOS 只发布 DMG，不另行发布 `.app` 压缩包。

## 播放器发现顺序

首页按以下顺序寻找播放器：

1. 设置界面中用户选择的可执行文件；
2. `MPV_ENJOY_HOME_PLAYER` 环境变量；
3. 首页可执行文件同目录下的 `mpv.exe`、`mpv-player`、`mpv` 或 `mpv-bin`；
4. `PATH` 中的 `mpv`。

## 数据目录

默认使用 Tauri 的应用数据目录，数据库文件名为`mpv-enjoy-home.sqlite3`。数据库存储目录路径、文件索引以及用户显式配置的媒体服务器连接；不会复制或修改媒体文件。用户名和密码登录只用密码换取服务器访问令牌，密码不会保存；访问令牌保存在本机数据库中，也不会返回给WebView。通过非可信网络连接时应使用 HTTPS。

## 许可证

Copyright (c) 2026 HosamaJJF。项目自有代码采用 MIT License。第三方 Rust crate 和 npm
包保留各自许可证；正式发行前必须生成并随包提供第三方许可证清单与 SBOM。
