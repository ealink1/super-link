#!/usr/bin/env python3
"""Upload verified assets to draft releases, then optionally publish them.

对齐 Go 分支的 `tools/publish_release.py`：先校验本地 dist 完整，再按仓库分流
上传。应用包与说明进应用仓库；驱动包、签名清单与校验和进独立驱动仓库，因为
清单里的驱动下载地址指向后者。两个草稿都复核通过后，先放开驱动、再放开应用，
让客户端在发现新版本时驱动链接已可用。
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from release_platforms import (
    APP_REPO,
    DRIVER_REPO,
    RELEASE_PLATFORMS,
    RELEASE_RUNNERS,
    digest,
    version_text,
)
from verify_release import verify

METADATA = ['assets-{}-{}.json', 'build-info-{}-{}.json']
SIGNED = ['manifest.json', 'manifest.json.sig']
NOTES = 'RELEASE_NOTES.md'
SUMS = 'SHA256SUMS.txt'


def gh(*args, repo=APP_REPO):
    env = dict(os.environ)
    token = env.get('SUPERLINK_DRIVER_RELEASE_TOKEN')
    if repo == DRIVER_REPO and token:
        env['GH_TOKEN'] = token
    return subprocess.check_output(
        ['gh', *args], text=True, stderr=subprocess.PIPE, timeout=1800, env=env
    )


def check_workflow_matrix(workflow):
    """工作流矩阵必须与 `RELEASE_PLATFORMS` 完全一致，否则产物会被漏检或漏发。

    用正则而不是 YAML 解析器：发布 job 只保证有标准库，不额外安装 PyYAML。
    """
    block = re.search(
        r'^ {6}matrix:\n(.*?)(?=^ {6}\S)', workflow, re.MULTILINE | re.DOTALL
    )
    if block is None:
        raise ValueError('release workflow has no build matrix')
    declared = set(
        re.findall(
            r'goos:\s*(\w+)\s*\n\s*goarch:\s*(\w+)', block.group(1)
        )
    )
    expected = set(RELEASE_PLATFORMS)
    if declared != expected:
        raise ValueError(
            f'release workflow matrix {sorted(declared)} differs from '
            f'RELEASE_PLATFORMS {sorted(expected)}'
        )
    for goos, arch in expected:
        runner = RELEASE_RUNNERS.get((goos, arch))
        if runner is None or not re.search(rf'runner:\s*{re.escape(runner)}\b', block.group(1)):
            raise ValueError(f'no runner configured for {goos}/{arch}')


def present_platforms(dist):
    return [
        (goos, arch)
        for goos, arch in RELEASE_PLATFORMS
        if (dist / f'assets-{goos}-{arch}.json').is_file()
    ]


def classify(dist, version):
    """校验 dist 并按仓库分流，返回 {repo: {文件名}}。"""
    platforms = present_platforms(dist)
    if not platforms:
        raise ValueError('no platform assets found in ' + str(dist))
    expected = {(goos, arch) for goos, arch in RELEASE_PLATFORMS}
    if set(platforms) != expected:
        raise ValueError(
            f'release covers {sorted(platforms)}, expected {sorted(expected)}'
        )
    verify(dist, version, platforms)

    app_files, driver_files = set(), set()
    for goos, arch in platforms:
        for pattern in METADATA:
            app_files.add(pattern.format(goos, arch))
        for record in json.loads((dist / f'assets-{goos}-{arch}.json').read_text()):
            if record['kind'] == 'driver':
                driver_files.add(record['filename'])
            else:
                app_files.add(record['filename'])
    app_files.add(NOTES)
    signed = [name for name in SIGNED if (dist / name).exists()]
    if signed and len(signed) != len(SIGNED):
        raise ValueError('incomplete signed manifest')
    # 签名清单是客户端读取的入口，两个仓库都放一份，便于各自校验。
    app_files.update(signed)
    driver_files.update(signed)
    app_files.add(SUMS)
    driver_files.add(SUMS)
    return {APP_REPO: app_files, DRIVER_REPO: driver_files}


def write_sums(directory, names):
    """按仓库分别写 SHA256SUMS.txt，只覆盖该仓库要上传的文件。"""
    (directory / SUMS).write_text(
        ''.join(f'{digest(directory / name)}  {name}\n' for name in sorted(names) if name != SUMS)
    )


def find_draft(repo, tag):
    for release in json.loads(gh('api', f'repos/{repo}/releases?per_page=100', repo=repo)):
        if release['tag_name'] == tag and release['draft']:
            return release
    raise ValueError(f'draft release not found for {repo} {tag}')


def verify_uploaded(directory, names, repo, release):
    expected = {
        name: ((directory / name).stat().st_size, 'sha256:' + digest(directory / name))
        for name in names
    }
    pages = json.loads(
        gh(
            'api', '--paginate', '--slurp',
            f'repos/{repo}/releases/{release["id"]}/assets?per_page=100', repo=repo,
        )
    )
    actual = {}
    for page in pages:
        for asset in page:
            if asset['name'] in actual:
                raise ValueError('duplicate uploaded asset: ' + asset['name'])
            actual[asset['name']] = (asset['size'], asset.get('digest'))
    if actual != expected:
        raise ValueError(
            f'{repo} uploaded assets mismatch; '
            f'missing={sorted(expected.keys() - actual.keys())} '
            f'extra={sorted(actual.keys() - expected.keys())}'
        )


def driver_tag_source(tag):
    """驱动仓库用自身 main 建 tag：驱动二进制与应用源码无对应提交。"""
    try:
        # 先确认 tag ref 是否存在：缺失时 `git/ref/tags` 返回 404，而
        # `commits/{ref}` 对未知引用返回 422，无法用同一个条件区分。
        gh('api', f'repos/{DRIVER_REPO}/git/ref/tags/{tag}', repo=DRIVER_REPO)
        return gh(
            'api', f'repos/{DRIVER_REPO}/commits/{tag}', '--jq', '.sha', repo=DRIVER_REPO
        ).strip()
    except subprocess.CalledProcessError as error:
        if 'HTTP 404' not in (error.stderr or ''):
            raise
    source = gh('api', f'repos/{DRIVER_REPO}/commits/main', '--jq', '.sha', repo=DRIVER_REPO)
    gh(
        'api', '--method', 'POST', f'repos/{DRIVER_REPO}/git/refs',
        '-f', 'ref=refs/tags/' + tag, '-f', 'sha=' + source.strip(), repo=DRIVER_REPO,
    )
    return source.strip()


def stage(dist, repo, names, root):
    staging = root / repo.split('/')[1]
    staging.mkdir()
    # 校验和按仓库重写；硬链接会让 write_text 截断 dist 里的同一个 inode。
    for name in sorted(names - {SUMS}):
        os.link(dist / name, staging / name)
    write_sums(staging, names)
    return sorted(staging.iterdir())


def create_draft(repo, tag, source, files, notes, title):
    """建草稿并上传；重跑时沿用已存在的草稿，只补缺失的资产。"""
    try:
        info = find_draft(repo, tag)
    except ValueError:
        options = [
            'release', 'create', tag, '--repo', repo, '--verify-tag', '--target', source,
            '--draft', '--title', title, '--notes-file', str(notes),
        ]
        if '-' in tag:
            options.append('--prerelease')
        gh(*options, *(str(path) for path in files), repo=repo)
        info = find_draft(repo, tag)
    else:
        uploaded = {
            asset['name']
            for page in json.loads(
                gh(
                    'api', '--paginate', '--slurp',
                    f'repos/{repo}/releases/{info["id"]}/assets?per_page=100', repo=repo,
                )
            )
            for asset in page
        }
        pending = [str(path) for path in files if path.name not in uploaded]
        if pending:
            gh('release', 'upload', tag, '--repo', repo, *pending, repo=repo)
    info = find_draft(repo, tag)
    if info['target_commitish'] != source:
        raise ValueError(f'{repo} draft target commit mismatch')
    verify_uploaded(files[0].parent, {path.name for path in files}, repo, info)
    print(f'Verified draft: {repo} {info["html_url"]}')
    return info


def publish_draft(repo, tag, info):
    options = ['release', 'edit', tag, '--repo', repo, '--draft=false']
    options += ['--latest=false'] if '-' in tag else ['--latest']
    gh(*options, repo=repo)
    info = json.loads(gh('api', f'repos/{repo}/releases/{info["id"]}', repo=repo))
    if info['draft'] or info['prerelease'] != ('-' in tag):
        raise ValueError(f'{repo} release publication status mismatch')
    print(f'Published: {info["html_url"]}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    parser.add_argument('--publish', default='false')
    args = parser.parse_args()
    version = version_text(args.version)
    publish = args.publish.strip().lower() == 'true'

    workflow = Path(__file__).resolve().parent.parent / '.github/workflows/release.yml'
    check_workflow_matrix(workflow.read_text())
    plan = classify(args.dist, version)
    all_names = plan[APP_REPO] | plan[DRIVER_REPO]
    actual = {p.name for p in args.dist.iterdir() if p.is_file()}
    unexpected = actual - all_names
    missing = all_names - actual
    if unexpected or missing:
        raise ValueError(
            f'unexpected or missing release files; unexpected={sorted(unexpected)} '
            f'missing={sorted(missing)}'
        )
    if publish and not (args.dist / 'manifest.json.sig').is_file():
        raise ValueError('public release requires a signed update manifest')
    # 驱动仓库需要另一个仓库的写入令牌；先失败，别等草稿建好才发现无法上传。
    if os.environ.get('CI') == 'true' and not os.environ.get('SUPERLINK_DRIVER_RELEASE_TOKEN'):
        raise ValueError('configure SUPERLINK_DRIVER_RELEASE_TOKEN for the driver repository')

    with tempfile.TemporaryDirectory(prefix='superlink-release-', dir=args.dist.parent) as temp:
        root = Path(temp)
        driver_notes = root / 'driver-notes.md'
        driver_notes.write_text(
            f'# SuperLink Driver Agents {args.tag}\n\n'
            '供 SuperLink 按需下载的数据库驱动。\n\n'
            f'应用源码：https://github.com/{APP_REPO}/commit/{args.source}\n'
            f'版本：`{args.tag}`。请通过应用中的驱动管理安装。\n'
        )
        driver_files = stage(args.dist, DRIVER_REPO, plan[DRIVER_REPO], root)
        app_files = stage(args.dist, APP_REPO, plan[APP_REPO], root)
        driver_info = create_draft(
            DRIVER_REPO, args.tag, driver_tag_source(args.tag),
            driver_files, driver_notes, 'SuperLink Driver Agents ' + args.tag,
        )
        app_info = create_draft(
            APP_REPO, args.tag, args.source,
            app_files, args.dist / NOTES, 'SuperLink ' + args.tag,
        )

    # 两个草稿都复核通过后才放开；先驱动、后应用。
    if publish:
        publish_draft(DRIVER_REPO, args.tag, driver_info)
        publish_draft(APP_REPO, args.tag, app_info)
    else:
        print('Left both releases as drafts; pass --publish true to publish them.')


if __name__ == '__main__':
    main()
