#!/usr/bin/env python3
"""Verify the pinned Fyne copy and the exact reviewed local memory patch."""
from pathlib import Path
import hashlib
import json
import subprocess

ROOT = Path(__file__).resolve().parent.parent
manifest = json.loads((ROOT / 'docs/fyne-source.json').read_text())
checkout = ROOT / 'third_party/fyne'
expected = {**manifest['original_files'], **manifest['reviewed_files']}
actual = {p.relative_to(checkout).as_posix(): p for p in checkout.rglob('*')
          if p.is_file() and not ('testdata' in p.parts and 'failed' in p.parts)}
if set(actual) != set(expected):
    raise SystemExit(f"Fyne file inventory changed: {set(actual) ^ set(expected)}")
for name, digest in expected.items():
    if hashlib.sha256(actual[name].read_bytes()).hexdigest() != digest:
        raise SystemExit(f'Unreviewed Fyne change: {name}')
try:
    replacement = json.loads(subprocess.check_output(
        ['go', 'list', '-m', '-json', 'fyne.io/fyne/v2'], cwd=ROOT, text=True))
    if Path(replacement['Replace']['Dir']).resolve() != checkout.resolve():
        raise SystemExit('The native application is not using the reviewed Fyne copy')
except subprocess.CalledProcessError:
    # This branch does not compile the Go UI: the fork is kept for reference,
    # so only the replacement directive itself must survive.
    go_mod = (ROOT / 'go.mod').read_text()
    if 'replace fyne.io/fyne/v2' not in go_mod:
        raise SystemExit('Fyne replacement directive missing from go.mod')
patch = ROOT / 'docs/fyne-memory.patch'
if hashlib.sha256(patch.read_bytes()).hexdigest() != manifest['patch_sha256']:
    raise SystemExit('Fyne memory patch changed without review')
print(f"Pinned Fyne {manifest['version']}: {len(actual)} files and memory patch verified.")
