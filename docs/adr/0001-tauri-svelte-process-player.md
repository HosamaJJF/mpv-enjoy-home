# ADR-0001：使用 Tauri、Svelte 和进程播放器后端

- 状态：Accepted
- 日期：2026-08-03

## 决策

使用 Tauri 2 承载桌面窗口，以 Svelte 5 + TypeScript 构建 UI，以 Rust 实现文件扫描、
SQLite 数据访问和播放器启动。第一版通过独立进程调用 mpv。

## 原因

- 系统 WebView 避免随应用打包完整 Chromium；
- Web UI 能以较低设计成本提供现代且可访问的媒体库界面；
- Rust 层适合安全处理路径、数据库和子进程；
- 独立进程最大限度保留各 mpv 发行版的配置与插件行为；
- `PlayerBackend` 让未来 JSON IPC 或 libmpv 原型不侵入媒体库业务。

## 后果

- Windows 依赖 WebView2，构建需要 Rust MSVC 工具链；
- macOS 使用系统 WKWebView；
- 直接把 libmpv 视频表面嵌入 WebView 并非免费迁移，未来仍需原生渲染适配；
- 正式发行必须维护 Cargo/npm 依赖许可证和 SBOM。
