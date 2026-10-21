# 统一弹窗卡片风格

## 背景

更新流程存在两套设计语言：`发现新版本` 与下载进度是自绘白色圆角卡片，而 `安装并重启` 确认、更新失败报告仍使用 Fyne 原生对话框，视觉断层明显。全应用同类不一致共 36 处：

- 20 处 `dialog.NewCustom`
- 7 处 `dialog.NewCustomConfirm`
- 8 处 `dialog.NewForm` / `ShowForm` / `ShowCustom`
- 1 处 `NewCustomWithoutButtons`（关于）
- 8 处自绘 `widget.NewModalPopUp`（连接编辑、SSH 指纹、连接分组、分组表单、笔记分组、Shell 表单、数据源选择、Shell 连接）

另外 Fyne 的 zh_Hans 翻译把 `Yes`/`No`/`OK` 直译为「是」「不」「好」，正是确认框里「× 不 / ✓ 是」的来源。

## 设计

新增 `internal/ui/card_dialog.go` 作为唯一弹窗外壳：

- 白底、圆角 28、水平内边距 26、垂直 22，默认宽 440，表单宽 560
- 结构：标题 24pt 加粗居中 → 副标题 13pt 灰 → 可选附加区（版本胶囊）→ 信息块（图标 + 小标题 + 值）→ 正文 → 右对齐按钮 → 11pt 页脚
- 布局使用 Border：正文独占剩余高度并包裹滚动宿主，长表单可滚动
- 按钮主操作使用 `HighImportance`，不再使用带图标与否作为主次区分

`cardDialog` 保留原 `dialog` 的调用形状：`Show` / `Hide` / `Confirm` / `SetOnClosed` / `Resize`，`Hide` 视为取消并回调 `false`，`Confirm` 回调 `true`。新增 `SetActionLabel` / `SetActionDisabled` 供下载进度切换按钮文案。

调用层提供四个镜像函数：`newCardCustom`、`newCardCustomConfirm`、`newCardForm`、`showCardForm`、`newCardCustomNoButtons`，原有调用点仅替换函数名，`message_dialogs.go` 的三层包装（信息、错误、确认）自动覆盖 45 处调用。

原有 8 处自绘弹窗改为共用 `cardRadius` 圆角，无面板的（SSH 指纹、连接编辑、数据源选择）套 `cardFrame`。

## 文案

确认框按钮由 Fyne 的「是 / 不」改为「确定 / 取消」，信息框「好」改为「确定」，并同步更新两处依赖旧文案的测试。

## 验证

`go test ./internal/... ./cmd/...`、`go vet ./...`、`tools/check-architecture.py`、`go run ./tools/check-go-size` 全部通过；尺寸探针确认信息/确认/更新/表单卡片分别为 440×189、440×170、440×376、560×135；离屏渲染核对信息、确认、更新、表单与关于五类弹窗的排版。未使用浏览器。原生构建后重启应用。
