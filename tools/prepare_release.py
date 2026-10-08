"""Select driver reuse only for UI/docs changes against a signed public base."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

from driver_reuse import FILES, bounded, gh_json, load, verify_remote
from release_platforms import APP_REPO, DRIVER_BASE_URL, digest, version_text


def git(*args):
    return subprocess.check_output(['git', *args], text=True, stderr=subprocess.PIPE, timeout=30).strip()


def eligible(base, source):
    if not all(re.fullmatch('[0-9a-f]{40}', sha) for sha in [base, source]):
        return False
    subprocess.run(['git', 'merge-base', '--is-ancestor', base, source], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=30)
    paths = git('diff', '--no-renames', '--name-only', '-z', base, source).split('\0')
    files = {'README.md', 'docs/releases.md'}
    return all(not path or path in files or path.startswith(('internal/ui/', 'internal/branding/', 'docs/'))
               for path in paths)


def prepare(directory, version, source, public_key):
    if not public_key:
        return False
    info = gh_json(f'repos/{APP_REPO}/releases/latest')
    if info['draft'] or info['prerelease']:
        raise ValueError('base is not public stable')
    base_version = version_text(info['tag_name'])
    base_source = git('rev-parse', '--verify', 'refs/tags/' + info['tag_name'] + '^{commit}')
    remote_source = gh_json(f'repos/{APP_REPO}/commits/' + info['tag_name'])['sha']
    if remote_source != base_source:
        raise ValueError('local driver base tag differs from published source')
    if not eligible(base_source, source):
        print('Changes outside UI/docs require a full driver build.')
        return False
    directory.mkdir(parents=True, exist_ok=True)
    assets = {asset['name']: asset for asset in info['assets']}
    for name, output, limit in [('manifest.json', FILES[1], 1 << 20), ('manifest.json.sig', FILES[2], 1024)]:
        asset = assets[name]
        if not 0 < asset['size'] <= limit:
            raise ValueError('invalid signed base size')
        with (directory / output).open('wb') as stream:
            subprocess.run(['gh', 'api', '-H', 'Accept: application/octet-stream',
                            f'repos/{APP_REPO}/releases/assets/{asset["id"]}'],
                           stdout=stream, stderr=subprocess.PIPE, check=True, timeout=90)
    bounded(directory / FILES[2], 1024)
    manifest = json.loads(bounded(directory / FILES[1], 1 << 20))
    urls = [a['url'] for a in manifest['artifacts'] if a['kind'] == 'driver']
    match = re.fullmatch(re.escape(DRIVER_BASE_URL) + r'v([^/]+)/[^/]+', urls[0])
    if not match:
        raise ValueError('invalid driver base URL')
    proof = {'schema': 1, 'base_app_version': base_version, 'driver_version': match[1],
             'base_source': base_source, 'source': source, 'target_version': version,
             'public_key': public_key, 'manifest_sha256': digest(directory / FILES[1]),
             'signature_sha256': digest(directory / FILES[2])}
    (directory / FILES[0]).write_text(json.dumps(proof, indent=2) + '\n')
    verify_remote(load(directory, version, public_key))
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--directory', type=Path, default=Path('driver-base'))
    args = parser.parse_args()
    reused = False
    try:
        reused = prepare(args.directory, version_text(args.version), args.source,
                         os.environ.get('SUPERLINK_RELEASE_PUBLIC_KEY', ''))
    except (ValueError, KeyError, IndexError, OSError, subprocess.SubprocessError, json.JSONDecodeError):
        print('Signed driver base unavailable or ineligible; using full driver build.')
    if not reused and args.directory.exists():
        shutil.rmtree(args.directory)
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write('reuse_drivers=' + str(reused).lower() + '\n')
    print('Release mode:', 'reuse verified drivers' if reused else 'build all drivers')


if __name__ == '__main__':
    main()
