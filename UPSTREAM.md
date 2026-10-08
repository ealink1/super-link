# Upstream provenance

SuperLink is an independent Go module, desktop application and repository.
The Fyne UI, application services, credential store, metadata storage, driver
installer and signed whole-application updater are implemented in this project.
The reviewed driver/protocol dependency closure derives from
[Syngnat/GoNavi](https://github.com/Syngnat/GoNavi) at commit
`6e20b6ddf56b2ae76f7e5d5c6a3505cef1edc871` (Apache-2.0).
The Wails application object, React runtime and original application data are
not imported or shared. Reviewed frontend icon assets are retained separately
for the user-requested visual parity, with their original provenance.

## Traceability

`docs/upstream-import.json` records the original source path/hash, the initial
rewritten hash and the current retained hash for 523 files, including driver
entry points, source/test fixtures, catalog generation and vendored libraries.
`tools/verify-upstream.py` verifies those current hashes without network access.
`docs/upstream-adaptations.patch` records text adaptations against the rewritten,
`gofmt`-normalized source. The generated catalog ZIP is tracked by its hash.

`tools/import-upstream.py` documents the initial import. It reads a local checkout
at the pinned commit, changes module paths, environment variables and application
storage/log names, omits the Wails logger adapter, and refuses to overwrite an
existing retained tree. It is an initial-import tool, not an automatic update
mechanism. Subsequent imports must apply and review the recorded adaptations,
regenerate catalogs, tidy dependencies and pass tests before refreshing hashes:

```sh
python3 tools/verify-upstream.py
# Only after reviewing intentional upstream changes:
python3 tools/verify-upstream.py --refresh /path/to/pinned/upstream
```

## Intentional adaptations

- On 2026-10-03 the project namespace became SuperLink: module imports use
  `github.com/ealink1/super-link`, environment variables use `SUPERLINK_`, and
  app-owned temporary paths and log names use `superlink`. The import rewrite,
  retained source/tests, provenance hashes and adaptation patch were refreshed
  together against the same pinned upstream commit. Original upstream attribution,
  driver build tags and protocol revisions remain unchanged.
- JVM management is excluded by user request. No JVM connector, Java helper,
  JDK requirement, JVM DTO or JVM capability registry entry is retained. Unused
  translation keys in the full upstream catalogs and Java wire-protocol names
  of database services do not create a Java runtime dependency.
- Module imports target `github.com/ealink1/super-link/internal/upstream`.
  App-specific environment variables use `SUPERLINK_`; storage/log/agent names
  are isolated from the upstream application. Existing `superlink_*_driver` build tags and `src-*`
  compatibility revisions now carry the `fyne-values1-` prefix; agents from the
  original baseline must be rebuilt/reinstalled for the adapted value transport.
- The unused Wails logger adapter and its test are omitted. The retained logger
  is independent of Wails. Legacy DTO tests/comments preserve upstream context.
- MongoDB command execution checks server write and write-concern errors,
  decodes cursor batches within the request row/byte/field budgets, and reports
  truncation. The application also makes the first BSON key the actual command.
- SOCKS5 uses its context-aware dialer to cancel unfinished handshakes. App-owned
  proxy listeners/connections have explicit lifetimes rather than cache-only
  cleanup. Unsupported SQL proxy/TLS/URI/multi-host combinations fail explicitly.
- Embedded catalog generation references this project's generator, and the
  compressed catalog is regenerated after namespace transformations.
- Moved Oracle SQL and public TLS-certificate test fixtures use the new relative
  paths. The two upstream TLS private-key fixtures are excluded from Git and the
  provenance inventory. MySQL DSN tests generate their certificates and keys in
  temporary directories. The vendored highgo-pq integration tests require locally
  generated TLS fixtures; its `certs/Makefile` is retained for that purpose.
  All rewritten Go files are formatted with `gofmt`.
- Native SQL scanning preserves BLOB/BINARY/BYTEA values before display conversion.
  JSON-lines v2 uses typed bound arguments and separately indexed binary-cell
  metadata, preserving int64, bytes, dates, booleans and NULL. Old agents without
  the typed-argument capability fail explicitly. Bound queries receive the same
  scanner budgets as queries without parameters. These are reviewed IPC extensions.
- Named parameter conversion retains exact integers/decimals, rejects non-finite
  values and checks all missing names. Dialect lexer corrections preserve PostgreSQL
  E-strings and MySQL hash/dash comment rules. Agent dispatch was split by subject.

## Licenses and assets

`LICENSE` retains the upstream Apache-2.0 text; `NOTICE` records attribution.
`third_party/highgo-pq` and `third_party/go-irisnative` retain their source and
license files. `THIRD_PARTY_NOTICES.md` collects license texts from 164 linked
Go modules and the Go standard library; regenerate it when dependencies change.
The Dameng module download has no root license text, so its redistribution terms
must be reviewed before public driver distribution. Native SDK/system-library
licenses also remain part of distribution review.

The UI's CJK source is Noto Sans SC under SIL OFL 1.1. Original source:
[google/fonts, Noto Sans SC](https://github.com/google/fonts/tree/main/ofl/notosanssc).
`tools/generate-fonts.py` instantiated static weights 400 and 600 with FontTools
4.63.0. `tools/build_fonts.py` composes their Latin glyphs with Fyne's licensed
Inter Regular, Noto Sans Bold and DejaVu Sans Mono Powerline into renamed NaviUI
and NaviMono derivatives. Source fonts and all notices are in `third_party/fonts`;
the application package includes their license texts. No macOS system font is
redistributed; native font rasterization parity remains unverified.

The three complete font resources now use OpenType/CFF outlines. The conversion
keeps all 30,890 mapped codepoints and 31,036 glyph advances in each face; it does
not subset Chinese characters. `internal/ui/assets/fonts.json` pins their bytes,
coverage and reference advances. CFF prevents eager decoding of every TrueType
outline at startup. Repeated CFF instructions are shared using Adobe's
[cffsubr 0.4.0](https://github.com/adobe-type-tools/cffsubr), reducing the three
resources by about 17.5% without changing any glyph. FontTools and go-text each
verified every pre/post compression outline and horizontal advance. Regeneration
uses `tools/font-requirements.txt`; these Python tools are build-time only.

Fyne v2.8.1 is preserved in `third_party/fyne` with its BSD-3-Clause license.
`docs/fyne-source.json` records all 2,145 original file hashes plus the reviewed
seven reviewed production/test files, and `docs/fyne-memory.patch` is its exact diff. The patch
shares parsed fonts by content (a 32-entry bounded cache), bounds ephemeral font
scope maps to 4,096 stores, and loads backup/locale fonts only for missing glyphs.
Theme scopes share small display state instead of retaining their container.
Destroying a theme renderer removes its retired child scopes while preserving
independent nested themes. These changes cover four production files and three
regression test files; the complete pinned copy contains 2,149 files.
Fonts and fallback behavior remain available. `tools/verify-fyne.py` validates
the copy and module replacement; the painter/cache suites and affected container
tests run in selfcheck/CI. See `docs/memory-analysis-2026-10-02.md` and
`docs/memory-refinement-2026-10-02.md` for measurements and known upstream
container snapshot failures on this Mac.

`internal/ui/assets/superlink/sources.json` records hashes and source paths for 63
UI icons, 21 database SVGs and 13 database PNGs. JSX icon geometry was extracted
mechanically; PNG/SVG assets were copied, and ICO pixels converted to PNG.
`tools/verify-ui-assets.py` verifies the complete inventory. Fyne applies the
original frames/scales and brand palette at rendering time, retaining outline
strokes and explicit fills.

Retained upstream files preserve existing structure and size to support review
and upstream comparisons. New production files follow the limits in AGENTS.md.

## SuperLink namespace cleanup (2026-10-08)

Product text, authored documentation, retained driver build tags, tests and
internal protocol keys now use the SuperLink namespace. UI assets live under
`internal/ui/assets/superlink`. Original repository URLs and attribution remain
in provenance and license records. Retained hashes and the adaptation patch
were refreshed against the pinned source. Driver revisions now carry the `superlink-values2-` prefix. Binaries must be rebuilt for
this namespace change; do not reuse older driver builds.
