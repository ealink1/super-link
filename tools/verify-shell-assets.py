#!/usr/bin/env python3
"""Check licensed Shell SVG sources before distribution."""
import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET

DIRECTORY = Path(__file__).resolve().parent.parent / 'assets/shell'


def main():
    manifest = json.loads((DIRECTORY / 'sources.json').read_text())
    expected = set()
    prefix = f'https://raw.githubusercontent.com/lucide-icons/lucide/{manifest["version"]}/icons/'
    for record in manifest['assets']:
        name = record['file']
        if Path(name).name != name or not name.endswith('.svg') or name in expected:
            raise SystemExit('invalid or duplicate Shell asset path')
        if record['url'] != prefix + name:
            raise SystemExit('Shell asset source/version differs')
        expected.add(name)
        raw = (DIRECTORY / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != record['sha256']:
            raise SystemExit(f'Shell asset hash changed: {name}')
        root = ET.fromstring(raw)
        if root.tag.rsplit('}', 1)[-1] != 'svg' or not root.get('viewBox'):
            raise SystemExit(f'invalid Shell SVG: {name}')
    actual = {p.name for p in DIRECTORY.glob('*.svg')}
    if expected != actual:
        raise SystemExit('Shell asset inventory differs')
    if not (DIRECTORY / 'LICENSE').is_file():
        raise SystemExit('Shell asset license missing')
    print(f'Shell asset provenance verified: {len(expected)} Lucide SVGs at {manifest["version"]}.')


if __name__ == '__main__':
    main()
