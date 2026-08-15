# Windows 1.0.4 候选包测试

## 获取候选包

候选包由 Pull Request 的 `CI` 工作流在 `windows-2025` 原生 runner 上生成，不使用正式 Release，
也不需要提前创建 `v1.0.4` 标签。

1. 打开 Pull Request 的 **Checks**，进入最新一次 `CI` workflow run。
2. 等待 `Native (Windows x64)` 成功。
3. 在 run 页面底部下载
   `mpv-enjoy-home-1.0.4-windows-x64-candidate` artifact。artifact 保留 14 天。
4. 解压外层 artifact。它应包含：
   - `mpv-enjoy-home-1.0.4-windows-x64-setup.exe`
   - `mpv-enjoy-home-1.0.4-windows-x64.msi`
   - `mpv-enjoy-home-1.0.4-windows-x64.zip`
   - `SHA256SUMS.txt`

不要把候选包当作正式发行包转发。它没有经过 Release 工作流的 SBOM、第三方许可证清单和发布
门禁，也可能触发 Windows SmartScreen 的未知发布者提示。

## 校验下载内容

在解压后的 artifact 目录打开 PowerShell，执行：

```powershell
$expected = @{}
Get-Content -LiteralPath .\SHA256SUMS.txt | ForEach-Object {
  $hash, $name = $_ -split '\s+', 2
  $expected[$name.Trim()] = $hash.ToLowerInvariant()
}

foreach ($name in $expected.Keys) {
  $actual = (Get-FileHash -LiteralPath $name -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $expected[$name]) {
    throw "SHA-256 mismatch: $name"
  }
}

"SHA-256 OK: $($expected.Count) files"
```

还应确认内层免安装 ZIP 只包含一个顶层目录，并带有隐藏标记：

```powershell
Expand-Archive -LiteralPath .\mpv-enjoy-home-1.0.4-windows-x64.zip -DestinationPath .\portable-test
Get-ChildItem -Force .\portable-test\mpv-enjoy-home-1.0.4-windows-x64
```

输出中必须出现 `.mpv-enjoy-home-portable`、`mpv-enjoy-home.exe` 和 `LICENSE`。

## 测试矩阵

三种包不要同时安装在同一个 Windows 用户环境中。优先使用 Windows 10/11 x64 虚拟机快照，
每条路径从干净快照或独立测试账户开始。

### 1. 免安装 ZIP：全新启动

1. 从内层 ZIP 解压到新的空目录，不要覆盖正在运行的 v1.0.3。
2. 启动 `mpv-enjoy-home.exe`，确认设置页显示版本 `1.0.4` 和 Windows 免安装类型。
3. 检查更新。由于公开最新版本仍是 v1.0.3，结果应为“已是最新版本”，不能把旧版本当成更新。
4. 添加一个只含测试媒体的临时目录，完成扫描、排序、播放和退出播放器后的首页恢复。
5. 关闭并重新打开 Home，确认媒体源、主题和播放器设置仍在。
6. 在 1200×750 和 900×600 下各检查一次完整侧栏与图标侧栏，确认文字和路径不溢出。

### 2. 免安装 ZIP：从 v1.0.3 手动迁移

v1.0.3 的免安装包没有便携标记，不能用其旧更新逻辑覆盖自身。此路径只测试手动迁移：

1. 备份 v1.0.3 目录和应用数据，并退出 Home 与 mpv。
2. 在 v1.0.3 目录中创建无害哨兵文件 `keep.old` 和 `keep.pending_delete`。
3. 把 v1.0.4 内层 ZIP 解压到新目录；不要从 v1.0.3 点击自动更新，也不要直接覆盖原目录。
4. 启动 v1.0.4，确认已有媒体库和设置仍可读取，扫描与播放正常。
5. 重新启动 v1.0.4 后，确认 v1.0.3 目录中的两个哨兵文件仍存在。应用不得按宽泛后缀清理
   可执行目录中的文件。

### 3. NSIS：覆盖安装

1. 在干净环境安装公开的 v1.0.3 NSIS 包，并启动一次。
2. 退出 Home 与 mpv，运行 `mpv-enjoy-home-1.0.4-windows-x64-setup.exe` 覆盖安装。
3. 确认安装目录存在 `uninstall.exe`，设置页显示 `1.0.4` 和 Windows 安装版。
4. 确认原有设置与媒体库仍在，并完成扫描、播放、退出播放器和再次启动测试。
5. 检查更新时应报告已是最新版本；不应下载 v1.0.3。
6. 通过系统卸载入口卸载，确认卸载程序可以正常完成。测试前的应用数据备份保留到验收结束。

### 4. MSI：全新安装与手动更新提示

1. 从干净环境安装 `mpv-enjoy-home-1.0.4-windows-x64.msi`。
2. 确认应用可启动且版本为 `1.0.4`。
3. 检查更新器展示的安装类型和操作：MSI 当前没有可靠的自动升级标识，应只允许打开 Release
   页面手动处理，不能提供自动下载安装按钮。
4. 完成最小媒体扫描、播放和卸载测试。

## 通过条件与记录

正式发布前，至少记录以下信息：

- Windows 版本和是否为虚拟机；
- CI run 链接、候选提交完整 SHA，以及三个包的 SHA-256 校验结果；
- ZIP 全新启动、v1.0.3 手动迁移、NSIS 覆盖安装和 MSI 全新安装的逐项结果；
- 版本、安装类型、检查更新结果和异常界面的截图；
- 扫描、播放、退出播放器、重启和卸载是否通过。

候选构建为 v1.0.4，而公开 Release 仍是 v1.0.3，所以它能验证“不会降级”、包体识别、手动迁移
和安装覆盖，但不能在不发布 v1.0.4 的前提下完成真实的 v1.0.3 → v1.0.4 在线下载。正式发布后
还需使用保留的 v1.0.3 环境验证一次 GitHub Release 附件选择、大小/SHA-256 校验和 NSIS 打开；
旧免安装版仍按上述手动迁移方式处理。
