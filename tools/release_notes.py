#!/usr/bin/env python3
"""Generate a download-first Markdown Release from verified build products."""
import argparse
import json
from pathlib import Path
import re
import subprocess

from release_platforms import LABELS, RELEASE_PLATFORMS, version_text
from verify_release import verify

GROUPS = [
    ('feat', '✨ 新功能'),
    ('fix', '🐛 问题修复'),
    ('perf', '⚡ 性能优化'),
    ('other', '🔧 其他改进'),
]
APP_REPO = 'ealink1/super-link'


def escape(text):
    return re.sub(r'([\\`*_{}\[\]<>|])', r'\\\1', text).replace('\r', ' ').replace('\n', ' ')


def changes(revision, previous=None):
    span = revision if not previous else previous + '..' + revision
    subprocess.run(
        ['git', 'rev-parse', '--verify', revision + '^{commit}'],
        check=True,
        capture_output=True,
    )
    raw = subprocess.check_output(
        ['git', 'log', '--no-merges', '--format=%s', '-n', '100', span],
        text=True,
    )
    groups = {key: [] for key, _ in GROUPS}
    for subject in raw.splitlines():
        normalized = re.sub(r'^[^A-Za-z0-9一-鿿]*', '', subject)
        match = re.match(r'^(feat|fix|perf)(?:\([^)]*\))?!?:\s*(.+)$', normalized)
        key, text = (match.group(1), match.group(2)) if match else ('other', subject)
        if text and escape(text) not in groups[key]:
            groups[key].append(escape(text))
    return groups


def download_rows(dist):
    rows = []
    for goos, arch in RELEASE_PLATFORMS:
        assets_path = dist / f'assets-{goos}-{arch}.json'
        if not assets_path.is_file():
            continue
        assets = json.loads(assets_path.read_text())
        app = next((a for a in assets if a['kind'] == 'app'), None)
        if app is None:
            continue
        rows.append((goos, arch, app))
    return rows


def render(version, dist, groups, previous=None, signed=False):
    text = [
        f'# SuperLink v{version}',
        '',
        '原生数据库、SSH 与笔记工作区。请选择与你的操作系统及 CPU 对应的安装包。',
        '',
        '## 📦 下载',
        '',
        '| 系统 | CPU | 便携版 / 更新包 |',
        '|---|---|---|',
    ]
    for goos, arch, app in download_rows(dist):
        cpu = 'Intel / AMD（x64）' if arch == 'amd64' else 'ARM64'
        if goos == 'darwin' and arch == 'arm64':
            cpu = 'ARM64（Apple Silicon）'
        text.append(
            f'| {LABELS[goos]} | {cpu} | '
            f'[ZIP · {app["size"] / 1048576:.1f} MB]({app["url"]}) |'
        )
    text += ['', '## 📝 更新内容', '']
    if not any(groups.values()):
        text += ['本次发布没有新增提交。', '']
    for key, title in GROUPS:
        if groups.get(key):
            text += [f'### {title}', ''] + ['- ' + line for line in groups[key]] + ['']
    text += [
        '## 🚀 安装说明',
        '',
        '- **macOS**：解压 ZIP 后将 SuperLink.app 拖入 Applications。未配置开发者签名及 Apple 公证时，首次打开可能需要在系统设置中允许。',
        '- **Windows**：解压 ZIP 后运行 `SuperLink/superlink.exe`。未配置代码签名时，系统可能显示发布者未知。',
        '- **SQLite**：基础包已包含离线驱动。其他数据库驱动在驱动管理中按需安装。',
        '',
        '## 🔐 校验与更新',
        '',
        '下载后可使用 `SHA256SUMS.txt` 校验文件完整性。',
    ]
    if signed:
        text += [
            '本次附带 Ed25519 签名更新清单，应用可验证更新与驱动下载。',
            '',
        ]
    else:
        text += [
            '本次未配置正式更新签名，在线更新与驱动安装不可用；请使用本页安装包手动安装。',
            '',
        ]
    if previous:
        text += [
            f'[查看完整变更](https://github.com/{APP_REPO}/compare/{previous}...v{version})',
            '',
        ]
    text += [
        f'[反馈问题](https://github.com/{APP_REPO}/issues) · '
        f'[项目说明](https://github.com/{APP_REPO})',
        '',
    ]
    return '\n'.join(text)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--previous')
    parser.add_argument('--platform', action='append')
    parser.add_argument('--signed', default='false')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    version = version_text(args.version)
    platforms = (
        [tuple(p.split('/')) for p in args.platform] if args.platform else RELEASE_PLATFORMS
    )
    verify(args.dist, version, platforms)
    notes = render(
        version,
        args.dist,
        changes(args.revision, args.previous),
        args.previous,
        args.signed.strip().lower() == 'true',
    )
    output = args.output or args.dist / 'RELEASE_NOTES.md'
    output.write_text(notes)
    print('Generated release notes:', output)


if __name__ == '__main__':
    main()
