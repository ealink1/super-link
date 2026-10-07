#!/usr/bin/env python3
"""Collect the license/notice files of linked Go modules for redistribution."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
template = '{{if .Module}}{{.Module.Path}}\t{{.Module.Version}}\t{{if .Module.Replace}}{{.Module.Replace.Dir}}{{else}}{{.Module.Dir}}{{end}}{{end}}'
output = subprocess.check_output(['go', 'list', '-deps', '-tags', 'gonavi_full_drivers',
                                  '-f', template, './cmd/superlink', './cmd/update-helper',
                                  './cmd/driver-agent'], cwd=ROOT, text=True)
modules = sorted(set(line for line in output.splitlines() if line.strip()))
parts = ['# Third-party notices\n\nGenerated from the native app, update helper and all-driver agent dependency graphs.\n'
         'Module versions are pinned in go.mod/go.sum. License texts below retain their original wording.\n'
         'Native SDK/system-library dependencies can have additional notices; retain their distribution licenses.\n']
missing = []
for entry in modules:
    name, version, directory = entry.split('\t')
    if name == 'github.com/ealink1/super-link':
        continue
    folder = Path(directory)
    licenses = sorted(set(path for pattern in ['LICENSE*', 'LICENCE*', 'COPYING*', 'NOTICE*', 'COPYRIGHT*']
                          for path in folder.glob(pattern) if path.is_file()))
    parts.append(f'\n## {name} {version}\n')
    if not licenses:
        missing.append(name)
        parts.append('\nNo root license file was included in this module download; review its upstream distribution.\n')
    for license_file in licenses:
        parts.append(f'\n### {license_file.name}\n\n```text\n{license_file.read_text(errors="replace").rstrip()}\n```\n')
go_root = Path(subprocess.check_output(['go', 'env', 'GOROOT'], text=True).strip())
parts.append(f'\n## Go standard library\n\n```text\n{(go_root / "LICENSE").read_text().rstrip()}\n```\n')
parts.append('\n## GoNavi interface assets\n\nIcon geometry and database marks originate from Syngnat/GoNavi at the pinned commit. '
             'Per-asset paths and SHA-256 hashes are in internal/ui/assets/gonavi/sources.json. Apache-2.0 code licensing does not transfer trademark rights.\n')
parts.append('\n## Lucide Shell interface icons\n\nOriginal SVG geometry from lucide-icons/lucide 0.468.0. '
             'Per-asset source URLs and SHA-256 hashes are in internal/ui/assets/shell/sources.json. '
             'Only currentColor is resolved at render time.\n\n```text\n'
             + (ROOT / 'internal/ui/assets/shell/LICENSE').read_text().rstrip() + '\n```\n')
parts.append('\n## Archived derivative font sources\n\nThese historical font assets are retained in the source tree but are no longer embedded in the application. NaviUI and NaviMono combine Noto Sans SC with Inter, Noto Sans and DejaVu Sans Mono Powerline glyphs. '
             'Family names have been changed. Generation and source records are in tools/build_fonts.py and third_party/fonts.\n')
for license_file in sorted((ROOT / 'third_party/fonts').glob('*')):
    if license_file.is_file() and ('LICENSE' in license_file.name or 'OFL' in license_file.name):
        parts.append(f'\n### {license_file.name}\n\n```text\n{license_file.read_text().rstrip()}\n```\n')
(ROOT / 'THIRD_PARTY_NOTICES.md').write_text(''.join(parts))
print(f'Collected notices for {len(modules)-1} modules.')
if missing:
    print('Modules needing a distribution-license review: ' + ', '.join(missing))
