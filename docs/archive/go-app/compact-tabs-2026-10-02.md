# 单行标签修改与自检

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

日期：2026-10-02。独立项目 superlink，SuperLink 源仓库未修改。

标签从 166×46 缩为 132×32（逻辑尺寸），取消第二行，只保留名称与关闭按钮。悬停时显示完整名称、连接名称和数据库。查询更换连接 / 库后提示同步更新。表设计与导入标签也带所属连接 / 库；无上下文的欢迎页只显示名称。

提示在工作区被动层中显示，避免 Fyne PopUp 拦截画布点击。所有标签共用一份提示，无悬停计时器 / 新后台线程；移开、点击、切换、关闭或销毁标签时隐藏并清除引用。保留稳定的标签控件、横向滚动和选中顶线。

## 验证

以下检查通过：

```sh
go test ./internal/ui -count=1
go test -race ./internal/ui -run 'Test(CompactDocument|DocumentTooltip|DocumentStrip|RemovedDocuments|CustomTabs)' -count=1
go vet ./internal/ui
python3 tools/check-architecture.py
go run ./tools/check-go-size
git diff --check
go build -trimpath -ldflags '-s -w -X main.version=0.1.0' -o .cache/remember-password-check/superlink-compact-tabs ./cmd/superlink
```

鼠标事件测试覆盖提示出现 / 消失、首次点击选中和关闭、关闭后不再显示、库更新、长名称保留与边界、深色主题、弹窗遮挡和 renderer 销毁。最后新增的关闭按钮用例曾因测试夹具遗漏表页面的 TabItem 引用而失败，补齐夹具后全量 UI 和相关 race 用例复跑通过。

通过原生“文件 → 退出”正常结束旧进程，保存查询草稿后启动新版默认工作区。PID 36306、健康检查成功；实际 2498×1724 窗口显示三个单行标签，两份查询草稿恢复，当前 SQL 没有自动执行。未执行用户 SQL、读出真实密码或修改业务库。

原生单行布局已截图复核。继续检查悬停时，电脑控制工具返回 ScreenCaptureKit -3812 截图错误，重新绑定仍失败；因此悬停内容和点击行为以 Fyne 鼠标事件测试为证据，尚未完成原生悬停截图验收。

## 本机构建

- `bin/superlink` 与应用包内主程序一致，SHA-256：`b112f6a28c02360413eddc6f8870aefb63abfa199e018e56ba8e0a3d6a5d3ee4`。
- ZIP：`dist/superlink_0.1.0_darwin_arm64.zip`，87,074,107 字节，SHA-256：`92dfbb9239840d35745cfc312c76f43023d63a5d6202163d3ecbeb9414b2db92`。
- 应用更新采用原子文件替换；ZIP CRC、路径和包内主程序字节检查通过，23 条资源清单保留，22 份驱动资产的大小与哈希复核不变。

未运行浏览器测试，未提交、推送或发布 Release。本轮只实现用户要求的标签调整，不代表完整 UI 一比一复刻已完成。
