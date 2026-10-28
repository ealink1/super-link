# SuperLink · rewrite/rust-gpui

这个分支正在用 **Rust + GPUI** 重写桌面应用。完整的 Go 应用仍在 `main` 分支维护；
本分支只保留两样东西：

1. **rust/** — Rust + GPUI 应用（正式开发目标）
2. **internal/upstream + cmd/driver-agent** — Go 驱动进程（36 类数据源，Rust 通过 IPC 调用）

其余 `internal/` 下被移除的 Go 目录（ui/infra/application 等）属于 main 分支的
在产应用，随分支收敛会在验收后彻底退场。

- 设计与进度：[docs/plans/2026-10-27-rust-gpui-rewrite.md](docs/plans/2026-10-27-rust-gpui-rewrite.md)
- Rust 结构与运行方式：[rust/README.md](rust/README.md)
- 驱动层溯源：[UPSTREAM.md](UPSTREAM.md)

## 文档结构

```
docs/
  plans/        活跃计划（当前只有 Rust 重写计划）
  archive/
    go-app/     main 分支 Go 版的历史设计、QA 与截图（已归档）
  upstream-adaptations.patch   驱动层上游适配补丁（UPSTREAM.md 引用）
  upstream-import.json         上游导入清单（同上）
```

Go 版历史文档仅在追溯 main 分支行为时查阅，不随 Rust 开发更新。

## 快速开始

```sh
cd rust
cargo test
cargo run -p superlink          # 需要本机 OpenGL/Metal 工具链
python3 tools/build.py --skip-app   # 只构建驱动 agent
```

默认工作区为 `~/Library/Application Support/SuperLink`（可用
`SUPERLINK_DATA_ROOT` 指向独立目录，避免与 Go 版互相影响）。

## 状态

- 后端：驱动校验/会话、AES 凭据、SQLite 元数据、SQL 词法与补全、导入导出、
  更新验签、SSH/SFTP/PTY 已全部就位并有测试
- 驱动移植：SQLite（`SUPERLINK_NATIVE_SQLITE=1`）、PostgreSQL（`SUPERLINK_NATIVE_PG=1`，
  含 TLS 与客户端证书）、MySQL 协议族（`SUPERLINK_NATIVE_MYSQL=1`，覆盖
  MySQL/MariaDB/OceanBase/TiDB、Kingbase/Vastbase、ClickHouse）已有进程内实现；
  SQLite 与 agent 结果逐值一致，其余数据源继续走 agent
  （GaussDB/OpenGauss/HighGo 因 sha256/SM3 认证保留 agent）
- UI：SQL（编辑/运行/结果/历史/新建连接表单）、Shell（主机卡片/终端）、Note（预览）
- 尚未对齐：多行编辑器、表目录、SFTP/监控面板、Note 编辑、发布打包
