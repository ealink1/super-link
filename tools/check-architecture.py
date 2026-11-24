#!/usr/bin/env python3
"""Check Rust boundaries, source budgets and the absence of a Go toolchain dependency."""
from pathlib import Path
import ast
import re

ROOT = Path(__file__).resolve().parent.parent
errors = []
for path in ROOT.iterdir():
    if path.is_file() and (path.suffix == '.go' or path.name in {'go.mod', 'go.sum', 'go.work', 'go.work.sum'}):
        errors.append(f'{path.name}: retired Go source/module')
# Check tracked/source roots, not Cargo caches or Git's historical objects.
for folder in ['rust/apps', 'rust/crates', 'rust/vendor', 'internal', 'third_party', 'tools', 'cmd']:
    for path in (ROOT / folder).rglob('*'):
        if path.is_file() and (path.suffix == '.go' or path.name in {'go.mod', 'go.sum', 'go.work', 'go.work.sum'}):
            errors.append(f'{path.relative_to(ROOT)}: retired Go source/module')
for folder in ['rust/apps', 'rust/crates']:
    for path in (ROOT / folder).rglob('*.rs'):
        source = path.read_text()
        production = source.split('#[cfg(test)]', 1)[0]
        if path.name != 'tests.rs' and len(production.splitlines()) > 800:
            errors.append(f'{path.relative_to(ROOT)}: exceeds 800 authored production lines')
for name in ['superlink-app', 'superlink-domain']:
    manifest = (ROOT / 'rust/crates' / name / 'Cargo.toml').read_text()
    if re.search(r'(?m)^\s*(gpui|superlink-ui|fyne|wails)\s*=', manifest):
        errors.append(f'{name}: UI dependency crosses the application boundary')
for path in ROOT.glob('tools/*.py'):
    ast.parse(path.read_text(), filename=str(path))
for path in [ROOT / '.github/workflows/ci.yml', ROOT / 'tools/build.py', ROOT / 'tools/generate-notices.py']:
    if re.search(r'setup-go|GOPROXY|gofmt|[\"\']go[\"\']\s*,|(?<![A-Za-z])go (test|build|run|vet|env|list)\b', path.read_text()):
        errors.append(f'{path.relative_to(ROOT)}: retired Go build dependency')
if (ROOT / 'internal/upstream/jvm').exists():
    errors.append('JVM connector must not be present')
if errors:
    raise SystemExit('\n'.join(errors))
print('Architecture checks passed: Rust app/drivers/tools, zero Go sources/modules, bounded authored files, UI boundaries.')
