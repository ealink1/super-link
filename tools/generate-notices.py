#!/usr/bin/env python3
"""Collect the Cargo dependency graph's original license files for redistribution."""
from pathlib import Path
import json
import subprocess
import hashlib

ROOT = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1', '--manifest-path', 'rust/Cargo.toml'], cwd=ROOT, text=True))
workspace = set(metadata['workspace_members'])
parts = ['# Third-party notices\n\nGenerated from the Rust workspace dependency graph pinned by rust/Cargo.lock.\n'
         'Cargo SPDX license expressions and available original license/notice texts are retained below.\n'
         'This inventory includes build dependencies and platform-specific dependencies.\n']
missing = []
texts = {}


def retain_license(path):
    text = '\n'.join(line.rstrip() for line in path.read_text(errors='replace').splitlines()).rstrip()
    key = hashlib.sha256(text.encode()).hexdigest()[:16]
    texts.setdefault(key, text)
    return f'[{path.name}](#license-{key})'


for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
    if package['id'] in workspace:
        continue
    folder = Path(package['manifest_path']).parent
    files = sorted({p for pattern in ['LICENSE*', 'LICENCE*', 'COPYING*', 'NOTICE*', 'COPYRIGHT*'] for p in folder.glob(pattern) if p.is_file()})
    if package.get('license_file'):
        path = folder / package['license_file']
        if path.is_file() and path not in files:
            files.append(path)
    parts.append(f"\n## {package['name']} {package['version']}\n\nLicense: {package.get('license') or 'see license file'}.\n")
    if package.get('repository'):
        parts.append(f"Source: {package['repository']}\n")
    if not files:
        missing.append(f"{package['name']} {package['version']}")
        parts.append('\nThis crate archive contains no root license text; use its declared SPDX license and upstream repository.\n')
    for path in files:
        parts.append(f'\nOriginal text: {retain_license(path)}\n')
for folder in ['third_party/protocol-licenses', 'third_party/fonts', 'assets/shell']:
    for path in sorted((ROOT / folder).glob('*')):
        if path.is_file() and any(k in path.name.upper() for k in ['LICENSE', 'OFL', 'NOTICE']):
            parts.append(f'\n## {path.relative_to(ROOT)}\n\nOriginal text: {retain_license(path)}\n')
parts.append('\n## GoNavi interface and protocol attribution\n\nReviewed assets originate from Syngnat/GoNavi at commit 6e20b6ddf56b2ae76f7e5d5c6a3505cef1edc871. '
             'Original paths/hashes are retained in assets/superlink/sources.json and docs/upstream-import.json. '
             'Rust IRIS/Caché and HighGo SM3 protocol adaptations retain their original MIT attribution; see UPSTREAM.md. '
             'Trademark rights are not transferred by source-code licenses.\n')
for key, text in texts.items():
    parts.append(f'\n<a id="license-{key}"></a>\n\n## License text {key}\n\n```text\n{text}\n```\n')
(ROOT / 'THIRD_PARTY_NOTICES.md').write_text(''.join(parts))
print(f"Collected notices for {len(metadata['packages']) - len(workspace)} Cargo packages.")
if missing:
    print('Crate archives without root license text (SPDX metadata retained): ' + ', '.join(missing))
