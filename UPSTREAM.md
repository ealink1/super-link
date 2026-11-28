# Upstream provenance

SuperLink is a Rust + GPUI desktop application. Application services, drivers,
SSH/SFTP, update helper and asset tools are implemented in Rust. This branch has
no Go source files, Go modules, Go compiler requirement or Go child process.
SQLite and DuckDB use their bundled native database engines through Rust crates;
OS frameworks and Cargo dependencies retain their own licenses.

## Historical attribution

The original application and reviewed protocol behavior derive from
[Syngnat/GoNavi](https://github.com/Syngnat/GoNavi), Apache-2.0, pinned at
`6e20b6ddf56b2ae76f7e5d5c6a3505cef1edc871`. `LICENSE` and `NOTICE` preserve that
attribution. Go/Fyne source was retired on 2026-11-02 at the user's request.
Earlier implementation notes are retained as historical material in
`docs/archive/go-app/upstream-history.md`; they do not describe current runtime dependencies.
The old adaptation patch and Go module manifests were removed together with the Go source.
Original source remains available through Git history and the `main` branch.

## Reviewed Rust adaptations

- `rust/apps/driver-agent` replaces all 37 former process entry points. MySQL and
  PostgreSQL compatible products share their protocol implementations; Oracle,
  Dameng, SQL Server, IRIS/Caché, IoTDB and message services have dedicated adapters.
  The binary reports `implementation: rust` and `json-lines-v3-rust`, preventing
  accidental use of legacy Go agents. Connection-scoped processes bound requests,
  responses and preview results; typed IPC preserves int64, exact decimal text,
  binary values and NULL. SQLite/MySQL/PostgreSQL exports read cursor chunks.
  MySQL Navicat script tunnels retain their bounded binary response protocol;
  scripts refuse cross-request transactions. Oracle service names, catalog schema
  selection, DuckDB file paths and virtual message catalogs stay separate.
- IRIS/Caché list framing, credential obfuscation and SuperServer requests were
  ported from `caretdev/go-irisnative` (MIT). Its original license is retained in
  `third_party/protocol-licenses/go-irisnative-MIT.txt`. No Go source is linked or launched.
- HighGo authentication code 13 and the two-stage SM3 password response follow
  the former MIT `highgo-pq` adapter. Original license:
  `third_party/protocol-licenses/highgo-pq-MIT.txt`.
- `rust/vendor/tokio-opengauss` and `rust/vendor/opengauss-protocol` preserve
  crates.io 0.1.0 sources and original licenses. Reviewed changes add HighGo SM3
  authentication and remove protocol debug output from stdout.
- `rust/vendor/dameng` preserves the MIT dameng 0.1.0 crate. Reviewed changes bound
  protocol/LOB allocation, enforce exact frame reads, stop timeout polling,
  bound retained fetch rows, preserve verified TLS options and correct UTF-8/quoted
  placeholder substitution in the OPE fallback. The crate archive omits its license text; the original
  `LICENSE.txt` is retained from https://github.com/rarnu/rust-dameng.
- `superlink-tools` replaces the previous Go icon and catalog ZIP generators.

`docs/upstream-import.json` records source/current hashes for retained resources,
licenses and Rust forks. `docs/rust-upstream-adaptations.patch` records the Rust
fork diffs against the crate archives. Run `python3 tools/verify-upstream.py` after
changes; provenance updates require review of changed source and hashes together.

## Assets and notices

Interface assets retain source paths and hashes in `assets/superlink/sources.json`
and `assets/shell/sources.json`. Lucide shell icons retain their original license.
Noto Sans SC and historical renamed font derivatives retain OFL/license texts in
`assets/OFL.txt` and `third_party/fonts`. Trademark rights are not transferred by
source-code licenses. Translation resources remain under `internal/upstream/i18n`;
they contain JSON/ZIP data only. Unused historical TypeScript helpers were retired.

`THIRD_PARTY_NOTICES.md` is generated from Cargo metadata and available original
license files by `python3 tools/generate-notices.py`. Cargo.lock pins dependency
versions. Platform SDK/database-engine license terms remain applicable.

## Verification limits

Pure Rust source/build/runtime replacement and native functionality are distinct
from live service compatibility. The current service/network verification matrix
and unsupported combinations are tracked in
`docs/plans/2026-11-02-pure-rust-drivers.md`; compile/identity probes do not constitute
live integration acceptance of all 37 products. JVM management remains excluded.

## 2026-11-02 Shell monitor parity assets

`assets/shell/monitor-*.svg` retains the original project's
`monitorMetricIcon` geometry from `9816f626^:internal/ui/shell_monitor_cards.go`.
Only the standalone SVG wrapper/currentColor binding changed. Exact immutable
source revision and new SVG hashes are recorded in `assets/shell/sources.json`
under `project_assets`; these project drawings are not labeled as upstream
Lucide originals. No historical Go source was restored to the working tree.

## 2026-11-03 SQL 工作区交互参考

按用户提供的 GoNavi 截图和已安装原生应用的只读界面观察，独立实现 Rust/GPUI
SQL 工作区：连接树常驻及类型分组、多表标签、可调整列宽的滚动网格、底部分页、
字段类型、JSON 预览、单元格完整值查看/复制与暂存变更保护。
未复制 GoNavi 代码、图标或打包资源；继续复用现有已归档资源与 SQL 安全构造器。

### SQL 表分类补充

继续基于用户截图及 GoNavi 原生只读界面观察，独立实现列/索引/外键/触发器分类与
右侧结构浏览联动、数据库范围已存查询与行数标注。分类复用既有已归档 SVG，未新增
GoNavi 代码或二进制资源；外键和触发器编辑未声称已迁移。

### 2026-11-03 可编辑字段设计器

按用户提供的 GoNavi 字段设计器截图及已安装原生界面的只读观察，独立实现 Rust/GPUI
字段草稿、类型候选、字段属性、多选、增删/插入/排序、字段复制粘贴、目标表复制与表备注。
新增纯 SQL 变更计划与应用层完整元数据回读、保存前冲突检查。MySQL 字段变更合为一条
ALTER TABLE，字段改名/排序保留已有复合主键顺序；提交时仅规范当前连接的字符串转义模式。
修正原有 MySQL DDL COMMENT/DEFAULT 使用 CONVERT 表达式导致的语法错误；查询过滤字面量
编码保持不变。无 GoNavi 源代码、图标或打包资源引入。

### 2026-11-03 表格横向滚动条

按用户标注的 GoNavi 底部滚动条独立实现常驻轨道、比例滑块、拖动和轨道定位，复用表头与数据
共同的 ScrollHandle。补齐 SQL 工作区/滚动容器宽度约束，防止宽表撑开外层导致滚动范围为零；
纵向区限制滚动轴并显式转发横向滚轮/Shift 滚轮。未引入 GoNavi 源代码或资源。

### 2026-11-03 滚轮方向修正

按用户明确交互规则，在数据网格滚轮事件的捕获阶段统一分发：普通 Y 滚轮只操作纵向数据，
Shift+Y 只操作横向列；兼容平台已转换为 X 的 Shift 滚轮与触控板横滑。阻止嵌套原生滚动器
再次处理同一事件，并禁用滚动轴自动映射。

### 2026-11-03 表头悬停与排序入口

字段表头新增原生 GPUI tooltip，显示完整字段名、类型和已加载的主键/空值/默认值/注释等元数据。
移除整块表头单击排序，改由独立排序箭头触发，避免查看被截断字段时意外重新查询。

### 表头控件尺寸样式补充

按用户参考图将单一排序字符替换为紧凑漏斗与上下三角，控件靠首行对齐，排序激活方向高亮。
漏斗复用现有 SVG 并打开当前字段的筛选区；上下三角独立绘制，未复制参考应用资源。

### 2026-11-03 连接树双击展开

按用户要求，分组、连接、数据库和对象目录名称改为双击展开/收起；小箭头继续单击切换，并阻止事件冒泡。保留表名称双击打开数据与箭头展开结构的交互，使用原生 GPUI 实现。

### 2026-11-03 表对象预览

依据用户提供的对象列表参考图，在数据库与表目录名称单击时打开表对象预览，保留双击折叠与箭头操作；预览支持名称搜索、刷新、已加载行数和双击打开表数据。采用现有 Rust 元数据缓存及 GPUI 控件，未复制第三方代码或资源。

### 2026-11-04 固定对象入口

依据用户参考，将 SQL 表标签栏左端改为始终显示、不可关闭的固定“对象”入口，表标签在右侧独立滚动。入口复用当前表数据库或最近对象范围，使用现有原生 GPUI 与对象预览逻辑。

### 2026-11-04 查询与表统一标签栏

依据用户参考图，将查询标签提升到外层工作区，与固定对象入口及表标签共用一行；采用紧凑 30px 栏高、单行标题、细分隔线与小图标，保留查询草稿切换/关闭/新建逻辑。全部为原生 Rust/GPUI 自行实现，未复制参考应用资源。

### 2026-11-04 SQL 标签右键关闭菜单

按用户框选参考，为查询和表标签增加关闭、关闭其他选项卡、关闭右侧选项卡、全部关闭。按可见混合顺序计算目标，固定对象入口不参与关闭，批量操作预先检查表暂存变更与运行状态；沿用原有 Rust 标签释放逻辑，未复制第三方代码或资源。

### 2026-11-04 标签溢出入口

按用户参考移除统一标签栏末尾的新建加号，改为右端固定双箭头“»”入口。原生弹层列出打开的查询与表标签，标记当前标签并支持切换；新建查询保留顶部入口。使用原有 GPUI 组件自行实现。

### 2026-11-24 新建连接表单对齐

依据用户指出的「新建连接与 Fyne 版差距」，按 `main` 分支 `internal/ui/connection_forms.go`、`connection_network.go`、`connection_advanced.go`、`datasource_picker.go` 重建表单：新建先进入带分类计数与搜索的数据源选择器；表单分为「基本 / 网络与安全 / 外观 / 高级」，补回连接串解析/生成/复制、连接参数、数据库显示过滤、拓扑与节点地址、生产连接保护四项、SQLite/DuckDB 文件选择、类型说明、环境与分组、外观图标与颜色（侧栏使用 `iconType`），SSL 验证模式恢复 `required/preferred/disable/skip-verify` 并补充 CA 校验，超时上限改为连接 120 秒、查询 3600 秒，端口按旧版允许留空。原生驱动新增连接串/连接参数合并（MySQL charset、PostgreSQL application_name/options/connect_timeout）。纯 Rust/GPUI 自行实现，未复制 Fyne 代码或资源。

### 2026-11-28 发布签名工具改为 Rust 实现

`main` 分支的发布链路由 Go 的 `cmd/release-sign` 签出更新清单。本分支禁止 Go 源码与 Go 构建依赖，因此在 `superlink-tools` 中新增 `sign` 子命令等价实现：读取 `tools/build.py` 产出的 `assets-*.json`，用 `Manifest::to_signed_bytes()`（两空格缩进 JSON 加结尾换行）编码，以 Ed25519 签名并写出 `manifest.json` 与 `manifest.json.sig`。编码与 Go 工具逐字节一致，`sign::tests::matches_the_published_wire_format` 用固定密钥与固定清单的已知答案锁住这一格式，既有签名清单继续可验。签名前先核对每个文件的体积与 SHA256、下载地址版本，并与 `SUPERLINK_RELEASE_PUBLIC_KEY` 核对私钥推导出的公钥，签名后再用公钥自验一遍才落盘。同时补齐 `tools/verify_release.py`、`tools/release_notes.py`、`tools/publish_release.py` 与 `.github/workflows/release.yml`，把发布校验、说明渲染与草稿发布接到 Rust 产物上；受 GPUI 依赖配置限制，发布矩阵暂时只覆盖 macOS 与 Windows 各两种架构。
