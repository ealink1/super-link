# SuperLink v0.1.0 Alpha 自检报告

> 名称与命令已于 2026-10-03 统一为 SuperLink；历史截图、产物哈希和验收结论仍对应记录当日版本，本次更名验证见 [完整更名记录](superlink-namespace-2026-10-03.md)。

日期：2026-10-01。实施目录：`.`。
目标 remote：`git@github.com:ealink1/super-link.git`。

已实现独立 Go 模块和 Fyne 原生应用，选择性复用固定 SuperLink 基线的数据库 / 协议代码。
目录包含 36 类固定数据源与自定义 Driver / DSN，共 37 个入口；已构建 22 个可选
原生 Agent，并对 12 类数据源完成真实数据操作。当前交付是 Alpha，尚未达到完整
实施计划中全部平台、专有数据库及高级功能的发布验收门槛。

**JVM 管理连接器、其 DTO / UI / 目录入口、Java Helper 和对应测试均已移除。应用
构建与运行不要求宿主机 JDK。** Elasticsearch / Nacos 测试服务自身的 Java 运行时
仅在隔离容器内，不属于客户端的 JVM 连接器。

## 1. 环境与边界

| 项目 | 本次环境 |
| --- | --- |
| 主机 | macOS，darwin/arm64 |
| Go / Fyne | Go 1.26.3 / Fyne 2.8.1 |
| 编译工具 | 本机 clang / CGO，Python 3.11 |
| 上游基线 | `6e20b6ddf56b2ae76f7e5d5c6a3505cef1edc871` |
| 集成服务 | 独立 Colima profile `superlink-selfcheck`，Docker context `colima-superlink-selfcheck` |
| 隔离 | 不挂载宿主目录，服务端口仅绑定 127.0.0.1 临时端口，容器结束后清理 |
| UI 检查 | Fyne 无显示驱动测试 / 渲染检查，以及真实原生应用进程启动和升级 |

按用户要求，未调用浏览器进行测试。没有改动原 SuperLink 应用源码，没有提交 / 推送代码，
没有发布 Release，也没有调用真实生产数据库。测试凭据随机生成，临时配置 0600，
测试结束清理；原生启动 / 更新使用私有测试工作区和临时应用包。

验收结束已停止并删除本次专用 Colima profile / Docker context 和测试应用临时目录，
原有 default profile 仍保持停止，默认 Docker context 未改变。

## 2. 工程检查

| 命令 | 结果 / 覆盖 |
| --- | --- |
| `python3 tools/check-architecture.py` | 通过；核心层不依赖 Fyne / Wails；新生产文件不超过 800 行；没有 JVM 模块 |
| `python3 tools/verify-upstream.py` | 通过；510 个保留上游文件与来源记录 / 当前摘要一致 |
| `go test ./...` | 通过；默认主工程与保留的上游单元测试 |
| `go test -tags superlink_full_drivers ./internal/upstream/db ./cmd/driver-agent` | 通过；全部原生驱动组合及 Agent 合约测试 |
| `go test -race ./internal/application ./internal/infra/... ./internal/domain ./internal/ui ./cmd/release-sign` | 通过；应用、自研基础设施、UI 和签名工具竞争检测 |
| `go vet ./...` | 通过 |
| `python3 tools/build.py --all-drivers --package` | 通过；22 个本机 Agent 构建 / metadata 握手，主程序 / Helper / `.app` / ZIP |

最终重新打包后，独立核对 23 个资产的实际长度和 SHA256、ZIP 完整性、包内主程序 /
Helper 与许可资源的字节一致性，全部通过；再以该最终包运行真实原生更新检查通过。

macOS 链接器报告 `ignoring duplicate libraries: '-lobjc'`，构建和原生启动均成功，
该提示未作为失败忽略其他诊断。依赖由 `go.mod` / `go.sum` 锁定；原生 `.app` 包含
LICENSE、NOTICE、上游来源、第三方许可与字体许可。

## 3. 真实数据源验收

命令：

```sh
SUPERLINK_TEST_DRIVERS="$PWD/bin/drivers" \
  go test -tags integration -count=1 -v ./internal/infra/runtime -run TestLocalFileAgents

python3 tools/integration.py --docker-context colima-superlink-selfcheck --group core
python3 tools/integration.py --docker-context colima-superlink-selfcheck --group messages
python3 tools/integration.py --docker-context colima-superlink-selfcheck --group documents
python3 tools/integration.py --docker-context colima-superlink-selfcheck --group vectors
python3 tools/integration.py --docker-context colima-superlink-selfcheck --group configuration
```

| 数据源 | 环境 | 实际通过的操作 |
| --- | --- | --- |
| SQLite | 本机真实 Agent / 临时文件 | 建表、写入、读取、对象与 DDL、删除；10,000 行预算截断；数据库层只读拒绝写入 |
| DuckDB | 本机真实 Agent / 临时文件 | 建表、写入、读取、对象与 DDL、删除；10,000 行预算截断 |
| MySQL | `mysql:8.4` | 建表、写入、读取、对象与 DDL、删除 |
| PostgreSQL | `postgres:17-alpine` | 建表、写入、读取、对象与 DDL、删除 |
| Redis | `redis:7-alpine` | SET / GET / DEL、中文值、SCAN、类型 / TTL |
| MQTT | `eclipse-mosquitto:2` | QoS 1 保留消息发布、限量订阅读取、清除保留消息 |
| RabbitMQ | `rabbitmq:4-management-alpine` | Management API、持久队列、消息发布、两次 requeue 预览 |
| MongoDB | `mongo:8`，真实 Agent | 原生 JSON insert / find，结果与限量读取 |
| Elasticsearch | `8.19.4`，真实 Agent | 创建索引、中文文档、refresh 后搜索、删除索引 |
| Qdrant | `v1.14.1` | Collection 创建 / 浏览、向量 upsert、相似度搜索、中文 payload |
| Chroma | `1.0.20` | Collection 创建 / 浏览、embedding upsert、query、中文文档 |
| Nacos | `v2.5.1` | 配置发布、中文内容读取、配置列表、Namespace 请求、删除 |

SQL 实测还检查 `9223372036854775807`、中文、NULL，以及重名结果列不会丢值。
镜像标签可能变化，本次实际 image ID、digest 与 linux/arm64 平台记录在
[selfcheck-images.json](selfcheck-images.json)。服务镜像版本不代表已覆盖该产品所有版本。

MQTT 不提供完整 Topic 枚举，实测覆盖订阅期间采样；RabbitMQ 预览会改变消息顺序 /
redelivered 状态，应用将其归类为需确认操作。Nacos 发布成功后只轮询读取等待缓存传播，
不重试写入来掩盖远端结果。

其余数据源已有目录 / 适配实现；可选 Agent 已通过本机编译和真实 metadata 进程握手。
未连接真实服务的数据库不标记为数据操作验收通过，尤其 Oracle、SQL Server、国产
专有库、IoTDB、TDengine、ClickHouse、Trino、Milvus、Kafka、Pulsar、RocketMQ 等。

## 4. 保护、状态与资源验证

- 只读模式在应用入口阻止有副作用命令；SQL lexer 处理注释、引号、美元引用和多语句，
  对写入 CTE、版本化注释、手工事务、危险函数采取保守拒绝。
- 写入确认绑定连接 ID / Revision / Scope / 原文，短时有效且仅能消费一次；取消和
  失败写入不自动重试，连接状态不确定时丢弃会话。
- Mongo 聚合写阶段、ES 写请求、Redis 脚本 / 阻塞命令和消息读取副作用由协议策略检查。
  客户端保护不替代服务端账号权限；所有自定义数据库函数无法仅凭文本准确判断。
- 密码、URI / DSN、敏感参数、SSH / 代理及嵌套凭据拆分到 Vault；SQLite 元数据无明文
  测试凭据。会话 Vault 复制输入 / 输出，删除时清零；冲突保存不破坏旧凭据。
- 高级配置隐藏凭据后再合并不会用空字段覆盖；显式空值仍可清除。未知 JSON 字段和
  尚未实现的细粒度 `protection` 设置会拒绝，避免用户误以为限制已生效。
- SQLite WAL 快照、版本保护、乐观冲突、连接删除级联、实例锁、历史 / 草稿上限均测试。
  关闭草稿重新打开和退出前最后一次快速编辑保存通过回归测试。
- SQL / MongoDB 的扫描预算进入 Agent / 游标，结果列保持有序、重名分辨、NULL 与大整数
  保留；UTF-8 大字段截断不切断字符。其他部分 SDK 先加载协议响应，尚无全协议内存硬上限。
- 代理转发实测字节传输、活动连接终止和监听器关闭；代理 + SSH 保留原网关验证身份。
  不支持的 SQL TLS / URI / 多地址代理组合及 Pulsar broker 隧道路由明确拒绝。
- 驱动执行前校验 SHA256、本机架构、身份与协议，复制后再次校验；安装任务可取消并互斥。
  ELF / Mach-O / PE 文件头检查不能替代对应平台上的实际运行验收。
- CSV 对公式形态的字符串 / 列名转义；JSON 保留 null。导出范围是已加载结果，非全量备份。

系统钥匙串的原生授权弹窗、跨重启存取，以及真实 SSH / TLS 证书与所有协议代理组合
尚未进行端到端验收。本次不会操作用户现有钥匙串或已有数据库来补齐这些检查。

## 5. 更新与原生应用验证

### 清单 / 下载 / 解压 / 回滚测试

通过以下场景：清单字节被改动、未知字段、尾随数据、Release tag 不一致、错误平台、
下载长度 / SHA256 不符、取消下载、越界 ZIP、软链接、大小写冲突与非空解压目标。
签名 CLI 读取实际资产后验签，资产改动时拒绝签名；密钥生成不覆盖旧文件。

失败升级测试会主动将状态改为新值并将 schema 升至 99，再触发启动失败；旧应用、
SQLite schema 与原数据均成功恢复。成功升级保留旧应用和状态快照，工作区锁存在时
Helper 拒绝替换；这些属于本地组件测试。

### 真实桌面进程

```sh
python3 tools/native-smoke.py --upgrade
```

通过真实 macOS `.app` 进程测试：

1. v0.1.0 打开 Fyne 事件循环并写入匹配 Token / 版本 / PID 的健康确认。
2. 私有工作区创建状态并自动安装离线 SQLite；同工作区第二个进程被拒绝。
3. 创建完整 v0.1.1 测试包，运行真实外部 Helper，等待正在使用的 v0.1.0 退出。
4. Helper 替换整个 `.app`，新进程完成健康确认，实际二进制报告 v0.1.1。
5. 保留应用备份、更新前的 SQLite 完整快照和报告；测试进程退出并清理私有目录。

首次原生更新检查的 Helper 已成功，但测试脚本读取了 Helper 按设计删除的健康文件，
因此检查脚本报错。已改为从最终报告取得确认 PID，重新运行全流程通过；没有将该次
脚本失败计为通过。健康确认文件仍按正常流程清理。

正式 GitHub 签名 Release 的线上下载、断网恢复、macOS 公证与 Gatekeeper、Windows
文件占用 / 防病毒、Linux 包管理目录权限及实际发行版本迁移尚未完成。

## 6. 自检中修复的主要问题

| 发现 | 修复与验证 |
| --- | --- |
| 本地文件路径或上游测试 fixture 路径迁入后失效 | 修正证书 / SQL fixture 相对路径；默认与完整驱动测试通过 |
| 原生 Agent 被统一当作 SQL 多结果接口 | 只向支持固定会话的 SQL 驱动发送；MongoDB 原生命令实测通过 |
| Mongo 命令第一字段和 find 无上限 | 命令重排、歧义拒绝、扫描上限与写错误检查 |
| SQLite 单连接 metadata 可能等待自身会话 | 固定连接 metadata 查询，真实文件数据库与只读测试通过 |
| SQL 代理未生效或可能绕过参数中的真实地址 | 明确转发支持边界，含不支持组合的失败测试 |
| URI 被表单默认地址 / 旧凭据覆盖 | GUI URI 优先；默认端口 / Hosts / 凭据清空后交给 URI 解析 |
| schema / 表名预览直接分段与 Oracle LIMIT 不兼容 | 按方言识别带引号的名称路径；Oracle ROWNUM / SQL Server 与 IRIS TOP；注入字符回归 |
| 关闭标签 / 快速退出可能遗漏最后编辑内容 | 保存关闭状态，等待任务并在原生退出后最终落盘；UI race 测试通过 |
| 高级 JSON 的空结构字段覆盖隐含凭据 | 直接生成省略凭据键的公共 JSON，保留 / 显式清除回归通过 |
| 尚未实现的细粒度保护标志被忽略 | 保存和 runtime 入口明确拒绝，不写入无效配置 |
| macOS 应用标记位置、签名后 SQLite SHA 不一致 | Resources 标记识别；嵌套签名后重算校验值，再封装应用 |
| 更新只确认版本、未绑定实际新 PID | 健康确认增加进程身份；真实 Helper 升级通过 |
| 中文字体字重渲染异常 | 固定 400 / 600 字重字体，Fyne 渲染检查通过 |
| RabbitMQ 测试队列参数 / Nacos 异步缓存导致误判 | 使用持久队列；Nacos 只轮询读结果；两组真实服务复测通过 |

## 7. 后续验收与功能缺口

当前可用功能见 [README](../README.md)，完整高级功能清单保留在
[实施计划第 13–15 节](plans/2026-10-01-superlink-implementation-plan.md)。
SQL 高亮 / 元数据补全、可编辑结果与 ChangeSet、事务控制、结构设计、导入、全量
流式导出、备份恢复、迁移同步、完整分页、AI / 云备份仍属于后续工作。

平台和驱动下一步验收应使用实际环境，逐项完成连接测试、对象浏览、基础读写、只读、
取消、断网和资源清理。专有驱动缺少服务或再分发条款时保留“待验证”，不以构建或
mock 通过代替。CI 配置已经提供，但本次尚未运行云端 CI；六种平台组合尚未全部构建。

本次 Alpha 可以从本机 `bin/SuperLink.app` 启动，正式更新签名 / 发布步骤见
[releases.md](releases.md)。本机生成产物和缓存未纳入 Git。

## 8. 连接打开交互补充修复

用户实际使用时停留在欢迎页，原实现中单击只选择连接，双击没有打开动作；
“打开工作台”按钮可以正常创建标签，但没有自动列出数据库或加载对象。
保存配置只意味着本地保存，不能作为已建立数据库连接的反馈。

本次补充：

- 连接列表明确提示“单击选择 · 双击打开”，双击会先选择实际行再打开。
- 重复打开同一连接聚焦已有工作台；标签栏 `+` 仍创建独立查询。
- 主动打开时开始加载对象；SQL / Mongo / Nacos 未填写范围时先读取可选范围，
  单一范围自动选择，多个范围显示选择对话框；没有可访问范围或连接失败会显示原因。
- 搜索 / 重排保留连接 ID，筛选隐藏已选行时清除选择，避免高亮行与实际操作对象不同。
- 链式“范围 → 对象”加载在无显示驱动中暴露了取消句柄的登记竞争，
  改为在启动 worker 前登记句柄，避免覆盖后续请求的取消入口。

验证命令：`go test ./...`、`go test -race -count=1 ./internal/ui`、`go vet ./...`，
均通过。原生修复包用隔离工作区进行启动 / Helper 更新复测，同样通过。
本次没有替用户执行业务 SQL；已有会话凭据保留在正在运行的旧进程中，
构建产物的交互改动在下次启动时生效。
