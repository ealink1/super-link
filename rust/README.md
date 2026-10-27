# SuperLink · Rust / GPUI

Go 版本的重写分支产物。目录结构与 Go 分层一一对应，驱动进程继续复用 Go agent。

## 结构

```
crates/superlink-domain   纯领域模型（无 I/O、无 UI）
crates/superlink-app      应用服务（凭据分离、修订冲突）
crates/superlink-infra    SQLite 元数据、AES-256-GCM 凭据、驱动 IPC
crates/superlink-ui       GPUI 主窗口与工作区
apps/superlink            可执行程序
```

## 构建与运行

```sh
cd rust
cargo test
cargo run -p superlink
```

默认工作区为 macOS 的 `~/Library/Application Support/SuperLink`，
可用 `SUPERLINK_DATA_ROOT` 指向独立目录，避免与 Go 版本互相影响。

## 环境要求

- Rust stable（构建时使用 1.98.1）。
- **仅安装 Command Line Tools 时，GPUI 的 Metal 着色器无法在构建期编译**
  （`xcrun metal` 属于完整 Xcode）。因此在根 `Cargo.toml` 中启用了
  gpui 的 `runtime_shaders` 特性，改为运行期编译着色器。
- 若要使用构建期着色器，安装 Xcode 后把该特性关掉即可。

## 兼容性

- SQLite 表结构与 `user_version` 迁移与 Go 版本一致。
- 凭据文件格式一致：`NFCR\x01` + 12 字节随机 nonce + AES-256-GCM，
  附加数据为凭据引用，目录 0700 / 文件 0600。
- 驱动 agent 使用同一套 JSON-lines 协议。

## 当前状态

已完成领域层、基础设施层、应用层与 GPUI 外壳，32 个测试通过。
驱动接入、SQL 编辑器与结果表格、终端、笔记编辑器、更新器仍在后续阶段。

详见 `docs/plans/2026-10-27-rust-gpui-rewrite.md`。
