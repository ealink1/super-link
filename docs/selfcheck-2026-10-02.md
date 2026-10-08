# SuperLink 功能与界面对齐自检

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

日期：2026-10-02。环境：macOS arm64、Go 1.26.3、Fyne 2.8.1。
目标目录：`.`。SuperLink 源码目录保持未改动。
**本报告记录已实施和已验证部分；不代表完整的一比一复刻完成。**
旧版 Alpha 的真实服务和原生更新测试保留在[2026-10-01 报告](selfcheck-2026-10-01.md)，
未将这些历史结果重新写成本轮最新 UI 的验收。
最新增加“记住密码”，不再调用系统钥匙串；兼容、保护边界和自检见
[密码记忆报告](remember-password-2026-10-02.md)。后续修复保存后旧提示残留，并已重启默认工作区的原生应用，验证密码掩码恢复及无残留弹窗。
随后按用户要求将工作标签缩为单行，连接 / 库改为悬停提示；最新构建、测试和原生验收范围见[标签自检](compact-tabs-2026-10-02.md)。
之后新增 SQL / Shell 整页切换及 Shell 首阶段，完整功能路线和最新产物见[Shell 自检](shell-workspace-2026-10-02.md)。原生标题栏复查被锁屏阻断，不能视作完整界面验收通过。
此后使用电脑控制取得完整 iShell Pro 原生截图并重做 Shell 界面；已实际验证标题栏切换、卡片 / 中文搜索、认证表单切换和本地终端。最终构建、竞争修复、最新包摘要及锁屏后的未验收项见[Shell 界面自检](shell-ui-ishellpro-2026-10-02.md)。下文产物表为首阶段快照，最新包以该后续记录为准。

## 本轮行为

- 主窗口、工具栏、工作标签、绿色主题及上游图标重新对齐；后续按用户要求改为紧凑单行标签。
- 惰性连接树和 SQL 表 / 视图类别、数据库 / schema 选择、独立表数据页。
- 表字段元数据、服务端分页 / 筛选 / 排序、暂存增删改、SQL 预览及受保护提交。
- 字段 / 索引设计；索引新增预览、取消新增、撤销删除；保存前检查结构变化。
- 原生高亮编辑器、行号、Cmd/Ctrl+R、日志 / 参数 / 结果圆角标签。
- 参数化执行和 typed Agent 传输；BLOB 保真、精确整数、预算与缺参数检查。
- 已存查询命名 / 打开 / 删除 / 冲突检查，SQL 文件打开和原子导出；草稿保留库 / schema。
- XLSX / CSV / JSON / Markdown / HTML / INSERT SQL 导出；文件预览、映射与分批导入。
- 全连接只读及数据 / 结构 / 脚本 / 导入独立策略；未知写入结果不重试。

## 查出的缺陷与修复

| 缺陷 | 修复与回归 |
| --- | --- |
| SQLite 固定会话占用唯一连接，元数据访问连接池可能自等待 | 通过同一 session 读取字段、索引、外键、触发器、DDL；真实 `:memory:` 5 秒超时用例 |
| binary 被展示字符串归一化，导致普通字段编辑无法准确定位原行 | 保留 binary 类型并使用独立 cell 元数据传输；原值按列类型绑定；SQLite / DuckDB 编辑含 BLOB 行并验证原 bytes |
| 有参数的 Agent 查询漏传预览预算 | request / scanner 使用与无参数相同预算；10,002 行查询只返回 100 行且标记截断 |
| 输入框失焦或虚拟行回收可能丢失编辑 | 失焦暂存，按 cell identity 保留草稿；无效输入阻止分页 / 提交；回归中文输入、NULL、不同行复用 |
| 已确认回滚仍关闭内存数据库会话 | `RolledBackError` 明确区分成功回滚与未知提交；应用保留确认回滚会话，未知状态关闭；真实约束失败整批回滚 |
| 两个命名查询编辑器可能覆盖保存内容 | 保存 / 删除比较 revision；打开独立草稿，不自动执行；迁移及连接删除级联测试 |
| 对象菜单可能跟随全局连接选择 | 动作绑定被点击节点及 literal schema / name；切换选择后仍打开正确连接、特殊表名不被拆分 |
| Fyne 测试驱动后台内联回调与测试渲染竞争 | 测试使用串行 UI 队列并等待 I/O / 回调都完成；生产默认仍 `fyne.Do`；首次 race 失败后修复，复跑通过 |
| 原生 SelectEntry 更新动作对象导致箭头失去布局 | 保留既有 Button identity，替换回调；数据库选项点击同时重载 schema |
| 切换深色后标签底色及表头前景仍是旧主题 | 软件截图对照发现；自定义 renderer 刷新颜色，明暗往返测试和重新截图 |
| PostgreSQL DDL 占位注释被当作可用元数据 | 明确返回不可用警告；不标为建表脚本或备份 |

## 本机已通过

```sh
python3 tools/check-architecture.py
go run ./tools/check-go-size
python3 tools/verify-upstream.py
python3 tools/verify-ui-assets.py
go test ./...
go test -tags superlink_full_drivers ./internal/upstream/db ./cmd/driver-agent
go test -race ./internal/application ./internal/infra/... ./internal/domain ./internal/ui ./cmd/release-sign
go vet ./...
SUPERLINK_TEST_DRIVERS="$PWD/bin/drivers" go test -tags integration -count=1 ./internal/infra/runtime -run 'TestLocalFileAgents|TestLocalAgentsBoundValuesAndOptimisticChanges|TestSQLiteAgentSessionMetadataDoesNotWaitForItsOwnPool'
```

- 架构检查：独立 Fyne，应用 / 领域层不引入 Fyne 或 Wails，无 JVM 运行依赖。
- 体积检查：新生产文件 ≤ 800 行、函数体 ≤ 120 行、测试文件 ≤ 1,500 行。
- 保留上游 523 份文件的来源和哈希通过；97 份图标（84 SVG、13 PNG）哈希 / 清单通过。
- 本轮实际构建 / metadata 握手覆盖全部 22 个可选 Agent，修订前缀 `fyne-values1-`。
- 164 个链接模块及字体 / 图标归属重新生成；Dameng 模块缺根许可证文本的既有分发审核状态保留。
- 真实本地 Agent 回归用独立临时库，不使用用户业务数据；包括文件、内存、metadata、参数、分页预算、binary 编辑及约束回滚。

最终构建命令 `python3 tools/build.py --all-drivers --package` 已通过。
生成 22 个独立 Agent 和 1 个 macOS arm64 应用 ZIP；逐个校验 manifest 的长度、
SHA256、修订及协议，ZIP CRC / 路径检查通过。包内主程序和 Helper 与 `bin/`
对应二进制一致，SQLite 离线 Agent 的 bundle 哈希一致，6 份字体等资源许可文本
以及根归属文档已入包。没有正式签名 / 公证或上传 Release。

| 产物 | 本机路径 / 校验 |
| --- | --- |
| 最新原生应用 | `bin/SuperLink.app`，v0.1.0，`io.github.ealink1.superlink` |
| ZIP | `dist/superlink_0.1.0_darwin_arm64.zip`，87,482,937 字节，包含第二轮内存修复、记住密码、单行标签及 SQL / Shell 首阶段 |
| ZIP SHA256 | `e86a20d2c96d072882319c86d2a82208d1d0eae2dcf167fbc1a9e0c4b6551c5d` |
| 主程序 SHA256 | `e3d48aacf609e38e1bf790e921a343d0bbfc351028a98fe81b5662e0026691a5` |
| 全部产物清单 | `dist/assets-darwin-arm64.json`，23 项，无重复文件名 |

颜色刷新修复后重新运行了完整 UI 单测、UI race 和 `go vet ./...`，均通过；
随后实施并重新构建了下述内存修复。最新构建后未再修改应用生产代码。

## 第一轮内存修复复查（历史记录）

- 共享字体解析、延迟系统字体回退、完整 CFF 字体及结果标签控件复用已实施。
- 相同原生欢迎页的 physical footprint 约 409 → 191 MiB，GC 后存活 Go 堆约 255 → 43 MiB；两查询页约 266 MiB。没有在产品中周期强制 GC。
- 更新并正常重启实际 QA，保留连接和草稿。恢复一个查询页约 218 MiB；电脑控制执行本地演示常量 SELECT 后约 228 MiB，结果实际显示正确。
- 最新应用全量单测、UI race、vet、架构 / 文件体积 / 资产来源检查通过；Fyne painter/cache 单测和 race 通过。固定副本的 2,147 份文件及三文件补丁校验通过。
- 字体保持全部 92,670 个字符映射和 93,108 个字形字宽；逐字符覆盖和文件摘要测试通过。
- 重新生成全部 22 个 Agent 和应用 ZIP，原生启动 / 离线 SQLite / 工作区独占 / 正常退出烟测通过；ZIP CRC、路径、包内外文件一致性通过。
- 额外 Fyne container 全套有七类 DocTabs 快照失败，原始未修改依赖也出现相同失败；不能声称 Fyne 所有测试通过。短时测量也不能证明长期无泄漏。

详细证据、补丁边界、度量差异和复现步骤见[内存分析](memory-analysis-2026-10-02.md)。

## 第二轮内存精简复查

- 相同原生窗口配置的欢迎页 footprint 191.00 → 180.11 MiB，两个查询草稿页 236.94 → 214.16 MiB；GC 后存活 Go 堆分别 42.89 → 33.06、59.55 → 47.23 MiB。
- 延后未用备用字体，完整 CFF 字体指令压缩约 17.5%；全部字符映射、字形顺序、度量和轮廓保留。FontTools 与 go-text 分别核对全部字形，重新生成资源与部署字节完全一致。
- 局部主题不再保留完整页面，退休主题范围清理保留独立嵌套主题；编辑器 / 结果主题只持有显示状态。弱引用回归验证相关页面模型能回收。
- 文档标签和操作控件复用，删除标签解除回调并清空切片尾部。切换、重命名、重排、选择颜色、换行与主题回归通过。
- 完整应用单测、UI race、vet、全部驱动标签、架构 / 体积 / 来源检查通过；Fyne painter/cache race、受影响 container 回归通过。固定副本 2,149 份文件、七份改动 / 测试文件的补丁摘要校验通过。
- 全部 22 个 Agent、应用和 ZIP 重新构建，原生烟测与 23 项产物校验通过。正常退出旧 QA、备份工作区及应用、新 QA PID 29599 健康握手与构建摘要一致。
- 电脑控制执行本地演示 SQLite 常量 SELECT，实际显示整数、文本与 NULL，主进程约 234.8 MiB。原 MySQL 密码为仅本次会话保存，重启后需重输；原配置和草稿保留，不用这一状态与旧截图直接计算优化百分比。

详细方法、回收边界、源代码补丁、产物摘要和复现步骤见[第二轮内存精简](memory-refinement-2026-10-02.md)。

## 视觉自检

已生成并查看浅色查询、深色查询、表数据和字段设计器的 **Fyne 软件渲染截图**。
本地路径及具体对照见[design-qa.md](../design-qa.md)，截图元数据见
`selfcheck-parity-images.json`。软件截图不构成真实 macOS 输入法、鼠标 / 键盘或窗口验收。

初次原生复查遇到 Mac 锁定；之后在内存修复中已完成真实 QA 重启、中文界面显示、
常量 SELECT 点击执行和结果显示复查。未丢弃连接 / 查询草稿，未操作用户业务库写入。
随后再次打开 SuperLink 和实际 QA，统一为 2940 × 1724 物理像素窗口，获取并检查
17 张新的原生截图，完成欢迎、数据源、MySQL 基本 / 网络页、新建查询和设置页的
采样对照；结果未达到一比一。另实际展开本地 SQLite 并打开 `products` 数据、
字段和 DDL，仅证明这些页面能打开，未做两端同表对照。
新发现 MySQL / MariaDB 图标空白、关闭图标为实心方块及多位行号截断；
尚未修复，详见[原生对比报告](ui-comparison-2026-10-02.md)。
中文 IME、拖动窗口缩放和完整原生逐页对照尚未验收。
未进行浏览器测试、Git 提交、推送、线上发布或云端 CI 运行。

## 仍未完成

完整对照见[功能矩阵](superlink-ui-observation-2026-10-01.md#5-与当前-superlink-的逐项差距2026-10-02-更新)。
尤其包括格式化 / 补全 / 执行计划、完整对象类别与右键动作、列管理 / ER、
外键 / 触发器编辑、事务工作台、数据库编辑快照 / 诊断、完整文件向导和任务历史、
比较 / 同步 / 调度 / 持续同步、AI / MCP / Skills、云备份与完整设置中心。

全量参数 / 多结果导出和流式 binary 保真未实现；其他数据库的新分页 / 编辑 / 结构
行为未全部真实测试。Windows / Linux、本机原生 IME / 高负载、正式签名公钥 / Release
及各平台生产安装仍待验收。不能因单测、软件截图或 Agent metadata 通过宣称一比一完成。

## 后续增量：SQL / Shell 共用日夜主题

已增加标题栏太阳 / 月亮切换，两套工作区共用背景、文字、边框、面板和主色；保存当前工作区选择并在重启恢复。打开中的表单和终端缓冲原生往返验证通过，终端中文和 ANSI 输出已实际显示，补齐此前锁屏后未完成的对应检查。Shell SVG 轮廓误填充已修复，并有栅格回归测试。

最新代码全量 `go test ./...`、UI / application / infra race、`go vet ./...`、体积 / 分层 / 两套资产 / Fyne 固定副本检查通过。已更新主程序、应用 ZIP 和 23 项清单，正常关闭隔离 QA，并打开最新版日常应用，原查询草稿恢复且未执行业务 SQL。当前摘要及证据见[共用主题自检](shared-appearance-2026-10-02.md)；上文产物表保留为历史记录。

## 后续增量：应用更名为 SuperLink

应用显示名称与打包目录已统一为 SuperLink，沿用原应用 ID 与数据目录。相关 UI / 实例锁 / 更新单测、全驱动打包、23 项产物校验和隔离原生升级烟测通过；电脑控制确认新名称、已有连接和查询草稿恢复。已重新启动日常应用，当前产物摘要见[SuperLink 更名自检](superlink-rename-2026-10-02.md)；共用主题报告的摘要保留为更名前记录。

## 后续增量：连接状态圆点

SQL 连接和库节点增加红 / 绿 / 黄状态点，跟随真实 Engine 会话更新；断开后清除缓存，重新展开可重连。application / UI 单测、race、vet、分层和源码体积检查通过；原生本地 SQLite 验证连接中、成功、失败、断开、重连及夜间显示。当前产物摘要和具体边界见[连接状态自检](connection-status-2026-10-02.md)，更名报告中的摘要保留为此前构建记录。
