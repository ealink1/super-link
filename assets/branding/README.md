# SuperLink 应用图标

`source.png` 为用户在 2026-10-02 提供的黑底白色 SL 与齿轮图片；保留原图案和颜色，不使用 SuperLink 或 Fyne 品牌标识。

`superlink-rounded.png` 是根据用户后续要求，以原图为参考通过 imagegen 编辑的 Dock 版本：缩小黑色底板，增加透明留白，四角改为圆角，保留白色 SL 与齿轮标志。

运行 `go run ./tools/app-icon` 从圆角版本重新生成：

- `superlink.png`：256×256，供 Fyne 运行时与常规应用元数据使用。
- `superlink.icns`：16 / 32 / 64 / 128 / 256 / 512 / 1024 像素的 PNG 表示，供 macOS Finder / Dock 使用。

转换工具仅进行尺寸缩放与格式编码；原始 `source.png` 保留，不被圆角版本覆盖。
