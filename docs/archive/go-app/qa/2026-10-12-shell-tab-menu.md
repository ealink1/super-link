# Shell 标签批量关闭菜单

- 点击或右键点击 Shell 标签，先选中该会话，再打开菜单。
- 菜单包含关闭左边标签、关闭右边标签、关闭其他标签、关闭所有标签；没有目标时置灰。
- 关闭按钮继续只关闭所在标签。
- 批量关闭通过现有 `CloseIntercept` 停止会话、取消上下文并释放终端资源；关闭目标使用独立切片，避免删除时遗漏标签。
- 保留标签的顺序及当前会话，恢复终端输入焦点；关闭所有后显示欢迎页。
- 已移除标签解绑菜单回调，过期菜单不会关闭其他会话。

验证：

- `go test -race ./internal/ui -run 'Test(ShellTab|ShellStrip|ClosedShellTab|ShellClosingDuringStartup|ShellLocal|RemovedDocuments|DocumentStrip|CompactDocumentTabs|DocumentTooltip)' -count=1 -timeout=180s` 通过。
- 测试通过 Fyne 画布点击标签和菜单项，覆盖四种范围、顺序、焦点、欢迎页、工作线程退出、左右边界、单标签、过期菜单、右键和独立关闭按钮。
- `go vet ./internal/ui` 和 `python3 tools/check-architecture.py` 通过。
- 本次菜单相关源文件的空白检查通过；新增生产文件 64 行。
- 全项目长度检查报告其他修改中的 `internal/ui/data_grid.go` 的 `bind` 函数 123 行，超过 120 行限制。
- 原生 darwin/arm64 应用及 SQLite 驱动打包、签名校验通过。项目和 `/Applications/SuperLink.app` 安装包已更新，二进制摘要一致；0.1.17 新进程启动健康检查通过。
- 没有使用浏览器测试。原生应用已打开 Shell 页面；菜单交互由 Fyne 测试驱动验证。
