# 架构说明

## 边界

mpv-enjoy Home 只负责媒体发现、索引、浏览和播放请求，不接管 mpv 的解码、渲染、Lua
脚本或用户配置。当前播放器实现是独立进程，首页不通过 shell 拼接命令。

```text
Svelte UI
  -> typed Tauri commands / DTO
    -> application services
      -> SQLite repository
      -> filesystem scanner
      -> PlayerBackend
           -> ProcessPlayerBackend (current)
           -> JSON IPC backend (possible)
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
- `settings`：播放器路径等少量本地设置。
- `PRAGMA user_version`：数据库 schema 版本。

扫描默认不跟随符号链接，避免目录环和越界扫描。当前重新扫描采用目录级事务替换索引；
后续可在不改变 UI API 的情况下替换成增量扫描。

## 播放器适配

`PlayerBackend` 接受已由应用层解析的媒体路径，当前 `ProcessPlayerBackend` 负责发现播放器
并调用：

```text
<player executable> -- <media path>
```

集成其他 mpv 发行版时，应新增适配器或构建期布局描述，不要在 Svelte 组件中加入发行版
判断。未来若引入 libmpv，数据库、扫描器和 UI DTO 应保持不变。
