# Rust + GPUI 重写

日期：2026-10-27。分支：rewrite/rust-gpui。

## 目标

用 Rust + GPUI 原生重写 SuperLink 桌面端。Go 版本保留在 main 分支，
Rust 版本独立放在 rust/ 工作区，两者不共享编译产物。

驱动层沿用现有 Go driver-agent 进程：它已经通过 stdin/stdout 的 JSON-lines
协议隔离，Rust 侧只实现客户端。这样 36 类数据源与已验收的兼容性矩阵不必重做，
重写范围集中在 UI、交互与应用服务。

## 分层映射

| Go | Rust | 约束 |
|---|---|---|
| internal/domain | crates/superlink-domain | 纯模型，无 I/O、无 UI |
| internal/application | crates/superlink-app | 用例服务，不导入 gpui |
| internal/infra | crates/superlink-infra | SQLite、凭据、驱动 IPC |
| internal/ui | crates/superlink-ui | 仅渲染与输入 |
| cmd/superlink | apps/superlink | 组装与启动 |
| internal/upstream/db + cmd/driver-agent | 保持 Go | 独立进程，跨语言协议 |

## 兼容性契约

重写不得破坏用户已有数据：

1. SQLite 元数据：表结构、列名和 user_version 迁移与 Go 版本完全一致
   （profiles / drafts / history / settings / saved_queries，版本 2）。
2. 凭据文件：credentials/master.key 为 32 字节密钥；每个 bundle 为
   NFCR\x01 + 12 字节随机 nonce + AES-256-GCM 密文，附加数据为凭据引用；
   目录 0700、文件 0600。
3. Profile JSON：字段名沿用 Go 的 json tag；未建模的配置项通过 flatten map
   原样保留，新版本仍可写回。
4. 驱动协议：一行一个 JSON 对象，id 关联，成功时读取 data 字段。

## 已完成（M1）

- 领域层全量移植：数据源目录（36 项与别名）、连接配置与可见性过滤、
  结果集/草稿/历史/保存查询、表结构与结构哈希、笔记与笔记本校验、SSH 主机校验。
- 基础设施层：SQLite 元数据存储（含迁移与乐观并发）、AES-256-GCM 凭据保险库、
  driver-agent JSON-lines 客户端。
- 应用层：ProfileService（凭据与元数据分离）、DraftService、QueryLibrary。
- UI 层：GPUI 主窗口、SQL / Shell / Note 三个工作区切换、连接侧栏、状态栏。
- 测试：cargo test 覆盖可见性、校验边界、加密往返与篡改检测、修订冲突。

## 后续阶段

1. 驱动接入：agent 发现、架构/哈希校验、connect/query/close 方法绑定，
   把结果集映射到 domain::ResultSet。
2. SQL 工作区：代码编辑器（GPUI EntityInputHandler）、语法高亮、补全、
   虚拟化结果表格、分页与写回。
3. 终端工作区：russh + portable-pty + alacritty_terminal，SSH 指纹确认、
   SFTP、Linux 监控。
4. 笔记工作区：Markdown 编辑与预览、分组、全文搜索、回收站、加密快照。
5. 桌面集成：托盘、后台驻留、多窗口、系统菜单、自动更新（Ed25519 校验）。
6. 打包与发布：六平台构建、签名/公证、与现有 release 流水线对接。

## 验证

```sh
cd rust
cargo test
cargo build -p superlink
```

## 环境限制

本机只有 Command Line Tools，没有完整 Xcode，`xcrun metal` 不可用，
GPUI 默认在构建期编译 Metal 着色器会失败。当前启用 gpui 的
`runtime_shaders` 特性，改为运行期编译；安装 Xcode 后可以关闭该特性。

## 已知取舍

- GPUI 仍为 pre-1.0，升级需要同步调整 API；当前锁定 0.2.2。
- 驱动层保留 Go，意味着发布包同时包含 Rust 主程序与 Go agent。
- 中文排版、输入法与无障碍需要在真实桌面环境中逐项验收，不能用编译通过代替。
