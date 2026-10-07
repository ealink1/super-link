#!/usr/bin/env python3
"""Generate a download-first Markdown Release from verified build products."""
import argparse
from pathlib import Path
import re
import subprocess

from release_platforms import LABELS, PLATFORMS, version_text
from verify_release import verify

GROUPS = [('feat', '✨ 新功能'), ('fix', '🐛 问题修复'), ('perf', '⚡ 性能优化'),
          ('other', '🔧 其他改进')]


def escape(text):
    return re.sub(r'([\\`*_{}\[\]<>|])', r'\\\1', text).replace('\r', ' ').replace('\n', ' ')


def changes(revision, previous=None):
    subprocess.run(['git', 'rev-parse', '--verify', revision + '^{commit}'], check=True, capture_output=True)
    span = revision
    if previous:
        subprocess.run(['git', 'rev-parse', '--verify', previous + '^{commit}'], check=True, capture_output=True)
        span = previous + '..' + revision
    raw = subprocess.check_output(['git', 'log', '--no-merges', '--format=%s', '-n', '100', span], text=True)
    groups = {key: [] for key, _ in GROUPS}
    for subject in raw.splitlines():
        normalized = re.sub(r'^[^A-Za-z0-9\u4e00-\u9fff]*', '', subject)
        match = re.match(r'^(feat|fix|perf)(?:\([^)]*\))?!?:\s*(.+)$', normalized)
        key, text = (match.group(1), match.group(2)) if match else ('other', subject)
        if text and escape(text) not in groups[key]:
            groups[key].append(escape(text))
    return groups


def render(version, downloads, groups, previous=None, signed=False):
    text = [f'# SuperLink v{version}', '', '原生数据库、SSH 与笔记工作区。请选择与你的操作系统及 CPU 对应的安装包。', '',
            '## 📦 下载', '', '| 系统 | CPU | 安装包 | 便携版 / 更新包 |', '|---|---|---|---|']
    for goos, arch in PLATFORMS:
        records = [d for d in downloads if (d['os'], d['arch']) == (goos, arch)]
        if not records:
            continue
        def link(d):
            label = {'dmg': 'DMG', 'setup': '安装程序', 'deb': 'DEB', 'tar.gz': 'tar.gz', 'zip': 'ZIP'}[d['format']]
            return f'[{label} · {d["size"] / 1048576:.1f} MB]({d["url"]})'
        installers = ' / '.join(link(d) for d in records if d['format'] != 'zip')
        portable = ' / '.join(link(d) for d in records if d['format'] == 'zip')
        cpu = 'Intel / AMD（x64）' if arch == 'amd64' else 'ARM64（Apple Silicon）' if goos == 'darwin' else 'ARM64'
        text.append(f'| {LABELS[goos]} | {cpu} | {installers} | {portable} |')
    text += ['', '## 📝 更新内容', '']
    if not any(groups.values()):
        text += ['本次发布没有新增提交。', '']
    for key, title in GROUPS:
        if groups.get(key):
            text += [f'### {title}', ''] + ['- ' + line for line in groups[key]] + ['']
    text += ['## 🚀 安装说明', '',
             '- **macOS**：打开 DMG，将 SuperLink 拖入 Applications。未配置开发者签名及 Apple 公证时，首次打开可能需要在系统设置中允许。',
             '- **Windows**：运行安装程序，默认安装到当前用户目录；也可解压 ZIP。未配置代码签名时，系统可能显示发布者未知。',
             '- **Linux**：DEB 使用系统包管理器安装；tar.gz 解压后运行 `SuperLink/install.sh`，安装到用户目录并支持应用内更新。DEB 安装目录由系统管理，请通过新 DEB 升级。',
             '- **SQLite**：基础包已包含离线驱动。其他数据库驱动在驱动管理中按需安装；Windows ARM64 暂不提供 DuckDB 驱动。', '',
             '## 🔐 校验与更新', '',
             '下载后可使用 `SHA256SUMS.txt` 校验文件完整性。',
             '本次附带 Ed25519 签名更新清单，应用可验证更新与驱动下载。' if signed else
             '本次未配置正式更新签名，在线更新与驱动安装不可用；请使用本页安装包手动安装。', '']
    if previous:
        text += [f'[查看完整变更](https://github.com/ealink1/super-link/compare/{previous}...v{version})', '']
    text += ['[反馈问题](https://github.com/ealink1/super-link/issues) · [项目说明](https://github.com/ealink1/super-link)', '']
    return '\n'.join(text)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    parser.add_argument('--revision', default='HEAD')
    parser.add_argument('--previous')
    parser.add_argument('--platform', action='append')
    parser.add_argument('--output', type=Path, default=Path('dist/RELEASE_NOTES.md'))
    args = parser.parse_args()
    version = version_text(args.version)
    platforms = [tuple(p.split('/')) for p in args.platform] if args.platform else PLATFORMS
    downloads = verify(args.dist, version, platforms)
    args.output.write_text(render(version, downloads, changes(args.revision, args.previous), args.previous,
                                 (args.dist / 'manifest.json.sig').is_file()))
    print('Generated release notes:', args.output)


if __name__ == '__main__':
    main()
