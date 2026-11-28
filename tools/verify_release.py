#!/usr/bin/env python3
"""Fail closed on missing platforms, stale builds and corrupt release assets.

本仓库的 `tools/build.py` 产出便携 ZIP、`assets-{os}-{arch}.json` 与
`build-info-{os}-{arch}.json`；Go 分支的 DMG/DEB/安装程序在本分支尚未生成，
因此校验只覆盖实际存在的产物，不假装有安装包。
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import zipfile

from release_platforms import (
    PLATFORMS,
    RELEASE_PLATFORMS,
    digest,
    platform_drivers,
    version_text,
)

APP_ID = 'io.github.ealink1.superlink'
ARTIFACT_NAME = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,160}')


def read_json(path):
    return json.loads(path.read_text())


def verify_artifact_file(dist, record, version, goos, arch, names):
    name = record['filename']
    if not ARTIFACT_NAME.fullmatch(name):
        raise ValueError('unsafe artifact filename: ' + name)
    if name in names:
        raise ValueError('duplicate artifact filename: ' + name)
    names.add(name)
    if (record['os'], record['arch']) != (goos, arch):
        raise ValueError('artifact platform mismatch: ' + name)
    if '/releases/download/v' + version + '/' not in record['url']:
        raise ValueError('artifact download URL/version mismatch: ' + name)
    path = dist / name
    if path.is_symlink() or not path.is_file():
        raise ValueError('missing or linked artifact: ' + name)
    if path.stat().st_size != record['size'] or digest(path) != record['sha256']:
        raise ValueError('artifact checksum/size mismatch: ' + name)


def verify_zip(path, version, goos, arch):
    prefix = 'SuperLink.app/' if goos == 'darwin' else 'SuperLink/'
    resources = prefix + 'Contents/Resources/' if goos == 'darwin' else prefix
    executables = prefix + 'Contents/MacOS/' if goos == 'darwin' else prefix
    suffix = '.exe' if goos == 'windows' else ''
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if len(entries) > 5000 or sum(e.file_size for e in entries) > 2 << 30:
            raise ValueError('application ZIP exceeds update limits')
        seen = set()
        for item in entries:
            p = PurePosixPath(item.filename)
            if not item.filename.startswith(prefix) or p.is_absolute() or '..' in p.parts:
                raise ValueError('unsafe application ZIP path')
            if '\\' in item.filename or item.filename.casefold() in seen:
                raise ValueError('duplicate or unsafe path in application ZIP')
            if (item.external_attr >> 16) & 0o170000 == 0o120000:
                raise ValueError('symlink in application ZIP')
            seen.add(item.filename.casefold())
        if archive.testzip() is not None:
            raise ValueError('application ZIP CRC mismatch')
        marker = json.loads(archive.read(resources + 'superlink.package.json'))
        if (marker['id'], marker['version'], marker['os'], marker['arch']) != (
            APP_ID,
            version,
            goos,
            arch,
        ):
            raise ValueError('application ZIP identity mismatch')
        for name in ['superlink' + suffix, 'update-helper' + suffix]:
            info = archive.getinfo(executables + name)
            if goos != 'windows' and not (info.external_attr >> 16) & 0o111:
                raise ValueError('package executable mode missing: ' + name)
        bundle = json.loads(archive.read(resources + 'drivers/bundle.json'))
        if (bundle['schema'], bundle['os'], bundle['arch']) != (1, goos, arch):
            raise ValueError('bundled driver identity mismatch')
        if len(bundle['drivers']) != 1 or bundle['drivers'][0]['type'] != 'sqlite':
            raise ValueError('offline SQLite driver missing')
        actual = digest_bytes(archive.read(resources + 'drivers/sqlite-driver-agent' + suffix))
        if bundle['drivers'][0]['sha256'] != actual:
            raise ValueError('bundled SQLite checksum mismatch')


def digest_bytes(raw):
    return hashlib.sha256(raw).hexdigest()


def verify(dist, version, platforms=None, public_key=None):
    version = version_text(version)
    platforms = tuple(platforms) if platforms is not None else RELEASE_PLATFORMS
    names = set()
    declared = set()
    for goos, arch in platforms:
        info = read_json(dist / f'build-info-{goos}-{arch}.json')
        if (info['version'], info['os'], info['arch']) != (version, goos, arch):
            raise ValueError(f'stale build metadata: {goos}/{arch}')
        if public_key is not None and info['releasePublicKey'] != public_key:
            raise ValueError(f'compiled release key mismatch: {goos}/{arch}')
        assets = read_json(dist / f'assets-{goos}-{arch}.json')
        identities = set()
        supportable = set(platform_drivers(goos, arch))
        for record in assets:
            identity = (record['kind'], record['id'])
            if identity in identities:
                raise ValueError(f'duplicate artifact identity: {goos}/{arch} {identity}')
            identities.add(identity)
            verify_artifact_file(dist, record, version, goos, arch, names)
            if record['kind'] == 'driver':
                if record['id'] not in supportable:
                    raise ValueError(f'driver unavailable on {goos}/{arch}: {record["id"]}')
                if (
                    not record.get('revision')
                    or record.get('protocol') != 'json-lines-v3-rust'
                ):
                    raise ValueError(
                        'driver compatibility metadata missing: ' + record['filename']
                    )
                declared.add(record['id'])
        apps = [record for record in assets if record['kind'] == 'app']
        if len(apps) != 1 or apps[0]['id'] != 'superlink':
            raise ValueError(f'exactly one application package is required: {goos}/{arch}')
        verify_zip(dist / apps[0]['filename'], version, goos, arch)
    # 驱动可以整体不发（仅应用发布），但不能只在部分受支持平台上出现，
    # 否则客户端会在缺失平台上找不到更新。
    for driver in sorted(declared):
        for goos, arch in platforms:
            if driver not in platform_drivers(goos, arch):
                continue
            assets = read_json(dist / f'assets-{goos}-{arch}.json')
            if not any(r['kind'] == 'driver' and r['id'] == driver for r in assets):
                raise ValueError(f'driver missing on {goos}/{arch}: {driver}')
    return names


def write_checksums(dist):
    paths = sorted(p for p in dist.iterdir() if p.is_file() and p.name != 'SHA256SUMS.txt')
    (dist / 'SHA256SUMS.txt').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in paths))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    parser.add_argument('--version', required=True)
    parser.add_argument('--platform', action='append', help='e.g. darwin/arm64')
    parser.add_argument('--public-key')
    parser.add_argument('--checksums', action='store_true')
    args = parser.parse_args()
    platforms = (
        [tuple(p.split('/')) for p in args.platform] if args.platform else RELEASE_PLATFORMS
    )
    if any(p not in PLATFORMS for p in platforms) or len(platforms) != len(set(platforms)):
        parser.error('invalid or duplicate platform')
    if args.public_key:
        base64.b64decode(args.public_key, validate=True)
    verify(args.dist, args.version, platforms, args.public_key)
    if args.checksums:
        write_checksums(args.dist)
    print(f'Verified {len(platforms)} platform(s): packages, bundled SQLite and driver metadata')


if __name__ == '__main__':
    main()
