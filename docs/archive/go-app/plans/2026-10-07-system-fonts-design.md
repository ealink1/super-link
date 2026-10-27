# System desktop fonts

SuperLink now reads installed desktop fonts once at startup instead of embedding
NaviUI Regular/Bold and NaviMono. Theme callbacks return cached resources.

macOS uses installed Arial and Courier New; Windows uses Segoe UI and Consolas
with Arial/Courier fallbacks; Linux tries installed DejaVu, Liberation and Noto.
Regular, bold, monospace and bold monospace styles are selected separately.
Files are closed after reading, capped at 32 MiB and parsed before selection.
Missing or invalid fonts use Fyne's small built-in fallback. Chinese and other
missing glyphs follow the existing Fyne system font fallback; display coverage
therefore depends on the host's installed fonts. Archived font sources remain in
the repository for provenance and are not embedded or copied into application
packages. Historical font coverage and outline audit tests are replaced by
system file selection, invalid-file fallback, style and monospace tests.

Verification: native macOS build/package, UI tests, static checks, installed
Chinese fallback coverage and artifact checks. Windows and Linux execution has
not been verified on a host running those operating systems.

Results: full UI tests and vet passed; architecture/source size and asset
provenance checks passed. All 22 driver metadata probes and native macOS ARM64
packaging passed; all 23 artifact hashes/sizes and ZIP integrity verified.
Main executable is 73.2 MiB (previously 105.0); ZIP is 45.8 MiB (previously 71.3);
uncompressed bundle is 126.8 MiB. Credential checkbox interaction tests now click
the actual vertical center, since system font metrics leave padding above it.
