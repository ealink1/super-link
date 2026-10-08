# SuperLink development

- Product names, release titles and application download filenames use the exact brand `SuperLink`. Preserve upstream attribution and existing immutable release records.

- Implement the accepted full feature and visual parity design in `docs/plans/2026-10-01-gonavi-parity-design.md`; this supersedes the original Alpha scope. JVM management remains excluded.
- Do not use a browser for testing unless the user explicitly requests browser tests.
- Native Go/Fyne implementation; application and domain packages must not import Fyne or Wails.
- New authored production files: target 400 lines, maximum 800. Keep functions small and scoped.
- `internal/upstream` preserves reviewed upstream code and tests; record changes in `UPSTREAM.md`.
- Pass request contexts to I/O, bound result memory, and release sessions, subscriptions and processes.
- Never persist plaintext credentials or log passwords, DSNs, tokens, or query results.
- Keep UI updates on the Fyne goroutine. Do not perform I/O in rendering or event callbacks.
- Verify changes with meaningful tests, static checks, and native builds; report unverified environments honestly.
- Do not commit or push unless the user requests it.
