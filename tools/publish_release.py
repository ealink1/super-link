#!/usr/bin/env python3
"""Upload a new draft, verify GitHub asset digests, then optionally publish."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from release_platforms import APP_REPO, DRIVER_REPO, PLATFORMS, digest, version_text
from verify_release import verify


def gh(*args, repo=APP_REPO):
    env = dict(os.environ)
    if repo == DRIVER_REPO and env.get('SUPERLINK_DRIVER_RELEASE_TOKEN'):
        env['GH_TOKEN'] = env['SUPERLINK_DRIVER_RELEASE_TOKEN']
    return subprocess.check_output(['gh', *args], text=True, stderr=subprocess.PIPE, env=env, timeout=1800)


def created_release(tag, repo=APP_REPO):
    # GitHub's by-tag endpoint excludes drafts. A newly created release is in
    # the bounded recent-release listing, which includes authorized drafts.
    recent = json.loads(gh('api', f'repos/{repo}/releases?per_page=100', repo=repo))
    matches = [release for release in recent if release['tag_name'] == tag]
    if len(matches) != 1:
        raise ValueError('new release was not uniquely found in the recent release listing')
    return matches[0]


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
    if any(p.is_symlink() or not p.is_file() for p in dist.iterdir()):
        raise ValueError('linked or non-file release input')
    downloads = verify(dist, version)
    allowed = {d['filename'] for d in downloads}
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



def stage_release(dist, version, repo, stage):
    # Validate the full matrix and reject undeclared files before partitioning.
    upload_files(dist, version)
    names = {'manifest.json', 'manifest.json.sig'} if (dist / 'manifest.json').is_file() else set()
    if repo == APP_REPO:
        names.add('RELEASE_NOTES.md')
    for goos, arch in PLATFORMS:
        metadata = 'downloads' if repo == APP_REPO else 'assets'
        records = json.loads((dist / f'{metadata}-{goos}-{arch}.json').read_text())
        names.update(record['filename'] for record in records
                     if repo == APP_REPO or record['kind'] == 'driver')
    stage.mkdir()
    for name in sorted(names):
        os.link(dist / name, stage / name)
    files = sorted(stage.iterdir())
    checksums = stage / 'SHA256SUMS.txt'
    checksums.write_text(''.join(f'{digest(p)}  {p.name}\n' for p in files))
    return sorted([*files, checksums])


def driver_tag_source(tag):
    try:
        gh('api', f'repos/{DRIVER_REPO}/git/ref/tags/{tag}', repo=DRIVER_REPO)
        return gh('api', f'repos/{DRIVER_REPO}/commits/{tag}', '--jq', '.sha', repo=DRIVER_REPO).strip()
    except subprocess.CalledProcessError as error:
        if 'HTTP 404' not in (error.stderr or ''):
            raise
    source = gh('api', f'repos/{DRIVER_REPO}/commits/main', '--jq', '.sha', repo=DRIVER_REPO).strip()
    gh('api', '--method', 'POST', f'repos/{DRIVER_REPO}/git/refs',
       '-f', 'ref=refs/tags/' + tag, '-f', 'sha=' + source, repo=DRIVER_REPO)
    return source


def create_draft(repo, tag, source, files, notes, title):
    options = ['release', 'create', tag, '--repo', repo, '--verify-tag', '--target', source,
               '--draft', '--title', title, '--notes-file', str(notes)]
    if '-' in tag:
        options.append('--prerelease')
    url = gh(*options, *(str(p) for p in files), repo=repo).strip()
    info = created_release(tag, repo)
    if not info['draft'] or info['tag_name'] != tag or info['target_commitish'] != source:
        raise ValueError('created release identity or draft state mismatch')
    pages = json.loads(gh('api', '--paginate', '--slurp',
                         f'repos/{repo}/releases/{info["id"]}/assets?per_page=100', repo=repo))
    verify_uploaded(files, pages)
    print('Verified release draft:', url)
    return info


def publish_draft(repo, tag, info):
    options = ['release', 'edit', tag, '--repo', repo, '--draft=false']
    options += ['--latest=false'] if '-' in tag else ['--latest']
    gh(*options, repo=repo)
    info = json.loads(gh('api', f'repos/{repo}/releases/{info["id"]}', repo=repo))
    if info['draft'] or info['prerelease'] != ('-' in tag):
        raise ValueError('release publication status mismatch')


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
    publish = os.environ.get('SUPERLINK_PUBLISH') == 'true'
    if publish and not (args.dist / 'manifest.json.sig').is_file():
        raise ValueError('public release requires a signed update manifest')
    # Fail before creating drafts if cross-repository authentication is absent.
    if os.environ.get('CI') == 'true' and not os.environ.get('SUPERLINK_DRIVER_RELEASE_TOKEN'):
        raise ValueError('configure SUPERLINK_DRIVER_RELEASE_TOKEN for the driver repository')
    tag = 'v' + version
    with tempfile.TemporaryDirectory(prefix='superlink-release-', dir=args.dist.parent) as directory:
        root = Path(directory)
        app_files = stage_release(args.dist, version, APP_REPO, root / 'app')
        driver_files = stage_release(args.dist, version, DRIVER_REPO, root / 'drivers')
        notes = root / 'driver-notes.md'
        notes.write_text(f'# SuperLink Driver Agents {tag}\n\n'
                         f'供 SuperLink 按需下载的数据库驱动。\n\n'
                         f'应用源码： https://github.com/{APP_REPO}/commit/{args.source}\n'
                         f'版本：`{tag}`。请通过应用中的驱动管理安装。\n')
        driver_source = driver_tag_source(tag)
        driver_info = create_draft(DRIVER_REPO, tag, driver_source, driver_files,
                                   notes, 'SuperLink Driver Agents ' + tag)
        app_info = create_draft(APP_REPO, tag, args.source, app_files,
                                args.dist / 'RELEASE_NOTES.md', 'SuperLink ' + tag)
        # Verify both drafts first; make drivers available before exposing the app.
        if publish:
            publish_draft(DRIVER_REPO, tag, driver_info)
            publish_draft(APP_REPO, tag, app_info)


if __name__ == '__main__':
    main()
