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

## 已完成（M1 + M2）

- 领域层全量移植：数据源目录（36 项与别名）、连接配置与可见性过滤、
  结果集/草稿/历史/保存查询、表结构与结构哈希、笔记与笔记本校验、SSH 主机校验。
- 基础设施层：SQLite 元数据存储（含迁移与乐观并发）、AES-256-GCM 凭据保险库、
  driver-agent JSON-lines 客户端（含完整响应帧）。
- 应用层：ProfileService（凭据与元数据分离）、DraftService、QueryLibrary、
  NotebookService（加密快照 + 修订冲突）、QueryService（真实执行）。
- 驱动层（M2）：bundle.json 校验、SHA256、Mach-O/ELF/PE 架构检查、
  metadata 身份核对、connect/query/close，行预算与列序映射。
  已用仓库自带的 sqlite agent 跑通端到端查询。
- UI 层（M2）：GPUI 主窗口、SQL / Shell / Note 工作区切换；
  SQL 工作区含分组连接树（可折叠）、查询标签条（草稿随标签保存）、
  工具栏（连接/数据库/行数上限/运行）、支持中文输入法的编辑器、
  带行号与表头的分页结果表；Shell 工作区含主机列表与终端；
  Note 工作区读取加密笔记本并预览。
  Shell 工作区改为卡片网格：搜索框、主机卡片（图标/名称/地址/备注/快速连接），
  侧栏仅在 SQL 工作区显示，与 Go 版布局对齐。
  编辑器支持文本变更回调，供搜索框实时过滤连接与主机。
- 外观：复用 Go 版已审阅的 SVG 图标（123 个），新增 AssetSource；
  主题支持 自动 / 浅色 / 深色 三态切换，写入 settings 表并在重启后恢复。
  工具栏与按钮已接图标：数据源专属图标（21 类）、新建查询、保存、运行、
  本地终端、连接、断开、重连、刷新、折叠箭头、关闭标签。
- 执行历史：按连接记录执行结果，字面量与注释写入前脱敏，右侧面板可查看与清空。
- 设置页：外观三态与结果行数上限，写入 settings 表；行数上限在执行时生效。
- 测试：cargo test 覆盖可见性、校验边界、加密往返与篡改检测、修订冲突、
  驱动校验与真实 SQLite 查询、查询服务与笔记本快照。

## 后续阶段

1. SQL 工作区完善：多行编辑器、语法高亮、补全弹窗、表目录浏览、分页与写回、结构设计器。
2. 终端工作区完善：已接通本地 PTY 与 SSH（主机密钥确认、VT 屏幕、滚动缓冲、
   输入法组合、断线自动重连），读写已分离（SessionHandle），仍缺窗口尺寸联动、
   SFTP 面板、监控面板与主机编辑表单。

   重连策略：仅对远程主机生效，退避 1/2/4/8/15 秒、最多 5 次；手动「断开」或
   本地 shell 退出不重连；任何新的连接尝试都会作废还在倒计时的旧重试。
3. 笔记工作区完善：Markdown 编辑、分组、全文搜索、回收站。
4. 桌面集成：托盘、后台驻留、多窗口、系统菜单、更新网络客户端。
5. 打包与发布：六平台构建、签名/公证、与现有 release 流水线对接。

## crate 进度

- superlink-domain：完成，17 个测试
- superlink-infra：完成，12 个测试
- superlink-app：完成（含查询与加密笔记本），13 个测试
- superlink-drivers：完成（bundle/架构/哈希/会话），9 个测试
- superlink-sql：完成（词法/补全/DDL/变更/分页），40 个测试
- superlink-files：完成（导入导出/SQL 文档），39 个测试
- superlink-update：完成（验签/校验/解压/回滚），74 个测试
- superlink-terminal：完成（SSH/SFTP/PTY/监控），63 个测试
- superlink-ui：SQL、Shell（终端）、Note 均已接通；12 个测试（含真实本地 PTY）

## 验证

```sh
cd rust
cargo test
cargo build -p superlink
```

## 实机验证：中文输入法（2026-10-27）

环境：macOS arm64，拼音输入法 com.apple.inputmethod.SCIM.ITABC。

方法：把可执行文件临时打包为开发版 .app，用 CGWindowList 定位窗口，
CGEvent 点击 Shell 标签与「本地终端」，随后在 ABC 与拼音之间切换：

1. ABC 下输入 `printf '`
2. 拼音下用物理按键输入 zhongwen，空格上屏
3. ABC 下输入 `' > /tmp/ime-real.txt`，回车

结果：文件内容为 6 字节 `e4b8ad e69687`，即「中文」，无重复字符、无拼音串
残留，说明组合过程没有泄漏到 PTY，提交只发生一次。整个注入过程在每次按键前
校验前台必须是 SuperLink，否则立即中止。

## 环境限制

本机只有 Command Line Tools，没有完整 Xcode，`xcrun metal` 不可用，
GPUI 默认在构建期编译 Metal 着色器会失败。当前启用 gpui 的
`runtime_shaders` 特性，改为运行期编译；安装 Xcode 后可以关闭该特性。

## 已知取舍

- GPUI 仍为 pre-1.0，升级需要同步调整 API；当前锁定 0.2.2。
- 驱动层保留 Go，意味着发布包同时包含 Rust 主程序与 Go agent。
- 中文排版、输入法与无障碍需要在真实桌面环境中逐项验收，不能用编译通过代替。
