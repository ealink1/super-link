# SuperLink 第二轮内存精简

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

日期：2026-10-02。环境：macOS arm64、Go 1.26.3、Fyne 2.8.1、Retina。
本页记录第一轮约 279.3 MB 截图之后的进一步优化；第一轮的历史数据见
[内存分析](memory-analysis-2026-10-02.md)。
本页产物哈希和 QA 运行状态为“记住密码”改动之前的内存验收快照，
当前最新构建见[标签自检](compact-tabs-2026-10-02.md)，密码行为见[密码记忆报告](remember-password-2026-10-02.md)。

## 本轮结果

相同配置的原生欢迎页，40 秒时的进程 physical footprint 从 **191.00 →
180.11 MiB**；两个查询草稿页从 **236.94 → 214.16 MiB**，分别减少约
5.7% 和 9.6%。诊断 GC 后的存活 Go 堆分别减少约 22.9% 和 20.7%。
这些对照使用隔离工作区、生产 `ui.New`、1320 × 860 的逻辑窗口配置和真实
Fyne/OpenGL；查询场景包含本地 SQLite fixture 与两份恢复的草稿，未自动执行 SQL。

三份字体的完整中文覆盖、全部字形、字宽和压缩前的绘制轮廓保留。没有降低
Retina 分辨率，没有在产品里加入周期强制 GC，也没有改动数据库执行和写入策略。

| 固定场景，40 秒 | 第一轮版本进程占用 | 本轮进程占用 | 第一轮存活 Go 堆 | 本轮存活 Go 堆 |
| --- | ---: | ---: | ---: | ---: |
| 欢迎页 | 191.00 | 180.11 | 42.89 | 33.06 |
| 两个查询草稿页 | 236.94 | 214.16 | 59.55 | 47.23 |

表内单位均为 MiB（1,048,576 字节）。进程数值取诊断 GC **之前**；存活 Go 堆
取诊断程序结束前单次 GC 之后。结束前的 GC 仅用于分析，没有进入应用代码。
查询场景 GC 前 Go 堆为 77.72 → 70.27 MiB，不能将回收后的 47.23 MiB 当作
整个应用的常驻内存。

中间版本测得欢迎页约 178–180 MiB、两查询页约 207–227 MiB；最终完整构建
采用上表最后一组数据。图形缓冲、系统压缩和回收时机有波动，没有将中间的
最低值作为最终结论。原始采样、字体对照和 heap profiles 保存在私有的
`.cache/memory-qa/round2-*`，不加入 Git。

## 改了什么

1. **备用字体按需解析。** 原来即使 NaviUI/NaviMono 已覆盖全部显示字符，
   Fyne 仍会解析默认文字、Emoji 和符号备用字体。本轮延后到实际缺字时加载，
   保持原来的备用字体顺序以及后续语言、逐字系统回退。实际字符覆盖不变。
2. **压缩重复字形指令。** 使用 Adobe 的
   [cffsubr](https://github.com/adobe-type-tools/cffsubr) 将 CFF 重复指令抽成子程序。
   三份字体总大小从 40,124,600 → 33,120,060 字节，减少约 17.5%；不做字符裁剪。
   这项对运行内存的额外收益较小，主要收益仍来自未用字体和对象释放。
3. **切断主题到整页的引用。** Fyne 缓存中的 `featureTheme` 原先引用完整
   `ThemeOverride`，继而保留页面；应用的结果按钮主题和编辑器主题也引用整个
   标签模型或编辑器。现在主题只共享必要的显示状态，保留选中颜色、换行与设备特性。
4. **清除退休的主题范围。** 局部主题渲染器销毁时，删除它所属的旧子对象范围，
   包括刷新时已经从内容树移除的 canvas 对象。独立嵌套主题保留；隐藏页重建
   渲染器时可以重新应用主题。回收仍遵循 Fyne 的缓存生命周期，关闭不等于即时清零。
5. **复用文档标签控件。** 切换、重命名和刷新文档时保持标签、关闭按钮和滚动条
   身份；删除标签时解除旧控件的回调，避免缓存的关闭/选择按钮继续引用整页。
   移除文档使用 `slices.Delete` 清空底层切片尾部，避免已关闭页面仍被容量区引用。

实现位于 `internal/ui/document_strip.go`、`document_tab.go`、`documents.go`、
`code_editor.go` 和 `result_tabs.go`。Fyne 修改集中于四个生产文件、三个回归测试
文件；固定副本共 2,149 个文件。来源、哈希与完整补丁见 `docs/fyne-source.json`、
`docs/fyne-memory.patch`，`tools/verify-fyne.py` 验证实际构建所用的模块替换。

## 字体和回收验证

- Python FontTools 对每个字体的 31,036 个字形逐一比较绘制指令展开后的轮廓，
  全部一致；全部 cmap、字形顺序和水平度量一致。
- 应用实际使用的 go-text 引擎对三个字体的全部 **93,108 个字形**逐一比较
  `GlyphData` 与水平字宽，压缩前后完全一致。固定资源仍包含全部 **92,670 个
  字符映射**；常规 CI 验证字体摘要、逐字符覆盖和参考字宽。
- 使用固定的 FontTools 4.63.0 / cffsubr 0.4.0，从许可源字体重新生成三份资源，
  与本轮部署的字体字节完全一致。构建脚本会先验证全部轮廓再保存；依赖仅用于
  字体再生成，应用和普通 `go build` 不需要 Python 字体工具。
- Go `weak.Pointer` 回归确认：保留缓存主题时，已经无其他引用的局部主题容器、
  编辑器与结果标签模型仍能回收。
- 回归覆盖旧子范围清理、嵌套主题保护、页面重新展示、设备状态变化、标签刷新
  100 次身份保持、重排/选择/重命名、移除后回调解除，以及编辑器换行显示层切换。

## 实际应用与产物

通过菜单正常退出原 QA，再备份工作区 SQLite 和应用包；更新可执行文件、辅助程序、
SQLite Agent 与来源说明。原有两个连接和三份草稿记录保留。新 QA PID 为 29599，
健康握手确认版本 0.1.0；实际文件与本轮构建 SHA256 一致。

电脑控制确认恢复的查询草稿、中文显示、语法高亮、设置和深色主题；本地演示 SQLite
执行 `SELECT 1 AS id, 'Fyne memory check' AS name, NULL AS optional_value;`，实际显示
一行三列，分别为整数、文本、NULL。该时刻主进程 footprint 约 **234.8 MiB**，峰值
248.1 MiB。未执行用户业务库 SQL 或数据写入。

重启带来一个需要用户处理的影响：原 MySQL 连接使用 **仅本次会话保存** 的密码，
进程退出后该密码清除，因此需要在连接编辑页重新输入。连接配置和原 SQL 草稿保留；
没有把会话密码自动改为持久保存。重启后 MySQL 凭据不可用、显示页与运行时长都不同，
因此不把用户截图的 279.3 MB 或更新前的 292.1 MiB 与新进程直接计算优化百分比。

| 产物 | 本轮结果 |
| --- | --- |
| 应用 / QA 主程序 SHA256 | `b4db2d6508b1c87da00eeae7760051636c3dde8c6b7a97b640c9c2f94547fce9` |
| 原生应用 | `bin/SuperLink.app`，macOS arm64，v0.1.0 |
| 全部驱动 | 22 个独立 Agent；无 JVM 管理连接器 |
| ZIP | `dist/superlink_0.1.0_darwin_arm64.zip`，83.10 MiB |
| ZIP SHA256 | `39f3e711dfdc5aaa136ba16a68be5fc977d8d4938b0e8f39eff8125aa287a2e5` |
| 清单 | `dist/assets-darwin-arm64.json`，23 项，摘要、大小、名称无重复通过 |

## 自检与边界

完整应用单测、全部驱动标签测试、UI race、`go vet`、架构与源文件体积检查、
来源 / UI 资产 / Fyne 补丁校验均通过。Fyne painter/cache 全套 race 与受影响的
ThemeOverride container 回归通过，后者连续五次运行通过并加入 CI/selfcheck。
原生启动、事件循环健康、离线 SQLite 安装、工作区独占与正常退出烟测通过；
ZIP CRC、路径安全和包内外文件字节一致性通过。本轮没有调用浏览器测试，也未执行 commit 或 push。

之前记录的上游 DocTabs 全套快照失败仍属于未解决的原环境限制，本轮没有将其
宣称为通过。短时固定场景和引用回收回归不能证明长时间、更多数据库或大结果集
的内存上限。Retina 的 IOSurface、GPU 及窗口图形缓冲会计入主进程 footprint；
完整窗口保留清晰度时，不能承诺整个应用低于 100 MB。

## 复现本轮检查

使用不存在的输出目录，避免误打开已有工作区：

```sh
go build -o .cache/memory-qa/memory-probe ./tools/memory-probe
.cache/memory-qa/memory-probe --output .cache/memory-qa/new-round2-welcome --duration 40s
.cache/memory-qa/memory-probe --fixture --output .cache/memory-qa/new-round2-queries --duration 40s
python3 tools/verify-fyne.py
go -C third_party/fyne test -race -tags=test ./internal/painter ./internal/cache
go -C third_party/fyne test -race -tags=test ./container \
  -run 'Test(CachedChildTheme|CachedFeatureState|DestroyedOverride|ThemeOverride)'
```

字体重新生成使用 `tools/font-requirements.txt` 的固定依赖和
`tools/build_fonts.py --fyne-fonts third_party/fyne/theme/font --output <新目录>`。
逐字 Go 引擎对照可设置 `SUPERLINK_FONT_REFERENCE_DIR` 与
`SUPERLINK_FONT_CANDIDATE_DIR`，运行
`TestCompressedFontsMatchEveryReferenceOutlineAndAdvance`；参考目录未提供时，
常规 CI 会明确跳过这项需要旧字体的额外审核，仍执行全部已部署字体的固定摘要和覆盖测试。
