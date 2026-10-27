# SuperLink 应用图标修复

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

使用用户提供的黑底白色 SL 与齿轮图片，原图保存在 `internal/branding/assets/source.png`，SHA256 为 `59b0da419fca094c5a002d6177658465781b920d490498efde35849151bf5625`。仅进行尺寸缩放和 PNG / ICNS 编码，未重绘或改变颜色。

旧图标的原因：更名时没有设置 Fyne 应用图标；手工 macOS 打包缺少图标文件和 `CFBundleIconFile` 声明。

## 修复

- Fyne 启动使用内嵌 256×256 PNG，常规打包配置同步引用它。
- macOS 包含 `SuperLink.icns`，Info.plist 声明该文件；构建脚本以后打包会继续携带它。
- macOS GLFW 不设置 Dock 图标，原生窗口创建后在主线程设置同一图片，兼容独立二进制和 `go run`。
- `go run ./tools/app-icon` 可从原图重建 16 / 32 / 64 / 128 / 256 / 512 / 1024px 表示，无需 Python 图像依赖或网络服务。

## 自检

- macOS `iconutil` 成功解码 ICNS；实际查看 256px 图像，图案与原图一致。
- 隔离应用成功启动；电脑控制打开 Finder 简介，顶部和大图预览均显示 SL / 齿轮。截图保存在忽略的 `.cache/app-icon/finder-icon.png`。自检应用与新开的 Finder 窗口已关闭。
- `go test ./cmd/superlink ./internal/branding ./internal/ui`、相关 `go vet`、分层 / 文件体积检查、`git diff --check` 和 macOS arm64 构建通过。
- 正式二进制、应用包与发布 ZIP 已更新；检查包内图标、图标声明、可执行权限、CRC、摘要，并保留全部 22 条可选驱动资产记录。LaunchServices 重新注册当前包。
- 没有重启日常应用、没有修改用户连接或笔记。运行中的旧进程需正常退出并重新打开，才会加载新的运行时图标。未重启 Dock 或清空全系统缓存。

目前原生显示验证为 macOS；Windows EXE 文件资源图标尚未实测。

## Dock 尺寸与圆角调整

用户随后要求与相邻 Dock 图标一样稍小、四角圆润。使用内置 imagegen，以保留的原图为编辑参考，生成 `internal/branding/assets/superlink-rounded.png`；未覆盖 `source.png`。图案仍为黑底白色 SL / 齿轮。

运行时 256px 图标的可见范围为 `(24,26)～(232,231)`，黑色底板约占画布 81.2%；四角和边缘为透明 alpha。全部 ICNS 表示重新编码，macOS iconutil 可读取。正式二进制 / 包 / ZIP 更新并校验，保留 22 个可选驱动记录。对照图保存在忽略的 `.cache/icon-rounding/comparison.png`。未结束日常会话，运行时显示需重新打开应用。

生成方式：内置 imagegen 图片编辑，透明背景。最终提示词：

> Edit the provided SuperLink app icon for a macOS Dock. Preserve the exact white SL intertwined monogram and gear silhouette from the input, with no redraw, no invented lettering, no additions, and keep the black background tile. Only adjust the app-icon footprint: on a square 1024x1024 transparent canvas, center a black rounded-square tile that occupies 82% of the canvas width and height (about 840x840, inset 92 on all sides). Round all four corners smoothly, corner radius about 190px on the tile. The white SL and gear artwork should retain its exact proportions and center position relative to the source tile, scaled together with the black tile. No glow, no text, no border, no outside shadow. The region outside the rounded square including the four corner cutouts must be genuinely transparent alpha, not painted white or a checkerboard. Produce a clean app-icon asset, matching neighboring macOS Dock icons in visual size. Save the edited PNG asset into ./internal/branding/assets/superlink-rounded.png if file output is supported.

实际输出为 1254×1254，再按原有工具生成各尺寸资源；提示词中的 1024px 不是最终源图分辨率。检查保留了标志构成和配色，不宣称生成版本与原图逐像素相同。

原生 Finder 简介的顶部图标和大图预览已通过电脑控制确认圆角显示，证据位于 `.cache/icon-rounding/finder.png`。全部 7 种 ICNS 分辨率的透明四角已检查；自检 Finder 窗口已关闭。
