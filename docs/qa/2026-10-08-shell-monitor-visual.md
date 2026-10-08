# Shell 监控面板视觉复查

以用户提供的完整监控侧栏截图为参考，在 420 × 829 逻辑像素下进行 Fyne 软件渲染对照，并检查浅色和深色主题。预览使用固定测试数据，不需要连接真实服务器。

本次统一顶部图标页签与珊瑚色选中背景、双行主机信息、两行指标卡片、单条 CPU 总负载曲线及渐淡填色、磁盘分区和网卡紧凑条目、底部四格目录统计。去除额外标题、原有页签下划线和采样工具栏；采样设置保留在顶部右键菜单。监控区域使用独立等宽字体，避免 macOS 默认 Courier 字体过细。

可重复预览：

```sh
SUPERLINK_UI_CAPTURE_DIR=/tmp/SuperLink-monitor-capture go test ./internal/ui -run TestMonitorReferenceCapture -count=1
```

输出 `monitor-reference-light.png` 和 `monitor-reference-dark.png`。参考图为 Retina 截图，预览按逻辑尺寸渲染；实际字体光栅化随系统和显示缩放而变化。此次未使用浏览器测试，未验证 Windows/Linux 实机渲染或真实 SSH/GPU 采样。

## 文件下载与操作菜单

文件行新增右键菜单，提供复制文件名、复制路径、用本地程序打开、下载、重命名、修改权限和删除；目录增加打开文件夹，下载和本地打开只适用于文件。菜单使用图标、灰色悬停背景与红色删除项。下载进度在列表底部以紫色进度条显示文件名、百分比和已传输大小，支持取消，并区分完成、失败和取消状态。保存目录仍由操作系统文件选择器选择。

本地打开将文件流式下载到独立临时目录，再调用系统关联程序；临时目录随 Shell 标签关闭清理。重命名拒绝覆盖已有目标，删除需要确认且不递归删除目录。当前没有实现参考图中的剪切、压缩、解压及远程编辑入口。

`TestShellFileMenuReferenceCapture` 可以在设置上述截图环境变量时输出 `files-menu-reference.png`。测试覆盖筛选后的目标映射、输入验证、传输完成/取消、SFTP 重命名覆盖保护、权限修改和删除。未进行 Windows/Linux 系统关联程序的实机测试。

## 文件列表行对齐

文件列表改为 37 逻辑像素行高，图标与各列文字按实际文字行高垂直居中。名称列增加图标后的留白，时间列预留完整日期宽度；文件夹为紫色描边，文件名为灰紫色，大小与时间使用灰色等宽字体。选中行使用连续灰色背景及左侧 1 像素紫色指示线，表头和行分隔线共享列位置。文件列表展示的大小使用两位小数与 KB/MB/GB 单位，日期去除前导零。

`TestShellFileRowPaintedContentIsVerticallyAligned` 对实际软件渲染像素中的图标和文字位置进行检查。`TestShellFileRowsReferenceCapture` 输出 `files-rows-reference.png`，用于对照完整文件行样式。

## 综合页网络接口范围

综合页排除回环接口以及已知容器内部接口名称（例如 `docker*`、`br-*`、`veth*`、`virbr*`、`cni*`）。保留主机 NIC、虚拟机 NIC、自定义接口及多网卡；完整接口列表仍在 NET 页。网络卡片、历史曲线和 NET 总吞吐量采用同一过滤规则，避免将容器内部流量重复计入主机流量。

`TestMonitorOverviewHidesContainerInterfacesWithoutLosingNETDetails` 覆盖大量 Docker 接口与一个主机网卡的情况，检查综合页显示范围、完整详情保留及流量汇总。

网卡条目的名称和上下行速率改为按实际文字行高居中；分区及网络标题采用固定图标尺寸并补偿中文字体行框留白。`TestMonitorNetworkTextPaintedAtRowCenter` 在 38/60 像素行高中检查三列文字的实际像素中心，`TestMonitorSectionIconAndTextPaintedTogether` 检查标题与图标的实际对齐。

顶部六个监控页签使用独立的图标/文字布局，保留标准 Fyne 按钮的点击、焦点及键盘行为，不再沿用通用 Shell 按钮的文字上移偏移。`TestMonitorTabIconAndLabelPaintedAtSameHeight` 检查全部六个页签的实际渲染像素（包括小字号中文的抗锯齿覆盖）与点击回调。

页签悬停刷新已改为独立渲染器，消除通用按钮渲染器刷新过程中临时恢复另一套间距的问题。图标和文字只有一套固定布局；悬停与键盘焦点仅改变背景。`TestMonitorTabHoverDoesNotMoveContent` 对全部六个页签多次移入、移出及焦点切换，检查实际渲染后的内容位置和尺寸不变。

监控面板字体进一步收小：IP/用户名 13→11、运行时间 16→13、指标百分比 19→15，其余标题、辅助信息、网卡文字和底部统计相应减小。卡片文字按字体实际行高居中，保持图标和文字间距；重新执行监控渲染、对齐与悬停检查。
