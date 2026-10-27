# Note 输入光标修复

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

Note 的标题、正文为了隐藏输入框边框，将 `SizeNameInputBorder` 设成了 0。Fyne 2.8.1 同时使用这个尺寸作为输入光标的宽度，导致编辑时看不到光标。

现在保留正数宽度，通过 `noteEntry` 的渲染器单独隐藏外框。仍然使用 Fyne 自带的光标动画、键盘导航、选择、撤销和重做，没有新增定时器或输入状态。

验证：

- 回归测试检查日间、夜间下标题与正文的实际光标几何、输入焦点、方向键移动、失焦隐藏和无边框外观。
- Note、AI、工作区相关测试与 Note 光标/编辑竞态检查通过，`go vet ./internal/ui`、源文件尺寸检查、`git diff --check` 通过。
- macOS arm64 原生构建通过。隔离数据目录的原生窗口中，标题和正文输入光标均可见，日夜模式切换后正文光标仍显示。未测试 Windows/Linux 原生窗口。
- 已更新 `bin/superlink`、`bin/SuperLink.app` 和 macOS 发布 ZIP，保留原有驱动清单。正在运行的旧进程需重开后生效。

截图：[日间正文](screenshots/note-caret/light.png)、[夜间正文](screenshots/note-caret/dark.png)。
