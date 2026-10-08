#!/usr/bin/env python3
"""Verify the recorded upstream icon inventory and local content hashes."""
import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
DIRECTORY = ROOT / 'internal/ui/assets/superlink'


def main():
    manifest = json.loads((DIRECTORY / 'sources.json').read_text())
    seen = set()
    svg_count, png_count = 0, 0
    for record in manifest['assets']:
        path = ROOT / record['file']
        if path.parent != DIRECTORY or path.name in seen:
            raise SystemExit('duplicate or out-of-directory asset in manifest')
        seen.add(path.name)
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != record['sha256']:
            raise SystemExit(f'asset hash changed: {path.name}')
        if path.suffix == '.svg':
            root = ET.fromstring(raw)
            if root.tag.rsplit('}', 1)[-1] != 'svg' or not (root.get('viewBox') or root.get('width') and root.get('height')):
                raise SystemExit(f'invalid SVG root: {path.name}')
            svg_count += 1
        elif path.suffix == '.png':
            if not raw.startswith(b'\x89PNG\r\n\x1a\n'):
                raise SystemExit(f'invalid PNG signature: {path.name}')
            png_count += 1
        else:
            raise SystemExit(f'unsupported asset: {path.name}')
    actual = {path.name for path in DIRECTORY.iterdir() if path.is_file() and path.name != 'sources.json'}
    if actual != seen:
        raise SystemExit(f'asset inventory differs: {sorted(actual ^ seen)}')
    print(f'UI asset provenance verified: {svg_count} SVGs, {png_count} PNGs at {manifest["local_source_revision"]}.')


if __name__ == '__main__':
    main()
