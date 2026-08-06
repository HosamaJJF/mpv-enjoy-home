# mpv-enjoy Home

一个轻量、跨平台、可复用的 mpv 媒体首页。它负责管理本地媒体目录和 Emby/Jellyfin
服务器、建立轻量索引并通过独立进程启动用户选择的 mpv 发行版；播放器本身仍负责解码、
渲染、脚本和快捷键。

项目目前处于技术样板阶段，已经提供：

- Windows 10/11 x64、macOS Apple Silicon 和 macOS Intel 的 Tauri 2 工程骨架；
- Svelte 5 媒体首页、分层媒体库和播放器设置界面；
- SQLite 媒体目录与媒体项索引；
- 本地目录选择、重新扫描和移除；
- 按本地文件夹层级浏览，并在首页合并展示本地文件和远程媒体的最近更新；
- 使用用户名/密码连接 Emby/Jellyfin；顶层按媒体库展示，库内直接呈现电影与剧集元数据，不暴露重复的物理文件夹；
- 电影与剧集统一详情页展示横幅、简介、播放进度、观看状态、季封面与简介、季筛选、单集简介和演职人员；只有一季时直接显示该季单集；
- 按服务器返回的宽高比展示封面，网格随窗口宽度自动填满、缩放和增加列数，少量条目受最大卡片宽度约束；没有封面时不渲染空白缩略图占位；
- 单集按季号和集号排序；启动 mpv 时加入同目录或同剧集的有序播放列表，并加载匹配的外挂字幕和外挂音轨；
- Emby/Jellyfin 远程视频从服务器中断点继续播放，并通过私有 mpv JSON IPC 定期及退出时回传进度；
- 通过参数数组安全启动外部 mpv，不经过 shell；
- `PlayerBackend` 边界，为后续 libmpv 适配预留空间。

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

## 播放器发现顺序

首页按以下顺序寻找播放器：

1. 设置界面中用户选择的可执行文件；
2. `MPV_ENJOY_HOME_PLAYER` 环境变量；
3. 首页可执行文件同目录下的 `mpv.exe`、`mpv-player`、`mpv` 或 `mpv-bin`；
4. `PATH` 中的 `mpv`。

mpv-enjoy 在组装发行包时只需把首页放到现有播放器入口旁边，或设置上述环境变量。

## 数据目录

开发版和独立发行版默认使用 Tauri 的应用数据目录，数据库文件名为
`mpv-enjoy-home.sqlite3`。数据库存储目录路径、文件索引以及用户显式配置的媒体服务器连接；
不会复制或修改媒体文件。用户名和密码登录只用密码换取服务器访问令牌，密码不会保存；
访问令牌保存在本机数据库中，也不会返回给 WebView。通过非可信网络连接时应使用 HTTPS。

架构与扩展原则见 [docs/architecture.md](docs/architecture.md)，重要技术决策记录在
[docs/adr/](docs/adr/) 中。

## 许可证

Copyright (c) 2026 HosamaJJF。项目自有代码采用 MIT License。第三方 Rust crate 和 npm
包保留各自许可证；正式发行前必须生成并随包提供第三方许可证清单与 SBOM。
