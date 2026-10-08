<div align="center">

# SuperLink

### 数据、终端与笔记，一个原生工作台。

在 SQL、Shell 与 Note 之间轻松切换，把查询、远程连接和工作记录留在同一处。

[![Release](https://img.shields.io/github/v/release/ealink1/super-link?label=release&color=16803d)](https://github.com/ealink1/super-link/releases/latest)
[![Native checks](https://github.com/ealink1/super-link/actions/workflows/ci.yml/badge.svg)](https://github.com/ealink1/super-link/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-475569)](LICENSE)

**[下载 SuperLink](https://github.com/ealink1/super-link/releases/latest)** ·
[版本记录](https://github.com/ealink1/super-link/releases) ·
[使用与构建](#运行与构建) ·
[开发文档](#开发文档)

macOS · Windows · Linux | ARM64 · x64 | Go + Fyne

</div>

---

## 一个工作台，三种工作方式

| SQL · 数据工作区 | Shell · 终端工作区 | Note · 笔记工作区 |
| --- | --- | --- |
| 管理连接、浏览数据库、编写查询 | 管理主机、连接 SSH、打开本地终端 | 编写 Markdown、预览与分屏 |
| 筛选表目录、分页读取、暂存数据编辑 | 多标签 PTY、基础 SFTP 与 Linux 监控 | 分组、标签、全文搜索与回收站 |
| 结构查看、导入导出、受保护提交 | 密码 / 私钥认证、隐私模式 | 本机加密自动保存、导入导出 |

**原生桌面。** 使用 Go + Fyne，不依赖 Java / JDK、Wails、React 或 WebView。
**本地工作区。** 连接、草稿和笔记保存在本机；凭据加密保存，查询结果不进入历史。
**多源接入。** 36 类固定数据源与自定义 Driver / DSN，应用包自带离线 SQLite。
**可验证更新。** 启动检查稳定版本，下载进度可见；校验签名与哈希后替换应用，健康确认失败时回滚。

## v0.1.7 · 当前稳定发布

- **表浏览更顺手**：紧凑列宽、清晰字体和字段类型；目录支持连续浅色悬停高亮与右键菜单。
- **字段管理更直观**：展示主键、自增、非空、无符号和默认值；提供新增、复制、编辑和暂存删除入口。
- **细节更清晰**：工具栏悬停说明、分割线与标签对齐；DDL 使用高对比文字。
- **修复导出闪退**：文件导出完成后正常关闭进度框并提示成功。

[下载 v0.1.7](https://github.com/ealink1/super-link/releases/tag/v0.1.7) ·
[发布与验证记录](docs/releases.md)

六个平台的原生构建、测试与安装包校验已通过。构建通过不等于所有真实服务和桌面环境均已验收。
完整功能与界面对齐仍在推进，**JVM 管理连接器不在范围内**；已实现能力与剩余工作见下文。

## 运行与构建

本次开发与原生自检环境为 **macOS arm64、Go 1.26.3、Fyne 2.8.1**。
需要 Go、Python 3，以及 Fyne / CGO 所需的本机编译工具。
macOS 使用 Xcode Command Line Tools；Linux 需要 OpenGL / X11 开发库；
Windows 使用 GCC。其他平台的完整应用尚未在本次机器上运行验收。
Linux 的系统文件选择器需要 `zenity`（DEB 包会声明此依赖；便携包需系统预先安装）。

```sh
# 默认构建主程序、更新 Helper 和离线 SQLite Agent
python3 tools/build.py
./bin/superlink

# 构建全部 22 个可选原生 Agent，并打包应用
python3 tools/build.py --all-drivers --package

# macOS 打包应用
open bin/SuperLink.app
```

开发程序自动发现 `bin/drivers/bundle.json`。标准 `.app` / Portable 应用包自带
SQLite，其他 Agent 作为独立产物放在 `dist/`。应用中打开 **驱动管理 → 导入本机
可信驱动包**，选择包含 `bundle.json` 的 `bin/drivers` 目录，可以离线安装其余驱动。
每个 Agent 都校验本机架构、SHA256、驱动身份、兼容修订和 IPC 协议。

```sh
# 也可显式指定驱动目录和独立工作区
./bin/superlink --drivers ./bin/drivers --data-root /absolute/path/to/workspace

# 单独构建某个驱动
python3 tools/build.py --driver sqlite --driver duckdb --skip-app
```

每次驱动构建会重写所选 Agent 的 `bundle.json`；需要完整离线目录时使用
`--all-drivers`。生成的二进制和测试缓存不会加入 Git。首次 Go 构建需要下载
锁定版本的依赖；CGO Agent 应在目标平台原生构建，不能仅设置 GOOS 就声称跨平台可用。

## 已实现功能

| 范围 | 当前行为 |
| --- | --- |
| 连接管理 | 类型搜索 / 分类、基本 / 网络 / 外观 / 高级表单、URI、环境、分组、库范围过滤、细粒度保护、测试、保存后连接、断开 |
| 主工作区 | 上游绿色主题和图标、单行工作标签与悬停连接 / 库提示；连接节点显示连接状态圆点，未连接时隐藏，数据库节点不显示圆点；单击库打开表目录；连接 → 数据库 → schema → 表 / 视图的惰性树；加载失败可重试；双击表打开独立数据页 |
| SQL | 原生文本编辑和显示层语法高亮、行号、查找、换行、撤销 / 重做、选中执行、Cmd/Ctrl+R、停止；连接 / 库 / schema 选择，日志 / 参数 / 结果页 |
| 参数 | 命名参数及类型、列表绑定；传入驱动参数而非拼接 SQL；缺参数、非法数值和超限内容在执行前拒绝 |
| 查询文档 | 自动草稿、命名已存查询、版本冲突检测；重新打开不执行；SQL 文件打开和原子导出；保留标题、库和 schema |
| 结果 | 虚拟网格、列宽拖动、行选择、NULL / 精确整数 / 二进制类型、JSON / 文本视图、单元格详情和复制 |
| 表数据 | 100 行起始服务端分页、总数、条件构建 / 手工只读条件 / 排序；增删改暂存、SQL 预览、主键和原值并发检查、参数化整批事务提交 |
| 结构设计 | 字段和索引暂存编辑、SQL 预览、保存前结构哈希检查；外键 / 触发器只读；DDL 按驱动能力显示 |
| 导出 | XLSX / CSV / JSON / Markdown / HTML / INSERT SQL；结果和列选择；已加载结果或无参数单条读取的流式重查；取消和失败保留目标原文件 |
| 导入 | CSV / TSV / JSON / XLSX 预览、编码、目标列映射、值检查、显式确认、分批事务、失败 / 取消报告；已有目标表 |
| MongoDB | Collection 浏览、Filter 表单、原生 JSON 命令、find 限量、写错误检查 |
| Elasticsearch | 索引浏览、原生 REST Console、JSON / NDJSON 请求、HTTP 状态与响应展示 |
| Redis | SCAN 对象预览、值读取命令、类型 / TTL；禁止 KEYS、脚本和阻塞订阅 |
| 向量数据库 | Collection 浏览、条目预览命令、数值向量检索表单和原生 JSON 命令 |
| 消息协议 | Topic / Queue 浏览、限量读取、停止、显式发布；说明各协议读取副作用 |
| Nacos | Namespace、配置列表 / 内容、服务 / 实例；显式发布和删除 |
| 本地状态 | SQLite WAL（schema v2）、连接及已存查询版本冲突检查、自动草稿、脱敏历史、独立实例锁 |
| 凭据 | “记住密码”在本机加密保存，重启自动读取；可选择仅本次运行保存 |
| 驱动与更新 | 本机可信包导入、签名 Release 驱动安装、启动检查新版、可取消的下载进度、完整应用替换、启动健康检查和失败回滚 |
| 界面 | Fyne 原生窗口、中文字体、明暗主题；网络和存储任务在后台执行 |
| Note | 独立笔记工作区；Markdown 编辑 / 预览 / 分屏、格式按钮与撤销重做，分组 / 标签 / 全文搜索、回收站与恢复、本机加密自动保存、Markdown 导入和原子导出 |
| Shell | 独立 iShell 风格工作区，与 SQL 共用日间 / 夜间主题；分组 / 标签 / 搜索 / 网格主机管理、密码 / 私钥、真实本地与 SSH PTY、多标签、基础 SFTP / Linux 监控；高级页面按阶段实施 |

窗口右上角的主题按钮切换整个应用的日夜模式。
三个工作区的颜色同步，设置随当前工作区保存，重启自动恢复；切换保留查询、弹窗输入和终端会话。

SQL 侧边栏的绿色圆点表示应用存在已建立的数据库会话，红色表示连接失败或会话异常丢弃；尚未连接或主动断开时隐藏圆点，黄色表示正在连接。保存配置或恢复草稿不会自动变绿；连接、查询、断开及重试会同步更新状态。圆点反映应用已知会话，不额外轮询服务器。

语法高亮由原生显示层实现；元数据补全、SQL 格式化和执行计划尚未完成。
任意 SQL 查询结果保持只读；独立表数据页支持受保护编辑。结构设计目前只对
已适配的 MySQL、PostgreSQL、SQLite 方言开放字段 / 索引变更，其他类型不冒充支持。

## 数据源覆盖

目录包含 **36 类固定数据源 + 自定义 Driver / DSN，共 37 个入口**。
“接入实现 / 原生编译”与“真实服务验收”分开记录，不能将编译通过视为全部功能已实测。

| 接入方式 | 类型 |
| --- | --- |
| 内建 Go 客户端 / 协议 | MySQL、GoldenDB、PostgreSQL、Oracle、Redis、Chroma、Qdrant、Milvus、RocketMQ、MQTT、Kafka、RabbitMQ、Pulsar、Nacos |
| 可选原生 Agent | MariaDB、OceanBase、Doris、StarRocks、Sphinx、SQL Server、SQLite、DuckDB、Dameng、Kingbase、HighGo、Vastbase、openGauss、GaussDB、IRIS、Caché、MongoDB、TDengine、IoTDB、ClickHouse、Elasticsearch、Trino |
| 自定义 | 当前主进程已注册的 Go SQL Driver 和 DSN；不加载任意 JDBC / ODBC 驱动 |

22 个可选 Agent 均在本机完成编译与真实进程 metadata 握手。已完成真实数据操作
验证的类型为 SQLite、DuckDB、MySQL、PostgreSQL、Redis、MongoDB、Elasticsearch、
MQTT、RabbitMQ、Qdrant、Chroma、Nacos。其他类型保留接入实现，但尚无真实服务验收。
具体服务版本、操作和限制见自检报告。

### 第一次使用 SQLite

1. 新建连接，类型选择 SQLite，主机 / 文件填写 `:memory:` 或本地文件绝对路径。
2. 保持只读保护可执行 `SELECT 1;`。需要建表时取消只读保护并保存。
3. 选择连接后新建查询，执行 SQL；写语句经检查并显式确认后执行。
4. 展开连接树并双击表，使用分页、筛选、编辑和结构设计；导入或导出数据。

`:memory:` 的数据属于当前数据库会话，断开或退出即丢失；需要持久数据请使用文件。

## 配置与操作边界

- 图形表单中的 URI 优先于基本主机、端口、用户名、密码、数据库和 Hosts 字段。
  范围输入用于选择目标数据库；Oracle 的范围是 schema，不替换服务名 / SID。
  SQLite / DuckDB 使用文件路径，自定义驱动使用 DSN。
- SSH 必须使用 known_hosts 或明确的服务器指纹。原生协议客户端保留其 TLS / SSH /
  代理能力；代理与 SSH 组合会转发网关并保留真实网关的主机密钥身份。
- SQL 代理当前仅支持单主机、基本地址字段、无 TLS 的 TCP 转发；TLS、URI / DSN、
  多主机或连接参数覆盖地址的组合明确拒绝。Pulsar 的 broker 发现路由暂不支持隧道。
- TLS 具体认证能力取决于驱动和参数。`required`、自定义 CA、`sslmode=verify-full`
  等不是所有协议共有的等价设置；`preferred` / `skip-verify` 具有兼容性降级行为。
- 连接默认只读。写入确认绑定连接 ID、修订、范围和实际文本，短时有效且只用一次。
  后端再次检查；绑定参数、schema 和操作类别也纳入确认内容。失败 / 取消的写入
  不自动重试，远端结果不确定时明确报告。只有已确认回滚才保留原会话。
- 高级 JSON 会拒绝未知字段，隐藏凭据在编辑后保留；显式写入空值可清除对应凭据。
  `protection` 对数据编辑、结构编辑、交互脚本和导入分别检查；全连接只读优先。
- 手工事务控制、部分管理命令和有副作用的 SQL 函数被拒绝。界面保护无法判断所有
  数据库自定义函数，生产环境应使用服务端只读账号和数据库权限。
- RabbitMQ Management API 预览使用 requeue，会影响顺序 / redelivered 状态，因此
  属于有副作用操作。Kafka 预览不提交业务 Consumer Group 位点；MQTT 读取是订阅期间采样。
- SQL / MongoDB 的读取预算进入扫描或 Agent：最多 10,000 行、约 64 MiB 结果预算，
  大字段约 1 MiB 预览并标记截断。预算不等于所有第三方 SDK 的进程内存硬上限；
  Redis / 部分 HTTP 或向量返回会先读取完整协议响应。大对象应使用服务端范围 / 分页参数。
- Redis SCAN 对象预览最多 1,000 个 Key / 20 轮；Nacos 列表每页 100 条；通用对象浏览
  尚未提供完整分页控件。取消速度取决于驱动，旧 Connect 接口以配置超时完成清理。
- CSV 转义可能被表格软件识别为公式的字符串和标题；NULL 在 CSV 中写为文本 `NULL`，JSON
  保留 null。参数查询、多结果和写操作仅支持已加载结果导出；流式重查上限为
  500 万行 / 8 GiB / 单字段 1 MiB，达到上限失败，不发布残缺文件。
- 表数据修改要求主键及完整原值定位，单批最多 1,000 行 / 16 MiB；无安全行定位、
  生成列、视图或截断数据拒绝编辑。MySQL DDL 可能逐条提交，失败报告保留已执行数量。
- PostgreSQL 上游尚未实现完整 CREATE DDL，本项目明确显示不可用及元数据警告，
  不将占位注释作为完整备份。全量流式导出的二进制值仍受上游展示归一化限制。

## 工作区与凭据

默认目录：`os.UserConfigDir()/SuperLink`；macOS 通常为
`~/Library/Application Support/SuperLink`。可用 `--data-root` 或 `SUPERLINK_DATA_ROOT` 指定。
应用 ID 为 `io.github.ealink1.superlink`；目录优先级为 `--data-root`、`SUPERLINK_DATA_ROOT`、默认目录。
不自动读取、迁移或修改旧版工作区，不识别旧环境变量。
不复用 SuperLink 的数据目录，同一工作区同时只允许一个进程打开。

| 内容 | 保存位置 / 行为 |
| --- | --- |
| 连接元数据、草稿、已存查询、脱敏历史、设置 | `state.sqlite`，WAL；数据库文件 0600，工作区 0700 |
| 密码、Token、URI / DSN 和敏感连接参数 | 勾选“记住密码”时在 `credentials/` 中 AES-256-GCM 加密保存；否则仅本次运行内存保存 |
| 可选驱动 | 工作区 `drivers/`，校验记录与本机原生 Agent |
| 更新下载、报告、状态快照 | 工作区 `updates/`；旧应用保留在原目录旁的 backup 目录 |

新建连接默认勾选“记住密码”，修改已有连接保留原选择；点击“保存”才写入，测试连接不会保存。
密码存储不调用系统钥匙串，不弹电脑密码授权。密钥 `credentials/master.key` 随机生成，
macOS/Linux 目录权限 0700、文件 0600；Windows 使用工作区继承的 ACL，尚未做原生权限验收。
本地密钥与密文同在本机，同账户下可读取整个工作区的程序仍可能解密，不能视为钥匙串等价保护。
备份密码需保留完整的 `credentials/`；密钥丢失或损坏不会自动替换。旧版钥匙串凭据不读取或删除，
需要重新输入一次并保存；旧会话凭据在重启后也需要重新输入。降级旧版不能读取新版本地密文。
无凭据的本地连接可以直接恢复。草稿保存原始编辑内容，可能包含业务数据；
已存查询最多 500 份，每份 SQL 至多 1 MiB，只保存文本和上下文，不保存参数值及结果。
查询历史经过脱敏，不能直接作为完整语句重放。删除连接会清除其草稿、已存查询、历史和凭据引用。
AI 普通问答需自行配置兼容接口；仅在点击发送后，将当前聊天输入及成功对话历史发送给所配置的服务。不会自动附带数据库结果、SSH 输出或笔记。API Key 加密保存在本机，聊天不落盘。没有遥测或云备份。

## 应用内更新

更新目标固定为本项目 GitHub Releases。稳定发布需提供 **Ed25519 签名的
manifest.json**、对应签名、平台应用 ZIP 和原生 Agent。应用依次验证签名、仓库 /
平台 / 版本、长度、SHA256、包路径，再由 Helper 等待退出后替换完整包。
新版本健康确认失败会恢复旧应用和更新前的 SQLite 状态快照；成功后保留备份。

**v0.1.7 已公开发布**，发行包内置更新验证公钥，可通过设置中心手动检查更新，
也会在启动完成后自动检查。仅发现可用的新版本时弹出更新确认；自动检查无更新或失败时不弹窗。
确认后显示下载进度与大小，下载及校验完成后询问是否替换当前应用并重启。

签名应用更新清单与操作系统代码签名承担不同职责。当前 macOS 包采用 ad-hoc 代码签名，
尚未配置 Developer ID 签名及 Apple 公证。连接局域网数据库或 SSH 主机时，需允许
SuperLink 访问本地网络；升级后的权限延续仍需稳定的 Apple 签名身份。

开发裸二进制可下载校验包但提示手动安装；原位更新用于包含 package marker 的应用包。
六平台安装包已通过云端构建与校验，真实桌面及生产环境验收边界见
[发布文档](docs/releases.md)。

## 自检

```sh
python3 tools/check-architecture.py
python3 tools/verify-upstream.py
python3 tools/verify-ui-assets.py
go run ./tools/check-go-size
go test ./...
go test -tags superlink_full_drivers ./internal/upstream/db ./cmd/driver-agent
go test -race ./internal/application ./internal/infra/... ./internal/domain ./internal/ui ./cmd/release-sign
go vet ./...
python3 tools/build.py --all-drivers --package

# 真实本地文件数据库；需先构建对应 Agent
SUPERLINK_TEST_DRIVERS="$PWD/bin/drivers" go test -tags integration -count=1 -v ./internal/infra/runtime -run 'TestLocalFileAgents|TestLocalAgentsBoundValuesAndOptimisticChanges|TestSQLiteAgentSessionMetadataDoesNotWaitForItsOwnPool'

# 隔离 Docker 服务，只发布到 127.0.0.1 临时端口并在结束时清理
python3 tools/integration.py --docker-context YOUR_TEST_CONTEXT --group all

# 桌面图形会话中的真实原生启动检查
python3 tools/native-smoke.py

# macOS 私有测试应用的真实 Helper 更新，不替换日常使用的应用
python3 tools/native-smoke.py --upgrade
```

`make selfcheck` 提供核心检查和全部 Agent 构建、应用打包。GitHub Actions 已配置默认测试和
macOS / Windows / Linux 的 ARM64 / x64 六平台原生构建；v0.1.7 的 CI、发布构建与资产校验已通过，
记录见 [发布文档](docs/releases.md)。
按用户要求，自检不使用浏览器。

## 后续功能

这些仍属于用户要求的完整复刻范围，尚未完成；不以入口或外观相似标记交付。
完整清单见逐项对照和实施计划第 13–15 节，主要包括：

- 元数据补全、SQL 格式化、执行计划、完整搜索 / 替换、全屏和编辑器设置。
- 对象树中的序列、函数 / 存储过程、触发器；置顶、重命名、清空、删除、ER 图。
- 外键和触发器编辑、唯一键替代行定位、手工事务工作台、自动提交模式。
- 导入完整向导、建表 / SQL 脚本导入、任务历史、错误继续策略；全量参数 / 多结果导出。
- 数据库编辑快照、诊断、慢查询、备份恢复。
- 跨库迁移、数据 / 结构比较与同步、任务恢复和检查点。
- 更完整的协议工作台、分页、运维信息和权限适配。
- 外部配置预览式导入；完整设置中心、字体 / 快捷键持久化、插件、国际化、AI 自动执行 / MCP / Skills 与云备份。
- Windows / Linux / macOS 两种 CPU 的真实平台验收、签名公证和生产更新验证。

## 开发文档

功能设计、验证记录和上游归属在此集中维护。日期文档记录当时的验收结果，
最新发布状态以 [发布记录](docs/releases.md) 为准。

<details>
<summary>展开设计与验证资料</summary>

- [实施计划及高级功能路线](docs/plans/2026-10-01-superlink-implementation-plan.md)
- [完整功能与界面对齐设计（当前范围）](docs/plans/2026-10-01-superlink-parity-design.md)
- [参考界面、操作流程与功能对齐清单](docs/superlink-ui-observation-2026-10-01.md)
- [最新代码自检与未验证范围](docs/selfcheck-2026-10-02.md)
- [内存问题定位、修复与原生对照数据](docs/memory-analysis-2026-10-02.md)
- [第二轮内存精简与页面释放复查](docs/memory-refinement-2026-10-02.md)
- [记住密码与免钥匙串授权自检](docs/remember-password-2026-10-02.md)
- [SQL / Shell 独立工作区与 iShell Pro 全功能分阶段计划](docs/plans/2026-10-02-sql-shell-workspaces-design.md)
- [Shell 首阶段功能、操作和自检边界](docs/shell-workspace-2026-10-02.md)
- [iShell Pro 原生观察、Shell 界面重做与最新产物](docs/shell-ui-ishellpro-2026-10-02.md)
- [SQL / Shell 共用日夜主题、标题栏切换与原生自检](docs/shared-appearance-2026-10-02.md)
- [SuperLink 完整命名与自检](docs/superlink-namespace-2026-10-03.md)
- [连接状态圆点与原生自检](docs/connection-status-2026-10-02.md)
- [AI 右侧普通问答、配置方法与自检](docs/ai-chat-2026-10-03.md)
- [界面对照验收与剩余差距](design-qa.md)
- [构建、签名、跨平台安装包与自动 Release](docs/releases.md)
- [上游来源与改动](UPSTREAM.md)

</details>

## 许可

项目沿用 Apache-2.0，并保留上游贡献者及第三方归属说明。
查看 [LICENSE](LICENSE)、[NOTICE](NOTICE)、[UPSTREAM.md](UPSTREAM.md) 和
[第三方许可文本](THIRD_PARTY_NOTICES.md)。图标保留上游来源；字体衍生自已许可的
Inter / Noto Sans / Noto Sans SC / DejaVu Powerline，不再分发系统字体。
专有驱动的再分发条款需在正式分发前核对。
