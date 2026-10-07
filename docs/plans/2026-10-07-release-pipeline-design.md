# SuperLink release pipeline

The release should present a clear download table and categorized Chinese change
notes, with native installers for macOS, Windows and Linux on AMD64 and ARM64.
Keep the existing ZIP-based application updater and offline SQLite behavior.

Use one reusable native build workflow for manual builds and version-tag releases.
Resolve release tags to immutable SHAs, test every target on its native runner,
build/probe supported driver agents, package installers and verify extracted
contents against the update ZIP. Unsupported Windows ARM64 DuckDB is excluded
explicitly; do not replace it with an incorrectly labelled AMD64 binary.

Keep distribution downloads in separate metadata from the client update manifest.
Aggregate jobs require all six platforms, all supported agents and all installer
formats. Validate platform/version/public-key identity, sizes/hashes, archive
paths and the embedded SQLite checksum. Sign the existing manifest only with a
key matching the compiled application key. Reject accidental extra upload files.

Create a draft by default. Verify GitHub's uploaded asset SHA256 digests before
optional explicit publication. Existing releases are immutable to this workflow.
Keep private keys in GitHub Secrets, never artifacts. OS code-signing certificates
and Apple notarization are separate maintainer configuration; unsigned installers
must be described honestly in the generated notes.

Local verification completed: release contract tests, actionlint, native ARM64
Mac build, DMG integrity/read-only mount/file comparison, application version
probe, real event-loop/offline SQLite/clean shutdown and helper upgrade/rollback
checks. Complete repository selfcheck and cloud matrix results are recorded in
the final validation report. Windows/Linux installer execution and GitHub upload
verification require actual runner results; local fixture tests do not establish
those results.
