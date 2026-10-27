# SuperLink 完整命名统一

日期：2026-10-03。此记录取代第一阶段仅更改界面显示名的约定。

| 项目 | 当前值 |
| --- | --- |
| 产品、窗口、应用包 | SuperLink、`SuperLink.app` |
| GitHub 仓库 / Go module | `github.com/ealink1/super-link` |
| Go 主命令 | `./cmd/superlink` |
| 本机可执行文件 | `bin/superlink`，Windows 为 `superlink.exe` |
| 应用 ID | `io.github.ealink1.superlink` |
| 发布资产 ID | `superlink` |
| ZIP | `superlink_<version>_<os>_<arch>.zip` |
| 包元数据 | `superlink.package.json` |
| 环境变量前缀 | `SUPERLINK_` |
| 新工作区默认目录 | `os.UserConfigDir()/SuperLink` |

构建脚本、CI、更新 API、下载地址白名单、签名工具、原生烟测、README 和历史文档中的项目称谓同步更新。本地 Git origin 指向新仓库。

## 数据目录与升级

按用户要求，本次不提供旧数据兼容。目录优先级为显式 `--data-root`、`SUPERLINK_DATA_ROOT`、`os.UserConfigDir()/SuperLink`。不识别旧环境变量，不自动读取、复制、合并、移动或删除旧数据目录。现有数据保留原状，新版使用 SuperLink 工作区。

旧发布包使用不同的资产 ID 和包元数据，升级到本次命名版本应手动替换应用包；新版本之间继续支持原有签名整包更新。其他脚本及 CI 配置应使用 `SUPERLINK_` 变量，包含 `SUPERLINK_RELEASE_PUBLIC_KEY`、`SUPERLINK_RELEASE_PRIVATE_KEY`、`SUPERLINK_MAC_SIGN_IDENTITY`。本次只改本地配置，不修改远端仓库变量或发布 Release。

本地检出目录仍是当前工作区路径；目录名不参与 module、应用 ID 或构建结果。SuperLink 的来源、许可证、驱动构建标签和协议修订标识保留原样。历史截图、产物哈希和原生验收结论仍属于各自记录日期，不作为本次构建证据。

## 验证

- `make selfcheck` 全部通过：架构、源码体积、上游 / Fyne / UI 资源来源、全量单元测试、全部驱动测试、竞态检查、`go vet` 与 Fyne 专项测试。
- macOS arm64 主程序、Helper、22 个驱动及 `SuperLink.app` / ZIP 构建通过。
- 已核对应用 ID、可执行文件名、包元数据、23 个发布资产的长度 / SHA256 / 仓库 URL，以及 ZIP CRC 和包内外字节一致性。
- `python3 tools/native-smoke.py --upgrade` 通过：隔离新工作区启动、离线 SQLite、实例独占、健康握手、0.1.0 → 0.1.1 整包替换与退出。测试不访问用户旧数据。
- 源码 / 文档 / 文件名旧项目命名扫描、Python 语法、文档相对链接和 `git diff --check` 通过。压缩语言包已重建，上游改动清单已同步。
- 未使用浏览器。Windows / Linux 未进行原生运行验收；未发布 Release。
