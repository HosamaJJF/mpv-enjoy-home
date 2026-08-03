# mpv-enjoy Home

一个轻量、跨平台、可复用的 mpv 媒体首页。它负责管理本地媒体目录、建立轻量索引并通过
独立进程启动用户选择的 mpv 发行版；播放器本身仍负责解码、渲染、脚本和快捷键。

项目目前处于技术样板阶段，已经提供：

- Windows 10/11 x64、macOS Apple Silicon 和 macOS Intel 的 Tauri 2 工程骨架；
- Svelte 5 媒体首页、媒体库和播放器设置界面；
- SQLite 媒体目录与媒体项索引；
- 本地目录选择、重新扫描和移除；
- 通过参数数组安全启动外部 mpv，不经过 shell；
- `PlayerBackend` 边界，为后续 JSON IPC 或 libmpv 适配预留空间。

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
`mpv-enjoy-home.sqlite3`。数据库只存储目录路径和文件索引，不复制或修改媒体文件。

架构与扩展原则见 [docs/architecture.md](docs/architecture.md)，面向 Agent 和维护者的完整
约束见 [AGENTS.md](AGENTS.md)。

## 许可证

项目自有代码采用 MIT License。第三方 Rust crate 和 npm 包保留各自许可证；正式发行前
必须生成并随包提供第三方许可证清单与 SBOM。
