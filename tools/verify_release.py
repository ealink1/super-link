#!/usr/bin/env python3
"""Fail closed on missing platforms, stale builds and corrupt release assets."""
import argparse
import json
from pathlib import Path, PurePosixPath
import re
import zipfile

from driver_reuse import load as load_reuse

from release_platforms import BASE_URL, DRIVER_BASE_URL, PLATFORMS, digest, platform_drivers, version_text


def read_json(path):
    return json.loads(path.read_text())


def verify_file(dist, record, version, goos, arch, names):
    name = record['filename']
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,160}', name):
        raise ValueError('unsafe artifact filename')
    if name in names:
        raise ValueError('duplicate artifact filename: ' + name)
    names.add(name)
    if (record['os'], record['arch']) != (goos, arch):
        raise ValueError('artifact platform mismatch')
    base = DRIVER_BASE_URL if record.get('kind') == 'driver' else BASE_URL
    if record['url'] != base + 'v' + version + '/' + name:
        raise ValueError('artifact download URL/version mismatch')
    path = dist / name
    if path.is_symlink() or not path.is_file():
        raise ValueError('missing or linked artifact: ' + name)
    if path.stat().st_size != record['size'] or digest(path) != record['sha256']:
        raise ValueError('artifact checksum/size mismatch: ' + name)


def verify_zip(path, version, goos, arch):
    prefix = 'SuperLink.app/' if goos == 'darwin' else 'SuperLink/'
    resource_dir = prefix + 'Contents/Resources/' if goos == 'darwin' else prefix
    executable_dir = prefix + 'Contents/MacOS/' if goos == 'darwin' else prefix
    suffix = '.exe' if goos == 'windows' else ''
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if len(entries) > 5000 or sum(e.file_size for e in entries) > 2 << 30:
            raise ValueError('application ZIP exceeds update limits')
        seen = set()
        for item in entries:
            p = PurePosixPath(item.filename)
            if not item.filename.startswith(prefix) or p.is_absolute() or '..' in p.parts or '\\' in item.filename:
                raise ValueError('unsafe application ZIP path')
            if item.filename.casefold() in seen or (item.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError('duplicate path or symlink in application ZIP')
            seen.add(item.filename.casefold())
        if archive.testzip() is not None:
            raise ValueError('application ZIP CRC mismatch')
        marker = json.loads(archive.read(resource_dir + 'superlink.package.json'))
        if (marker['id'], marker['version'], marker['os'], marker['arch']) != ('io.github.ealink1.superlink', version, goos, arch):
            raise ValueError('application ZIP identity mismatch')
        executable = ('Contents/MacOS/' if goos == 'darwin' else '') + 'superlink' + suffix
        helper = ('Contents/MacOS/' if goos == 'darwin' else '') + 'update-helper' + suffix
        if marker['executable'] != executable or marker['helper'] != helper:
            raise ValueError('unexpected package entrypoints')
        for name in ['superlink' + suffix, 'update-helper' + suffix]:
            info = archive.getinfo(executable_dir + name)
            if goos != 'windows' and not info.external_attr >> 16 & 0o111:
                raise ValueError('package executable mode missing')
        bundle = json.loads(archive.read(resource_dir + 'drivers/bundle.json'))
        if (bundle['schema'], bundle['os'], bundle['arch']) != (1, goos, arch):
            raise ValueError('bundled driver identity mismatch')
        if len(bundle['drivers']) != 1 or bundle['drivers'][0]['type'] != 'sqlite':
            raise ValueError('offline SQLite driver missing')
        import hashlib
        actual = hashlib.sha256(archive.read(resource_dir + 'drivers/sqlite-driver-agent' + suffix)).hexdigest()
        if bundle['drivers'][0]['sha256'] != actual:
            raise ValueError('bundled SQLite checksum mismatch')


def verify(dist, version, platforms=PLATFORMS, public_key=None):
    version = version_text(version)
    reuse = load_reuse(dist, version, public_key)
    downloads = []
    all_names = set()
    for goos, arch in platforms:
        info = read_json(dist / f'build-info-{goos}-{arch}.json')
        if (info['version'], info['os'], info['arch']) != (version, goos, arch):
            raise ValueError('stale build metadata')
        if public_key is not None and info['releasePublicKey'] != public_key:
            raise ValueError('compiled release key mismatch')
        if reuse and (info.get('source') != reuse[0]['source'] or info['releasePublicKey'] != reuse[0]['public_key']):
            raise ValueError('driver reuse build source or key mismatch')
        assets = read_json(dist / f'assets-{goos}-{arch}.json')
        ids = set()
        for a in assets:
            identity = (a['kind'], a['id'])
            if identity in ids:
                raise ValueError('duplicate artifact identity')
            ids.add(identity)
            if reuse and a['kind'] == 'driver':
                if a != reuse[1].get((a['id'], goos, arch)) or (a['os'], a['arch']) != (goos, arch):
                    raise ValueError('reused driver differs from signed base')
                if a['filename'] in all_names:
                    raise ValueError('duplicate artifact filename')
                all_names.add(a['filename'])
            else:
                verify_file(dist, a, version, goos, arch, all_names)
            if a['kind'] == 'driver' and (not a.get('revision') or a.get('protocol') != 'json-lines-v2'):
                raise ValueError('driver metadata missing')
        expected = {('app', 'superlink')} | {('driver', d) for d in platform_drivers(goos, arch)}
        if ids != expected:
            raise ValueError('platform driver/application inventory incomplete')
        app = next(a for a in assets if a['kind'] == 'app')
        verify_zip(dist / app['filename'], version, goos, arch)
        packages = read_json(dist / f'downloads-{goos}-{arch}.json')
        formats = set()
        for d in packages:
            if d['format'] in formats:
                raise ValueError('duplicate installer format')
            formats.add(d['format'])
            # The portable ZIP is shared by the update manifest and download table.
            if d['format'] == 'zip':
                if any(d[k] != app[k] for k in ['filename', 'size', 'sha256', 'url']):
                    raise ValueError('download ZIP differs from update manifest')
            else:
                verify_file(dist, d, version, goos, arch, all_names)
        expected_formats = {'zip'} | {'darwin': {'dmg'}, 'windows': {'setup'}, 'linux': {'tar.gz', 'deb'}}[goos]
        if formats != expected_formats:
            raise ValueError('installer inventory incomplete')
        downloads.extend(packages)
    return downloads


def write_checksums(dist):
    paths = sorted(p for p in dist.iterdir() if p.is_file() and p.name != 'SHA256SUMS.txt')
    (dist / 'SHA256SUMS.txt').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in paths))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    parser.add_argument('--version', required=True)
    parser.add_argument('--platform', action='append', help='restrict local verification, e.g. darwin/arm64')
    parser.add_argument('--public-key')
    parser.add_argument('--checksums', action='store_true')
    args = parser.parse_args()
    platforms = [tuple(p.split('/')) for p in args.platform] if args.platform else PLATFORMS
    if any(p not in PLATFORMS for p in platforms) or len(platforms) != len(set(platforms)):
        parser.error('invalid or duplicate platform')
    verify(args.dist, args.version, platforms, args.public_key)
    if args.checksums:
        write_checksums(args.dist)
    print(f'Verified {len(platforms)} platform(s), installers, update ZIPs and all driver hashes')


if __name__ == '__main__':
    main()
