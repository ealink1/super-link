# SQL / Shell 共用日夜主题自检

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

日期：2026-10-02。项目：独立的 `ealink1/super-link`。

后续应用已更名为 SuperLink，当前应用包名称及产物摘要见[更名自检](superlink-rename-2026-10-02.md)。本报告的主题验证和更名前摘要保留为历史证据。

## 已完成行为

- macOS 窗口控制按钮右侧、SQL / Shell 左侧新增太阳 / 月亮按钮，点击切换整个应用；悬停和无障碍名称说明将切换到哪种模式。
- SQL / Shell 共用现有应用主题的背景、面板、文字、边框、选中和绿色主色，各自保留布局和字号。Shell 不再固定使用紫色深色主题。
- 标题栏、SQL 外观设置与 Shell 外观入口改变同一状态。当前工作区 SQLite 保存 `ui.appearance`，正常重启恢复选择。
- 快速切换的设置写入串行处理，较早的写入不能覆盖最终选择；异步启动加载不会覆盖用户已经做出的选择，退出再保存最终状态。
- 主题切换只刷新显示，保留查询草稿、工作标签、弹窗输入、PTY 会话和终端缓冲。默认终端前景 / 背景同步改变，显式 ANSI 颜色保留。
- 局部主题只引用小型原子显示状态，不持有窗口或页面模型。文字和矩形在刷新时解析为具体颜色，避免旧纹理缓存继续显示原色。
- 原生检查发现默认 Fyne SVG 着色会填充轮廓并漏掉线条，已改为只解析 `currentColor`，保留 Lucide 原始几何，并让纹理名称随颜色改变。

设计见[共用主题方案](plans/2026-10-02-shared-appearance-design.md)。此前[Shell 界面记录](shell-ui-ishellpro-2026-10-02.md)的独立紫色设计和应用摘要已被本次替换。

## 原生电脑控制验证

使用单独的 `Navi Theme QA.app` 与 `.cache/shared-appearance/workspace/`。七台主机均为既有 `127.0.0.1` 演示条目，工作区没有数据库连接；没有执行远程连接或业务 SQL。

| 路径 | 实际结果 |
| --- | --- |
| SQL 日间 → 夜间 | 窗口背景、标题栏、主色和正文同步改变；原生按钮名称和太阳 / 月亮图标切换 |
| 切至 Shell | 懒加载工作区继承当前模式；主机卡片、导航和搜索框颜色与 SQL 共用调色板 |
| Shell 夜间 → 日间 | 卡片、彩色标签、文字、边框、图标正确刷新；轮廓未被填成实心 |
| 打开新建主机表单 | 日间与夜间弹窗均可读；输入 `Theme QA` 后切换主题，文字保留；取消未保存额外主机 |
| 本地终端 | 真正启动 zsh PTY，执行 `printf` 输出绿色 ANSI 文本与“你好”；主题往返后输出和会话保留，默认文字 / 背景改变 |
| 正常退出并重启 | 夜间选择落库；重新打开后 SQL 仍为夜间，Shell 继承该模式 |
| 最新应用包启动 | 正常退出隔离检查应用后，打开 `bin/SuperLink.app`；标题栏开关可见，日常连接和查询草稿恢复，未触发查询执行 |

截图留在忽略缓存 `.cache/shared-appearance/native/`：`sql-light.png`、`sql-night.png`、`hosts-light.png`、`hosts-night.png`、`form-light.png`、`form-night.png`、`terminal-light.png`、`terminal-night.png`、`restart-night.png`。本地终端含本机壳启动上下文，截图不加入公开仓库。

## 自动化验证

```sh
go test ./...
go test -race ./internal/ui ./internal/application ./internal/infra/...
go vet ./...
go run ./tools/check-go-size
python3 tools/check-architecture.py
python3 tools/verify-shell-assets.py
python3 tools/verify-ui-assets.py
python3 tools/verify-fyne.py
git diff --check
```

以上通过。新增回归覆盖快速连续切换后最终值、启动期间选择、重载、最终退出保存、设置入口同步、SQL 草稿 / 标签和弹窗输入保留、终端默认色与 ANSI 显式颜色、动态文本 / 矩形刷新，以及轮廓图标内部透明、线条显示和随主题更新。

原生 macOS arm64 构建通过，仅有既存的重复 `-lobjc` 链接提示。未使用浏览器测试。Windows / Linux 原生窗口、真实远程 SSH / SFTP / 监控页、中文 IME 预编辑及全部悬停 / 禁用状态仍需各自验证；这次主题自检不构成完整 SuperLink / iShell Pro 功能或像素一比一验收。

## 最新本机产物

主程序以 `go build -trimpath -ldflags '-s -w -X main.version=0.1.0'` 编译。更新既有应用包与 ZIP，保留 22 个已构建 Agent；逐项长度 / SHA256、ZIP CRC、包内主程序和离线 SQLite 哈希检查通过。

| 产物 | 校验 |
| --- | --- |
| 原生应用 | `bin/SuperLink.app`，v0.1.0 |
| 主程序 SHA256 | `54f8e08e621f9e64da9fbd43bac0c5d0894f0958e21242838173e2675e81fc7b` |
| ZIP | `dist/superlink_0.1.0_darwin_arm64.zip`，87,555,244 字节 |
| ZIP SHA256 | `1d5f967071e303e85abc04e45317d68871d0b920f34e3253d36943feb86d4ee0` |
| 分发清单 | `dist/assets-darwin-arm64.json`，23 项，22 个 Agent 加应用 |

未提交、推送、上传 Release 或进行正式签名 / 公证。

## 后续菜单精简

按用户截图要求删除 SQL 顶部的 `SuperLink` 标识及前置 80 像素留白，其余菜单从左侧开始排列。本次为布局调整，现有 `go test ./internal/ui` 和原生构建通过。正常退出保存草稿、更新应用与 ZIP 后，使用电脑控制重新打开最新版；实际窗口确认标识已消失、菜单左移且查询草稿恢复。上表摘要已经更新为此版本，23 项分发产物的长度 / SHA256 与 ZIP CRC 复核通过。
