# SuperLink · Rust / GPUI

Rust + GPUI 桌面应用。37 类数据源使用 Rust 驱动进程，源码与构建不依赖 Go。

## 结构

```
crates/superlink-domain   纯领域模型（无 I/O、无 UI）
crates/superlink-app      应用服务（凭据分离、修订冲突）
crates/superlink-infra    SQLite 元数据、AES-256-GCM 凭据、驱动 IPC
crates/superlink-ui       GPUI 主窗口与工作区
apps/superlink            桌面可执行程序
apps/update-helper       经用户确认后执行的独立更新助手
apps/driver-agent        37 类数据源的 Rust 隔离宿主
apps/tools               图标与语言目录 ZIP 生成工具
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
- 驱动使用 `json-lines-v3-rust`；旧 Go agent 不兼容，必须重新构建。

## 当前状态

逐项实现与验收记录见仓库根目录的
[功能对齐清单](../docs/plans/2026-11-02-fyne-parity.md)。不再把底层模块存在等同于界面完成。

从仓库根目录运行 `python3 tools/dev_run.py` 可自动构建并重启隔离的开发版，
使用 `bin/dev/data`，保留已安装的正式版本。最低工具链为 Rust 1.89。

驱动构建还需要 C/C++ 工具链（SQLite/DuckDB 原生引擎）及 protoc（Pulsar 协议）。
当前迁移限制和服务联调状态见 [驱动迁移清单](../docs/plans/2026-11-02-pure-rust-drivers.md)。
