# 全部数据源纯 Rust 迁移

用户明确要求全部 37 个数据源迁移，替代之前保留 Go agent 的决策。

## 验收条件

- 当前源码树没有 Go 源码和模块；构建、CI、打包和运行不调用 Go。
- 37 个入口均使用 Rust 实现。保留数据源能力与连接安全设置，不用空壳入口代替实现。
- 保留 JSON-lines 隔离边界，逐步复用已有 Rust 原生实现；可取消请求、有限帧、有限结果、固定事务会话和凭据保护继续生效。
- 保留上游许可证、资源及历史归属，历史发布记录不修改。
- 本地验证优先自建测试服务、协议模拟和 SQLite；真实服务未联调的项目明确标记。
- 不使用浏览器，不修改安装版，不提交或推送；验证后重建和重启仓库开发包。

## 架构选择

1. 采用 Rust 驱动进程：沿用已验证的隔离、取消和安装协议；驱动内部复用原生 Rust 实现，避免改动 UI 和应用层边界。
2. 不采用 Go 二进制包装：不能满足纯 Rust 的源码、构建及运行要求。
3. 暂不把全部驱动直接链接到 UI：SDK 故障和内存成本不应影响桌面主进程，保留独立入口便于发布与诊断。

现有 SQLite/MySQL/PostgreSQL/ClickHouse/Elasticsearch 是迁移起点；MySQL 和 PostgreSQL 协议兼容数据源需要逐项核对认证与方言。Oracle、达梦、IRIS/Caché、消息及向量协议独立适配，不能仅更名映射。

## 实施顺序

1. Rust agent 的协议、会话、参数、超时、预算、目录和流式请求；替换 SQLite 构建与安装回归。
2. SQL 协议兼容及独立 SQL 驱动、Redis/MongoDB、HTTP 数据源。
3. 向量、配置、消息服务与代理/SSH/TLS 路径。
4. 全部入口与回归覆盖完成后移除 Go 源码、模块、工具和 CI 依赖；修订溯源和开发文档。
5. 无 Go 环境构建、37 入口探测、端到端回归与本地包校验，自动重启开发包。

## 当前落地

- 已移除 514 个 Go 源码及模块文件；Go 专用导入器、大小检查、图标和 ZIP 生成命令退场。图标和语言 ZIP 生成由 `superlink-tools` 提供。
- 37 个入口均由 `superlink-driver-agent` 编译的 Rust 可执行文件提供，并报告 `implementation=rust`、`json-lines-v3-rust`。旧 Go 协议不能被当前注册器误用。
- 开发目录 37 个二进制已替换为 Rust。旧本地构建产物移出仓库原样保留；未修改已发布记录和安装版。
- CI、构建、许可证清单与上游校验已改为 Cargo；上游资源、MIT/Apache/OFL 署名保留。SQLite/DuckDB 仍使用官方原生引擎，项目及驱动源码不含 Go。

| 数据源 | Rust 路径 |
| --- | --- |
| MySQL、GoldenDB、MariaDB、DiRoS、StarRocks、Sphinx、OceanBase MySQL | MySQL 二进制参数/文本协议；复用原生连接器 |
| PostgreSQL、Kingbase、Vastbase | PostgreSQL 协议、TLS、类型化参数与游标 |
| openGauss、GaussDB、HighGo | openGauss Rust 协议；SHA256/HighGo SM3 认证适配 |
| Oracle、OceanBase Oracle | Oracle Thin Rust 连接器 |
| SQL Server、达梦 | Tiberius / 达梦 Rust 协议；达梦帧、LOB 和读取预算补丁 |
| SQLite、DuckDB | Rust 连接器与各自原生引擎 |
| IRIS、Caché | Rust SuperServer/列表帧编解码 |
| IoTDB、ClickHouse | Rust Thrift / 有界 HTTP SQL |
| MongoDB、Redis | Rust 官方 MongoDB 客户端 / Redis、Sentinel、Cluster 客户端 |
| Elasticsearch、Trino、TDengine | 有界、可取消的 Rust HTTP 请求与结果转换 |
| Qdrant、Chroma、Milvus、Nacos、RabbitMQ | Rust REST 协议与命令适配；Chroma v1/v2 探测 |
| Kafka、MQTT、Pulsar、RocketMQ | Rust 客户端或 RocketMQ Remoting 实现；预览不提交消费位点 |
| 自定义连接 | 显式映射到所选 Rust SQL 协议，保持 `custom` 进程身份 |

公共层保留有界 JSON-lines、精确 int64/decimal 文本、二进制、NULL、事务会话、请求截止与进程隔离。SQLite/MySQL/PostgreSQL 导出直接按游标分块。SSH 使用已有 Rust SSH 主机指纹校验、有限转发连接和显式释放；SOCKS5/HTTP CONNECT 不绕过启用的路由。MySQL 脚本式 HTTP 隧道也已移植，沿用每请求独立连接、拒绝跨请求事务的语义。

## 验证与边界

最新全工作区回归为 **470 通过、0 失败、2 忽略**（真实 SSH 测试需外部服务），包括脚本 HTTP 隧道真实本地往返、截止取消、40 MiB 游标导出、DuckDB 主键/三段路径/带点表名、Oracle 服务名与 schema 分离、消息默认主题保护、IRIS Unicode 帧和达梦绑定扫描。全工作区 Clippy 严格检查通过。禁用 Go/gofmt/gccgo 的发布构建已完成 37 个驱动与原生应用包；当前本地包已完成 ad-hoc 签名校验、已签名 SQLite checksum/实际 IPC 和 ZIP CRC 检查；ZIP 58.1 MiB。Rust 资源工具的六语言逐字节比对、确定性 ZIP 和七档 ICNS/PNG 也已验证。开发版已自动构建并重启，PID 88463；旧开发进程及 PTY 已释放，安装版未修改。

37 入口身份探测不能替代 37 个真实服务的联调。SQLite、DuckDB 已运行实际引擎；HTTP、SOCKS5、Navicat、IRIS/RocketMQ 编解码等有本地模拟或边界回归。其他远程数据库认证、服务版本和集群行为尚未在真实环境验收。

仍有功能边界，不能宣称完整功能对齐验收完成：

- SSH、代理与隧道叠加，以及动态集群节点转发，未接通的组合明确拒绝。
- 一部分 SQL 协议仍通过有界结果导出，超过预算返回失败；尚未全部实现无限量游标导出。
- SQL Server/Pulsar 的独立客户端证书配置，以及 IoTDB TLS 尚未接通；Oracle Thin 使用 wallet，MongoDB 客户端证书使用 URI 的合并 PEM 文件选项。
- 消息系统、向量服务和独立 SQL 方言的高级命令/元数据仍需对照原实现与真实服务逐项补验。当前保留全部数据源入口和实际协议实现，没有用 Go 回退。

