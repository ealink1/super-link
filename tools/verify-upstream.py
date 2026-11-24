#!/usr/bin/env python3
"""Verify retained non-Go assets and reviewed Rust forks without network/toolchain access."""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parent.parent
manifest = json.loads((ROOT / 'docs/upstream-import.json').read_text())
errors = []
expected = {record['target'] for record in manifest['files']}
actual = {'LICENSE', 'NOTICE', 'testdata/issue-1326-oracle11g.sql'}
for folder in ['internal/upstream', 'rust/vendor', 'third_party/protocol-licenses']:
    actual.update(p.relative_to(ROOT).as_posix() for p in (ROOT / folder).rglob('*') if p.is_file())
errors.extend(sorted(expected.symmetric_difference(actual)))
for record in manifest['files']:
    path = ROOT / record['target']
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != record['imported_sha256']:
        errors.append(record['target'])
if errors:
    raise SystemExit('Upstream provenance mismatch:\n' + '\n'.join(sorted(set(errors))))
print(f"Upstream provenance verified: {len(expected)} retained resources/Rust fork files; legacy attribution preserved.")
