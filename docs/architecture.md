# 架构说明

## 边界

mpv-enjoy Home 只负责媒体发现、索引、媒体服务器浏览、播放请求和经过约束的软件更新，不接管
mpv 的解码、渲染、Lua 脚本或用户配置。当前播放器实现是独立进程，首页不通过 shell 拼接命令。

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
      -> UpdateManager
           -> trusted GitHub Release metadata + verified release asset
```

## 模块

- `src/`：Svelte UI，只处理展示、交互状态和调用类型化命令。
- `src-tauri/src/domain.rs`：跨框架领域模型和媒体类型规则。
- `src-tauri/src/application.rs`：用例编排，不依赖具体 UI。
- `src-tauri/src/infrastructure/`：SQLite、文件扫描、播放器进程和受限更新适配。
- `src-tauri/src/commands.rs`：Tauri 边界，把领域错误转换为可显示错误。

WebView 不获得通用 shell、任意文件系统或任意下载地址能力。目录通过系统选择器获得，Rust 层
仍会验证路径；播放请求使用数据库中的媒体 ID 解析真实路径，更新命令也只接受无参数的“检查”、
“下载匹配包”和“打开项目 Release 页面”意图。

## 数据模型

- `library_folders`：用户显式添加的媒体根目录。
- `media_items`：扫描得到的媒体文件索引。
- `media_servers`：用户显式配置的 Emby/Jellyfin 地址、用户、访问令牌和自动登录密码。
- `settings`：播放器路径、类型化启动偏好、主题模式和主题色等少量本地设置。
- `PRAGMA user_version`：数据库 schema 版本。

扫描默认不跟随符号链接，避免目录环和越界扫描。媒体项同时保存相对路径，UI 通过相对路径
按需展开目录，不再把所有单集放到媒体库顶层。当前重新扫描采用目录级事务替换索引；后续
可在不改变 UI API 的情况下替换成增量扫描。媒体库浏览页可按自然名称或修改时间升降序排列；
本地文件夹的修改时间由其后代视频的最大修改时间即时聚合，因此每一层都能按内部最新视频排列
文件夹，同时保持文件夹和视频两个既有展示分区。从媒体源入口进入本地目录时会先在阻塞线程池
重新扫描一次，再读取当前路径；同一媒体源内逐级导航只复用刚更新的索引。自动扫描失败时保留并
展示上一次成功索引，不因目录暂时不可用而清空媒体库。

## 远程媒体源

Emby 与 Jellyfin 共用窄化后的远程媒体 DTO 和浏览用例。Rust 侧可用用户名和密码调用
`Users/AuthenticateByName` 换取访问令牌；连接入口不接受访问令牌或 API Key。密码保存在平台
应用数据目录的 SQLite 中，仅 Rust 后端读取，不返回 WebView，也不使用系统钥匙串。Unix 下
应用数据目录权限收紧为 `0700`，数据库、WAL 和 SHM 文件收紧为 `0600`。后续浏览请求使用
`Authorization`、
`X-Emby-Authorization` 与 `X-Emby-Token` 兼容请求头。访问令牌不会返回 WebView；封面由
Rust 下载、限制为 8 MiB 后通过 data URL 返回界面。远程播放 URL 只在 Rust 内生成，并使用
同源的 `api_key` 查询参数交给 mpv，使外挂字幕和需要自行读取网络媒体的 mpv 脚本也能认证。
认证请求使用首次运行时生成并保存到 `settings` 的安装级 `DeviceId`，在同一安装内保持稳定，
避免多个设备因共享硬编码标识而在服务器侧混淆会话。
服务端返回 401 时，应用读取已保存密码，在后端串行执行一次重新认证；验证成功后原位更新令牌、
用户信息和服务器版本，并继续原请求，不要求删除媒体源。若旧连接没有密码或密码已经变更，才用
稳定错误标记通知 UI 打开手动重新登录表单。403 保持为权限错误，避免错误地把内容授权问题当成
凭据过期。

远程顶层只展示服务器提供的媒体库。进入媒体库后，后端用 `Recursive=true` 和
`IncludeItemTypes=Movie/Series/Video` 构造元数据视图，不把物理 `Folder` 暴露成重复导航层。
浏览列表可按名称或服务器 `DateCreated` 时间升降序排列；顶层媒体库没有直接采用容器自身时间，
而是为每个库查询库内最新的电影、单集或视频并把该时间作为媒体库更新时间。剧集条目也不使用
系列自身的创建时间，而是按 `SeriesId` 聚合其所有单集，以最新一集的 `DateCreated` 作为更新
时间；没有单集的系列才回退到自身时间。排序只作用于浏览列表，进入剧集详情后不显示排序控件，
也不改变季和集的编号顺序。
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

首页“最近更新”使用统一 DTO 合并本地文件修改时间与各服务器按 `DateCreated` 返回的电影、
单集和视频。远程单集把所属 `SeriesId` 作为详情目标；单个服务器暂时不可达时只跳过该服务
器的最近更新，不阻止本地条目和其他服务器显示。媒体库先展示可换行的来源选择页，选定本地
目录或服务器后再进入内容页，避免把所有来源压进会裁切的横向标签栏。

外观偏好通过类型化 Tauri 命令保存到通用 `settings` 表，不需要为新增主题预设修改数据库
schema。UI 根据“跟随系统 / 明亮 / 黑暗”计算当前有效主题，并只通过集中定义的 design tokens
切换颜色；默认使用蓝色，同时提供粉、绿、黄、紫预设，不在组件内保存散落的颜色常量。
界面文字使用系统 UI 字体和集中定义的字号层级，正文、元数据与徽标分别保持明确的最小字号，
避免依赖仅在个别 WebView 生效的字体平滑属性。多季剧集使用只显示季名称的原生下拉框选择季度，
不随季数增长产生横向滚动；观看进度保留在当前季摘要中。

本地播放列表只包含数据库中已索引、仍存在且与当前视频同目录的普通文件。mpv 以
`--playlist-start` 从用户选择的文件开始，并用 `sub-auto=exact` 与 `audio-file-auto=exact`
加载同名或带语言后缀的外挂轨道。远程外挂字幕使用服务器公开的字幕流接口；所有携带访问
令牌的媒体与轨道 URL 都必须保持与媒体服务器同源。

## 软件更新

`UpdateManager` 是更新来源、安装类型和文件校验的唯一事实来源。默认构建只读取
`HosamaJJF/mpv-enjoy-home` 的最新已发布 Release；集成发行版必须在编译时同时提供仓库、附件
前缀、发行版名称、发行版版本和便携标记五项元数据，不能根据可执行文件路径或播放器名称猜测。
调试构建可以把元数据接口改为本机回环 HTTP 地址以做端到端测试，正式构建不接受运行时更新
地址覆盖。

更新检查只接受严格 SemVer 标签，并按当前平台和安装类型生成唯一附件名。Release 元数据限制为
2 MiB 且不跟随重定向；附件必须具有 GitHub 返回的 `sha256` digest、非零且不超过 512 MiB 的
声明大小，以及与配置仓库、标签和附件名完全一致的 GitHub 下载路径。下载写入权限受限的随机
临时目录，使用 `create_new` 防覆盖，同时限制实际字节数并在打开前验证大小与 SHA-256；中途失败
删除不完整文件。下载重定向只允许 GitHub 的 HTTPS 附件主机。

macOS 只打开验证后的对应架构 DMG。Windows 免安装版必须在可执行文件同目录包含发行时注入的
`.mpv-enjoy-home-portable` 标记，验证 ZIP 后只在资源管理器中定位，由用户退出应用并手动覆盖；
不再让运行中的程序重命名或覆盖自身。Windows 只有发现同目录 `uninstall.exe` 时才把当前副本
视为 NSIS 安装版并启动验证后的 setup，其他无法确认的安装器类型只提供 Release 页面，避免
在 MSI 与 NSIS 之间擅自切换。UI 不接触下载 URL、digest 或临时路径，也不能打开任意外链。

## 播放器适配

`PlayerBackend` 接受已由应用层解析的媒体路径，当前 `ProcessPlayerBackend` 负责发现播放器
并通过参数数组启动。远程播放额外传入 `input-ipc-server`：Unix socket 位于 `/tmp` 下随机
生成的 mode-0700 目录，Windows 使用随机命名管道；监控线程在 mpv 退出后删除 Unix socket
与目录。IPC 不监听网络地址，也不会复用用户配置中的固定端点。

播放器启动偏好通过独立的类型化 DTO 保存，默认均为“跟随 mpv/插件”。用户明确设置启动音量、
全屏状态或 uosc_danmaku 样式时，进程后端把经过范围与枚举验证的全局参数放在媒体参数之前；
弹幕样式使用逐项 `script-opts-append` 覆盖粗体、字号、描边、阴影、滚动时长、不透明度和显示
区域，未设置字段继续读取插件配置。不开放任意参数文本入口，也不写入或改写用户的 mpv 配置。
uosc_danmaku 的显示开关不是普通 mpv 选项：远程播放复用现有私有 IPC，本地播放仅在用户明确
覆盖弹幕状态时创建同等私有的短生命周期 IPC；后台等待媒体加载后读取插件状态，必要时发送
一次脚本消息。样式覆盖本身不要求 IPC。插件或 IPC 不可用时忽略对应偏好，不阻塞播放器与
WebView。

```text
<player executable> -- <media path>
```

集成其他 mpv 发行版时，应新增适配器或构建期布局描述，不要在 Svelte 组件中加入发行版
判断。未来若引入 libmpv，数据库、扫描器和 UI DTO 应保持不变。
