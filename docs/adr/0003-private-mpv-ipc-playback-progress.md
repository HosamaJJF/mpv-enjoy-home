# ADR 0003：私有 mpv IPC 与远程播放进度同步

- 状态：Accepted
- 日期：2026-08-04

## 背景

Emby/Jellyfin 的用户条目包含中断点，但仅把直播放址交给外部 mpv 不会自动恢复位置，也不
会在播放结束后更新服务器记录。解析 mpv 终端输出不稳定，而固定或网络可访问的 IPC 会扩大
mpv 无认证命令接口的攻击面。

## 决策

1. 获取远程条目时同时请求 `UserData`、`RunTimeTicks` 与 `MediaSources`。每个播放列表条目把
   `PlaybackPositionTicks` 转成自己的 per-file `--start` 参数；位置不合法或为零时不传。
2. `ProcessPlayerBackend` 仍以参数数组启动独立 mpv，不经过 shell。每次远程播放单独生成
   UUID v4：Unix 在 `/tmp` 创建 mode-0700 的随机目录并放置 socket，Windows 使用随机命名
   管道；端点不监听网络、不复用、播放器退出后立即清理。
3. 后台监控线程通过 mpv JSON IPC 每秒采样播放列表位置、时间、时长、暂停、静音、音量和
   倍速。它在条目开始时调用 `Sessions/Playing`，至少每 10 秒及状态变化时调用
   `Sessions/Playing/Progress`，换集或 mpv 退出时调用 `Sessions/Playing/Stopped`。
4. 回报使用与浏览相同的 Rust 认证客户端和同一个 `PlaySessionId`，不向 WebView 暴露访问
   令牌。回报失败只影响服务器同步，不终止 mpv，也不阻塞 Tauri 命令返回。
5. IPC 只用于观察已由应用启动的进程，不开放任意 UI 命令或远程控制能力。应用退出会结束
   监控；不会为了进度同步阻止用户关闭首页或播放器。

## 依赖与许可证

- `serde_json` 用于严格编码和解析逐行 JSON IPC；许可证为 MIT/Apache-2.0。
- `uuid` 仅使用 `v4` 生成不可预测的单次端点名；许可证为 MIT/Apache-2.0。
- 两项依赖已存在于 Tauri 依赖图，本项目将版本锁定到 `Cargo.lock` 中的具体版本，不引入运行
  时下载、遥测或远程代码。

## 后果

- 远程播放会像原生媒体客户端一样从中断点开始，并在播放、暂停、换集和退出时更新服务器
  状态；服务器仍负责判断完成播放和是否清零中断点。
- mpv 进程继续独立于 WebView，插件、渲染与用户配置不受接管。
- 如果媒体服务器在播放中离线，播放器继续工作，但离线期间的进度可能无法补报；当前版本
  不把认证信息或播放事件持久化到重试队列。
