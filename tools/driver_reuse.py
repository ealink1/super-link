"""Authenticate unchanged driver records; keep their immutable download URLs."""
import argparse
from functools import lru_cache
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from release_platforms import DRIVER_BASE_URL, DRIVER_REPO, PLATFORMS, digest, platform_drivers, version_text

ROOT = Path(__file__).resolve().parent.parent
FILES = ('driver-reuse.json', 'driver-base-manifest.json', 'driver-base-manifest.json.sig')
FIELDS = {'schema', 'base_app_version', 'driver_version', 'base_source', 'source',
          'target_version', 'public_key', 'manifest_sha256', 'signature_sha256'}


def bounded(path, limit):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
        raise ValueError('invalid or oversized driver reuse input')
    with path.open('rb') as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('oversized driver reuse input')
    return raw


def authenticate(dist, proof):
    command = ('go', 'run', './tools/verify-driver-reuse',
               '--manifest', str((dist / FILES[1]).resolve()),
               '--signature', str((dist / FILES[2]).resolve()),
               '--public-key', proof['public_key'], '--base-version', proof['base_app_version'],
               '--target-version', proof['target_version'])
    authenticated(command, proof['manifest_sha256'], proof['signature_sha256'])


@lru_cache(maxsize=16)
def authenticated(command, manifest_hash, signature_hash):
    # load() rechecks both file digests before reaching this process-local cache.
    subprocess.run(command, cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
                   stderr=subprocess.PIPE, timeout=180)


def load(dist, version, public_key=None):
    present = [os.path.lexists(dist / name) for name in FILES]
    if not any(present):
        return None
    if not all(present):
        raise ValueError('incomplete driver reuse proof')
    proof = json.loads(bounded(dist / FILES[0], 4096))
    if set(proof) != FIELDS or proof['schema'] != 1 or proof['target_version'] != version:
        raise ValueError('driver reuse proof identity mismatch')
    if public_key is not None and public_key != proof['public_key']:
        raise ValueError('driver reuse key mismatch')
    for key in ['source', 'base_source']:
        if not re.fullmatch('[0-9a-f]{40}', proof[key]):
            raise ValueError('invalid driver reuse source')
    version_text(proof['driver_version'])
    for name, key, limit in [(FILES[1], 'manifest_sha256', 1 << 20), (FILES[2], 'signature_sha256', 1024)]:
        bounded(dist / name, limit)
        if digest(dist / name) != proof[key]:
            raise ValueError('driver reuse digest mismatch')
    authenticate(dist, proof)
    base = json.loads((dist / FILES[1]).read_text())
    records = {}
    names = set()
    for record in base['artifacts']:
        if record['kind'] != 'driver':
            continue
        identity = (record['id'], record['os'], record['arch'])
        name = record['filename']
        if identity in records or name in names or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,160}', name):
            raise ValueError('duplicate or unsafe reused driver')
        if record['url'] != DRIVER_BASE_URL + 'v' + proof['driver_version'] + '/' + name:
            raise ValueError('driver reuse URL/version mismatch')
        if record.get('protocol') != 'json-lines-v2' or not record.get('revision'):
            raise ValueError('driver reuse protocol missing')
        records[identity] = record
        names.add(name)
    expected = {(d, goos, arch) for goos, arch in PLATFORMS for d in platform_drivers(goos, arch)}
    if set(records) != expected:
        raise ValueError('driver base inventory incomplete')
    return proof, records


def gh_json(endpoint):
    with tempfile.TemporaryFile() as output:
        subprocess.run(['gh', 'api', endpoint], timeout=90, stdout=output,
                       check=True, stderr=subprocess.PIPE)
        output.seek(0)
        raw = output.read((2 << 20) + 1)
    if len(raw) > 2 << 20:
        raise ValueError('release metadata exceeds limit')
    return json.loads(raw)


def verify_remote(reuse):
    proof, records = reuse
    info = gh_json(f'repos/{DRIVER_REPO}/releases/tags/v' + proof['driver_version'])
    if info['draft'] or info['prerelease'] or info['tag_name'] != 'v' + proof['driver_version']:
        raise ValueError('reused drivers must have a public stable release')
    assets = {}
    # 131 agents plus metadata: two bounded pages are sufficient.
    for page in [1, 2]:
        for asset in gh_json(f'repos/{DRIVER_REPO}/releases/{info["id"]}/assets?per_page=100&page={page}'):
            if asset['name'] in assets:
                raise ValueError('duplicate remote driver asset')
            assets[asset['name']] = (asset['size'], asset.get('digest'))
    for record in records.values():
        if assets.get(record['filename']) != (record['size'], 'sha256:' + record['sha256']):
            raise ValueError('remote driver size or checksum mismatch')


def attach(base_dir, dist, version, public_key):
    proof, records = load(base_dir, version, public_key)
    for path in dist.glob('assets-*.json'):
        info_path = dist / path.name.replace('assets-', 'build-info-')
        info = json.loads(info_path.read_text())
        if info['source'] != proof['source']:
            raise ValueError('build source differs from reuse proof')
        assets = json.loads(path.read_text())
        if any(a['kind'] != 'app' for a in assets):
            raise ValueError('application-only build contains standalone drivers')
        assets += [record for (_, goos, arch), record in records.items()
                   if (goos, arch) == (info['os'], info['arch'])]
        path.write_text(json.dumps(assets, indent=2) + '\n')
    for name in FILES:
        shutil.copyfile(base_dir / name, dist / name)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--base', type=Path, required=True)
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    parser.add_argument('--version', required=True)
    parser.add_argument('--public-key', required=True)
    args = parser.parse_args()
    attach(args.base, args.dist, version_text(args.version), args.public_key)


if __name__ == '__main__':
    main()
