# macOS 主窗口后台运行验证（2026-10-13）

## 行为

- 主窗口关闭拦截隐藏现有窗口，不调用 shutdown，不取消任务或释放 SSH/数据库会话。Dock 重开显示并聚焦同一窗口。
- 文件菜单退出、macOS 应用退出和 ⌘Q 复用原有保存及释放流程。隐藏时退出先显示窗口，保证未保存修改确认可见。
- 原生 AppKit delegate 包装器转发其他事件给 GLFW 原 delegate。真正关闭时恢复原 delegate 并释放 cgo handle，无 vendored 框架修改。其他平台关闭退出行为保持原样。
- shutdown 的异步完成回调使用现有 UI dispatcher，生产环境仍是 Fyne UI 队列；测试使用串行队列验证完整退出。

## 验证

- `TestWindowHideRestorePreservesSessionsAndBackgroundWork` 检查隐藏期间后台任务完成、终端上下文与页面保留、恢复时窗口内容及选中标签身份稳定、退出中禁止重新隐藏/恢复。
- `TestQuitFromHiddenWindowSavesNotesAndReleasesSessions` 使用生产退出菜单动作，检查最终笔记编辑保存、终端取消与服务释放、重复退出不重复清理。
- `TestQuitFromHiddenWindowKeepsUnsavedConfirmationVisible` 检查隐藏状态退出时未提交修改确认可见，尚未确认不开始 shutdown。
- macOS 原生探针使用生产 Window、AppKit delegate 与隔离加密工作区。通过真实 NSWindow `performClose:` 触发关闭；LaunchServices 对同一应用 bundle 的重开事件（Dock 重开路径）恢复窗口，检查原终端/笔记仍保留。
- 两次原生运行分别使用 NSMenu 的真实 ⌘Q key-equivalent 路径及原生「退出」菜单动作。均从隐藏状态真正退出，重开本地存储后确认最后笔记已保存。探针诊断入口仅存在于 Go overlay，正式应用不包含该入口；产物在忽略目录 `.cache/background-lifecycle/`。
- 全量 `go test ./...`、相关生命周期/笔记/工作区竞态测试、`go vet ./...`、文件尺寸、架构、Fyne/上游/资产来源检查通过。
- Windows/Linux amd64 的 UI 包使用 `CGO_ENABLED=0`、`-tags ci` 交叉编译通过；这不是其他平台原生图形验收。本次未连接真实外部 SSH/数据库服务，后台保留行为由上下文、任务完成及页面身份测试验证。
- 最新代码已原生构建、安装并重启，macOS 严格代码签名、启动健康握手及运行进程检查通过，安装可执行文件与本次构建 SHA256 一致。
- 未使用浏览器。修改已随 [SuperLink v0.1.21](https://github.com/ealink1/super-link/releases/tag/v0.1.21) 公开发布，六平台构建与签名验证见[发布记录](../releases.md)；正式 macOS arm64 包已安装并重启。
