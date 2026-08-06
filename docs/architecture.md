# 架构说明

## 边界

mpv-enjoy Home 只负责媒体发现、索引、媒体服务器浏览和播放请求，不接管 mpv 的解码、
渲染、Lua 脚本或用户配置。当前播放器实现是独立进程，首页不通过 shell 拼接命令。

```text
Svelte UI
  -> typed Tauri commands / DTO
    -> application services
      -> SQLite repository
      -> filesystem scanner
      -> RemoteMediaBackend
           -> Emby REST adapter
           -> Jellyfin REST adapter
      -> PlayerBackend
           -> ProcessPlayerBackend + private JSON IPC monitor (current)
           -> libmpv backend (possible)
```

## 模块

- `src/`：Svelte UI，只处理展示、交互状态和调用类型化命令。
- `src-tauri/src/domain.rs`：跨框架领域模型和媒体类型规则。
- `src-tauri/src/application.rs`：用例编排，不依赖具体 UI。
- `src-tauri/src/infrastructure/`：SQLite、文件扫描和播放器进程适配。
- `src-tauri/src/commands.rs`：Tauri 边界，把领域错误转换为可显示错误。

WebView 不获得通用 shell 或任意文件系统能力。目录通过系统选择器获得，Rust 层仍会验证
路径；播放请求使用数据库中的媒体 ID 解析真实路径。

## 数据模型

- `library_folders`：用户显式添加的媒体根目录。
- `media_items`：扫描得到的媒体文件索引。
- `media_servers`：用户显式配置的 Emby/Jellyfin 地址、用户和访问令牌。
- `settings`：播放器路径等少量本地设置。
- `PRAGMA user_version`：数据库 schema 版本。

扫描默认不跟随符号链接，避免目录环和越界扫描。媒体项同时保存相对路径，UI 通过相对路径
按需展开目录，不再把所有单集放到媒体库顶层。当前重新扫描采用目录级事务替换索引；后续
可在不改变 UI API 的情况下替换成增量扫描。

## 远程媒体源

Emby 与 Jellyfin 共用窄化后的远程媒体 DTO 和浏览用例。Rust 侧可用用户名和密码调用
`Users/AuthenticateByName` 换取访问令牌，也可接受用户显式提供的 API Key；密码仅存在于
连接请求的短生命周期 DTO 中，不会写入数据库。后续浏览请求使用 `Authorization`、
`X-Emby-Authorization` 与 `X-Emby-Token` 兼容请求头。访问令牌不会返回 WebView；封面由
Rust 下载、限制为 8 MiB 后通过 data URL 返回界面。远程播放 URL 只在 Rust 内生成，并使用
同源的 `api_key` 查询参数交给 mpv，使外挂字幕和需要自行读取网络媒体的 mpv 脚本也能认证。

远程顶层只展示服务器提供的媒体库。进入媒体库后，后端用 `Recursive=true` 和
`IncludeItemTypes=Movie/Series/Video` 构造元数据视图，不把物理 `Folder` 暴露成重复导航层。
电影与剧集打开统一详情页；剧集的季和集由同一个详情 DTO 返回，季作为筛选器而不是新的页面，
只有一季时隐藏多余筛选控件但保留季封面和季简介。详情 DTO 同时包含系列/电影简介、用户播放
状态和人员；集在后端按 `ParentIndexNumber`、`IndexNumber` 排序，标题只作为
同序号的兜底。Primary、Backdrop 和人员图片经受限图片命令加载。封面使用服务器返回的
`PrimaryImageAspectRatio` 区分横版与竖版，网格使用自适应列填满可用宽度；网格总宽度同时按
当前条目数和卡片最大宽度封顶，避免少量条目被拉伸到整行。发起播放前，后端会重新读取条目并确认
`MediaType` 为 `Video`；单集使用 `SeriesId` 获取最多 500 个按季号、集号排列的同剧集条目，
再把直播放址和每个条目自己的外部字幕/音轨地址以参数数组传给 mpv。远程条目使用 mpv 的
per-file 参数组，并设置包含剧集与季/集号的 `force-media-title`，避免换集串轨并让 mpv 脚本
能识别网络媒体。远程播放同时清空 `osd-playing-msg`，防止用户 mpv 配置在起播时把含令牌的
URL 当成文件名显示；音量、进度和轨道切换等其余 OSD 行为保持不变。播放器保留默认的外挂轨道
选择行为，只通过脚本选项禁用重复的目录自动加载。
当前版本只做 Direct Play，不实现转码选择或远程元数据本地缓存。远程条目读取用户自己的
`UserData.PlaybackPositionTicks`，以 per-file `start` 参数恢复中断点。启动 mpv 时同时建立随机、
短生命周期且仅当前用户可访问的本地 JSON IPC；后台线程读取当前播放列表位置、播放时间、
暂停、静音、音量和倍速，每 10 秒及状态变化时调用媒体服务器的播放开始/进度/停止接口。
换集会先停止上一集会话再开始新一集，mpv 退出时使用最后一次有效位置发送停止回报；网络
回报失败不会中断播放器或阻塞 WebView。

本地播放列表只包含数据库中已索引、仍存在且与当前视频同目录的普通文件。mpv 以
`--playlist-start` 从用户选择的文件开始，并用 `sub-auto=exact` 与 `audio-file-auto=exact`
加载同名或带语言后缀的外挂轨道。远程外挂字幕使用服务器公开的字幕流接口；所有携带访问
令牌的媒体与轨道 URL 都必须保持与媒体服务器同源。

## 播放器适配

`PlayerBackend` 接受已由应用层解析的媒体路径，当前 `ProcessPlayerBackend` 负责发现播放器
并通过参数数组启动。远程播放额外传入 `input-ipc-server`：Unix socket 位于 `/tmp` 下随机
生成的 mode-0700 目录，Windows 使用随机命名管道；监控线程在 mpv 退出后删除 Unix socket
与目录。IPC 不监听网络地址，也不会复用用户配置中的固定端点。

```text
<player executable> -- <media path>
```

集成其他 mpv 发行版时，应新增适配器或构建期布局描述，不要在 Svelte 组件中加入发行版
判断。未来若引入 libmpv，数据库、扫描器和 UI DTO 应保持不变。
