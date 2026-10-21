# SuperLink development

- Product names, release titles and application download filenames use the exact brand `SuperLink`. Preserve upstream attribution and existing immutable release records.

- Implement the accepted full feature and visual parity design in `docs/plans/2026-10-01-superlink-parity-design.md`; this supersedes the original Alpha scope. JVM management remains excluded.
- Do not use a browser for testing unless the user explicitly requests browser tests.
- 每次修改完成并通过相关验证后，自动重新构建并重启项目，运行最新代码，无需用户再次要求重启。该自动构建只作用仓库内的开发产物与本地进程。
- 不得自行覆盖或替换用户机器上已安装的版本包（如 `/Applications/SuperLink.app`）：不自动执行 `ditto`/`cp` 安装到 `/Applications`、不自动改版本号安装、不自动杀掉已安装应用进程做替换。安装或升级只能由用户自己确认，或由用户主动在应用内点击「检测更新」触发。
- Native Go/Fyne implementation; application and domain packages must not import Fyne or Wails.
- New authored production files: target 400 lines, maximum 800. Keep functions small and scoped.
- `internal/upstream` preserves reviewed upstream code and tests; record changes in `UPSTREAM.md`.
- Pass request contexts to I/O, bound result memory, and release sessions, subscriptions and processes.
- Never persist plaintext credentials or log passwords, DSNs, tokens, or query results.
- Keep UI updates on the Fyne goroutine. Do not perform I/O in rendering or event callbacks.
- Verify changes with meaningful tests, static checks, and native builds; report unverified environments honestly.
- Do not commit or push unless the user requests it.
- Git 提交描述使用中文，保留 `fix:`、`feat:` 等类型前缀，例如：`fix: 将 macOS 工作区选中样式改为绿色下划线`。
