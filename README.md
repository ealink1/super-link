# SuperLink · rewrite/rust-gpui

本分支使用 **Rust + GPUI**，应用、驱动进程、SSH/SFTP、更新助手与资源生成工具均为 Rust。
37 个数据源入口已切换到 Rust 驱动宿主；源码、构建和运行链中不再依赖 Go。
原 Go/Fyne 应用通过 `main` 分支和 Git 历史追溯，历史署名及许可证保留。
SQLite 和 DuckDB 通过 Rust crate 使用其原生数据库引擎。

- 设计与进度：[docs/plans/2026-10-27-rust-gpui-rewrite.md](docs/plans/2026-10-27-rust-gpui-rewrite.md)
- Rust 结构与运行方式：[rust/README.md](rust/README.md)
- 驱动层溯源：[UPSTREAM.md](UPSTREAM.md)

## 文档结构

```
docs/
  plans/        Rust 重写与 Fyne 功能对齐计划
  archive/
    go-app/     main 分支 Go 版的历史设计、QA 与截图（已归档）
  rust-upstream-adaptations.patch   Rust 驱动依赖适配补丁
  upstream-import.json         上游导入清单（同上）
```

Go 版历史文档仅用于追溯，不描述当前运行时。

## 快速开始

```sh
cargo test --manifest-path rust/Cargo.toml --workspace
python3 tools/build.py --skip-app              # SQLite agent
python3 tools/build.py --all-drivers --skip-app # 全部 37 个代理入口
python3 tools/dev_run.py                       # 共用本地工作区，构建并重启
python3 tools/dev_run.py --isolated-data       # 可选：使用 bin/dev/data 独立测试
```

默认工作区为 `~/Library/Application Support/SuperLink`（可用
`SUPERLINK_DATA_ROOT` 指定其他目录）。已有 `state.sqlite` 时直接共用原版数据库
及同目录的加密凭据；没有时使用 `state.db`。同一工作区一次只能由一个应用打开，
切换版本前请保存编辑并正常退出当前应用。

## 状态

纯 Rust 迁移及当前限制见 [驱动迁移清单](docs/plans/2026-11-02-pure-rust-drivers.md)。
当前界面实现及逐项验收见 [Fyne 对齐清单](docs/plans/2026-11-02-fyne-parity.md)。原生操作结果及未验收边界见 [design-qa.md](design-qa.md)。
SQL 执行、表编辑、文件导入导出、笔记编辑、多终端、SFTP、监控、AI 与签名更新入口
已接通；远程数据源、SSH 与正式更新安装仍需在对应环境验收，编译通过不等于实机通过。

开发启动只维护 `bin/dev/SuperLink.app` 及其进程，默认共用上述本地工作区，不替换
`/Applications/SuperLink.app`。发布构建使用 `python3 tools/build.py --package`，
签名更新校验公钥由构建环境 `SUPERLINK_RELEASE_PUBLIC_KEY` 注入；未配置时禁止自动更新。
