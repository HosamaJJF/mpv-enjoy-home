# mpv-enjoy Home 开发说明

本文件供开发者和本地 Agent 使用，应随仓库提交。开始修改前请完整阅读。

## 项目定位

mpv-enjoy Home 是独立于具体 mpv 发行版的本地媒体首页。它管理用户显式添加的目录、建立
轻量 SQLite 索引并启动外部 mpv。项目当前支持：

- Windows 10/11 x64
- macOS Apple Silicon（arm64）
- macOS Intel（x86_64）

不要把 mpv、插件、用户配置或平台二进制直接提交到本仓库。发行版集成应在各自构建阶段
注入播放器，或通过稳定的适配器和运行时布局完成。

## 技术栈与事实来源

- Tauri 2：桌面容器和受限命令边界。
- Svelte 5 + TypeScript + Vite：界面。
- Rust：应用服务、路径验证、扫描、SQLite 和子进程。
- `package-lock.json`、`src-tauri/Cargo.lock`：依赖版本事实来源，必须提交。
- `src-tauri/tauri.conf.json`：产品版本、窗口和安全配置事实来源。
- `docs/architecture.md` 与 `docs/adr/`：架构和重要决策。

升级 Tauri、Svelte、Vite、SQLite 或 Rust toolchain 时，不使用浮动 `latest`，应核对官方
Release、兼容范围、许可证和三个目标平台，并重新生成锁文件。

## 架构边界

### UI

`src/` 只能：

- 展示 DTO；
- 管理短生命周期交互状态；
- 调用 `src/api.ts` 中声明的类型化命令；
- 使用系统目录/文件选择器获取用户明确选择的路径。

禁止在 WebView 中开放通用 shell、任意文件读写或远程代码。不要在组件中判断
`mpv-enjoy`、其他发行版名称或播放器安装布局。

### Rust application/domain

- `domain.rs` 保存不依赖 Tauri 的模型和规则。
- `application.rs` 编排用例。
- `commands.rs` 是 Tauri 入口，不在其中堆积业务逻辑。
- `infrastructure/` 保存 SQLite、扫描器和播放器适配器。

新增后端能力时先定义窄接口或数据结构，再接到 Tauri command。不要让数据库 row、
`rusqlite::Connection` 或 `std::process::Child` 穿过 UI 边界。

### 播放器

当前 `ProcessPlayerBackend` 必须：

- 使用参数数组启动，不经过 shell；
- 在参数与媒体路径之间传递 `--`；
- 只播放数据库中已索引且仍存在的普通文件；
- 不改写播放器配置；
- 不等待播放器退出而阻塞 UI。

若加入 JSON IPC，socket/pipe 必须是当前用户私有、随机且短生命周期的；mpv IPC 没有认证，
不要监听网络地址。若加入 libmpv，应实现新后端并保留进程后端，先完成三个平台的输入、
全屏、硬件解码、HDR 和插件回归，再讨论切换默认值。

## 扫描与数据安全

- 只扫描用户显式选择的目录。
- 默认不跟随符号链接，不跨越目录根边界。
- 不删除、移动、重命名或修改媒体文件。
- 长扫描必须在阻塞线程池执行，不阻塞 WebView/Tauri 主线程。
- 数据库 schema 通过 `PRAGMA user_version` 迁移；不得为应用升级直接删除用户数据库。
- 路径是展示数据时必须按纯文本处理，不拼成 HTML。
- 文件扩展名集合集中维护在 `domain.rs`，不要在 UI 重复一份。

## UI 约定

- 保持系统标题栏、系统字体和浅色/深色主题。
- 使用 `src/styles.css` 中的 design tokens；不要在组件散落相近颜色常量。
- 优先复用小型本地组件，不引入大型 UI 套件或远程 CDN。
- 新交互必须可键盘聚焦，纯图标按钮必须有 `aria-label`/`title`。
- 动画尊重 `prefers-reduced-motion`。
- 正式合入前检查 900×600 最小窗口和长中文/长路径溢出。

## 目录和构建产物

- 不提交 `node_modules/`、`dist/`、`target/`、数据库、日志或签名材料。
- CI/外部集成建议把输出重定向到仓库下的 `build/<platform>/`。
- 应用数据使用平台应用数据目录，不写进 `.app` 或只读安装目录。
- 其他发行版若需要便携数据库，应通过未来的明确 `AppPaths` 配置实现，不要猜测当前目录
  是否可写。

## 依赖、许可证与供应链

- npm 使用 `npm ci`，Cargo 使用已提交的锁文件。
- 不引入运行时网络依赖、遥测或自动下载代码。
- 新依赖前先判断标准库/现有依赖能否完成；记录用途和许可证。
- 正式发行前生成 Rust/npm 第三方许可证清单与 SPDX/CycloneDX SBOM。
- GPL/LGPL/MPL 等依赖需要单独评估分发义务，不因项目自有代码采用 MIT 而忽略。

## 验证命令

每次修改至少运行：

```sh
npm run check
npm run build
npm run format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

修改窗口、路径、播放器或打包逻辑时，还要在 Windows x64、macOS arm64 和 macOS x64
执行 `npm run tauri build -- --no-bundle`。正式包必须额外验证架构、启动、目录选择、扫描、
播放以及播放器退出后的首页行为。

## Git 约定

- 分支使用英文结构化前缀，如 `feat/library-index`、`fix/player-discovery`。
- 提交遵循 Conventional Commits；type/scope 使用英文，主题和正文使用中文。
- 一个提交保持单一目的，不提交生成物、私人媒体路径或无关格式化。
- 架构边界、数据 schema 或平台支持发生变化时，新增或更新 ADR。
