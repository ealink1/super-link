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
`docs/fyne-source.json` records all 2,145 original file hashes plus the 15 reviewed
files, and `docs/fyne-memory.patch` is their exact diff. The memory work
shares parsed fonts by content (a 32-entry bounded cache), bounds ephemeral font
scope maps to 4,096 stores, and loads backup/locale fonts only for missing glyphs.
Theme scopes share small display state instead of retaining their container.
Destroying a theme renderer removes its retired child scopes while preserving
independent nested themes. Those memory changes cover four production files and three
regression test files; the complete pinned copy contains 2,153 files.
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

### macOS notebook input-method candidate position

The application-owned `internal/ui/note_ime_darwin.*` bridge supplies the rich editor caret rectangle to GLFWContentView while it has focus. The pinned GLFW Cocoa implementation of `firstRectForCharacterRange:actualRange:` returns the view frame origin, which leaves Chinese input-method candidates at the screen origin. The bridge converts Fyne canvas coordinates through AppKit view/window/screen coordinates, invalidates cached input coordinates and delegates to the original method for other focused controls. GLFW sources remain unchanged; the common Fyne caret API addition is described below.

The integration now covers all focused inputs, including inherited Entry controls, dialogs, SQL/grid/AI editors and terminal surfaces. The reviewed Fyne addition `widget/entry_ime.go` exposes only the rendered caret anchor, offset and size, including scroll hierarchy and blinking-independent positioning. Its hash and addition are recorded in the pinned Fyne manifest and patch. A window-scoped, coalesced UI update tracks focus without reading input values and stops on close.

### Host collection row theme scope (2026-10-09)

The reviewed Fyne List adaptation applies the list theme scope to virtual row wrappers, including pooled rows. Previously only row content inherited that scope, leaving the wrapper selection and hover background in the application theme. This permits the host grid to suppress row-wide highlights while retaining independent card and button feedback. The application regression test checks card hover, row selection/focus, empty slots and recycled card state. The pinned manifest and exact patch include `widget/list.go`.

### Synthetic italic fallback rendering (2026-10-09)

The reviewed Fyne painter now shears rasterized upright fallback faces when italic text is requested, including CJK normal and bold fonts. Real italic faces retain their original outlines. The temporary surface is scoped to one draw and bounded by the destination image; shaping advances and caret geometry remain unchanged. Pixel regressions cover Chinese regular/bold fallback, unchanged advances, and avoiding double slant on real italic faces. The manifest and exact patch include the painter changes and tests.

### Day/night switch cache retention (2026-10-10)

A day/night switch rebuilt every font and icon because the settings listener emptied the painter caches unconditionally, and each `ThemeOverride` refresh repeated that wipe once per contained widget. The reviewed painter now records the active theme's font resources and scale, so a switch that changes colours alone reuses the resolved faces; `ClearFontCache` also forgets that record, because any other component may have emptied the caches in between. `cache.overrideWidget` no longer clears the shared SVG and metric caches per child widget, since each override resolves through its own scope identifier. `svg.Colorize` memoises its output by source content and resolved colour, so repeated icon recolouring skips the XML parse and marshal. Measured text metrics and rasterised SVGs are still dropped on every change: their cache keys carry neither the resolved font nor the colour value, which `TestAccordion_ChangeTheme` in the pinned widget suite detects when the mapping is wrong. `internal/painter/theme_caches_test.go` covers the three retained-face rules directly. Measured on the SQL workbench with three open query tabs (1,147 cached objects), the synchronous theme apply fell from 105 ms to 43 ms, Shell from 38 ms to 14 ms and Note from 32 ms to 6 ms. The manifest and exact patch include `internal/painter/font.go`, `internal/cache/theme.go`, `internal/svg/svg.go`, `internal/driver/glfw/loop.go`, `test/app.go` and the new painter test.
