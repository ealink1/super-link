#!/usr/bin/env python3
"""Upload a new draft, verify GitHub asset digests, then optionally publish."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

from release_platforms import digest, version_text
from verify_release import verify

REPO = 'ealink1/super-link'


def gh(*args):
    return subprocess.check_output(['gh', *args], text=True, timeout=1800)


def verify_uploaded(files, pages):
    expected = {p.name: (p.stat().st_size, 'sha256:' + digest(p)) for p in files}
    actual = {}
    for page in pages:
        for asset in page:
            name = asset['name']
            if name in actual:
                raise ValueError('duplicate uploaded asset: ' + name)
            actual[name] = (asset['size'], asset.get('digest'))
    if actual != expected:
        raise ValueError('uploaded release asset names, sizes or SHA256 digests differ from verified local files')


def upload_files(dist, version):
    downloads = verify(dist, version)
    allowed = {d['filename'] for d in downloads}
    from release_platforms import PLATFORMS
    for goos, arch in PLATFORMS:
        metadata = [f'assets-{goos}-{arch}.json', f'downloads-{goos}-{arch}.json', f'build-info-{goos}-{arch}.json']
        allowed.update(metadata)
        for record in json.loads((dist / metadata[0]).read_text()):
            allowed.add(record['filename'])
    allowed.update(['RELEASE_NOTES.md', 'SHA256SUMS.txt'])
    manifest = ['manifest.json', 'manifest.json.sig']
    if any((dist / p).exists() for p in manifest):
        if not all((dist / p).is_file() for p in manifest):
            raise ValueError('incomplete signed manifest')
        allowed.update(manifest)
    actual = {p.name for p in dist.iterdir()}
    if actual != allowed:
        raise ValueError('unexpected or missing release files: ' + ', '.join(sorted(actual ^ allowed)))
    files = sorted(dist / name for name in allowed)
    expected_checksums = ''.join(f'{digest(p)}  {p.name}\n' for p in files if p.name != 'SHA256SUMS.txt')
    if (dist / 'SHA256SUMS.txt').read_text() != expected_checksums:
        raise ValueError('release checksum inventory differs from upload files')
    return files


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    args = parser.parse_args()
    version = version_text(args.version)
    if not re.fullmatch(r'[0-9a-f]{40}', args.source):
        parser.error('source must be the resolved immutable commit SHA')
    verify(args.dist, version)
    if not (args.dist / 'RELEASE_NOTES.md').is_file() or not (args.dist / 'SHA256SUMS.txt').is_file():
        raise ValueError('release notes and checksums are required before upload')
    publish = os.environ.get('SUPERLINK_PUBLISH') == 'true'
    if publish and not (args.dist / 'manifest.json.sig').is_file():
        raise ValueError('public release requires a signed update manifest')
    files = upload_files(args.dist, version)
    if any(p.is_symlink() or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,160}', p.name) for p in files):
        raise ValueError('unsafe release upload file')
    tag = 'v' + version
    options = ['release', 'create', tag, '--repo', REPO, '--verify-tag', '--target', args.source,
               '--draft', '--title', 'SuperLink ' + tag, '--notes-file', str(args.dist / 'RELEASE_NOTES.md')]
    if '-' in version:
        options.append('--prerelease')
    # Never overwrite an existing release/tag. A failed upload remains a draft.
    url = gh(*options, *(str(p) for p in files)).strip()
    info = json.loads(gh('api', f'repos/{REPO}/releases/tags/{tag}'))
    if not info['draft'] or info['tag_name'] != tag or info['target_commitish'] != args.source:
        raise ValueError('created release identity or draft state mismatch')
    pages = json.loads(gh('api', '--paginate', '--slurp', f'repos/{REPO}/releases/{info["id"]}/assets?per_page=100'))
    verify_uploaded(files, pages)
    if publish:
        options = ['release', 'edit', tag, '--repo', REPO, '--draft=false']
        options += ['--latest=false'] if '-' in version else ['--latest']
        gh(*options)
        info = json.loads(gh('api', f'repos/{REPO}/releases/tags/{tag}'))
        if info['draft'] or info['prerelease'] != ('-' in version):
            raise ValueError('release publication status mismatch')
    print('Verified public release:' if publish else 'Verified release draft:', url)


if __name__ == '__main__':
    main()
