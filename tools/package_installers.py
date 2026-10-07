#!/usr/bin/env python3
"""Native DMG / Inno Setup / DEB and portable Linux tar packaging."""
import argparse
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tarfile
import tempfile

from release_platforms import download_record, version_text, PLATFORMS

ROOT = Path(__file__).resolve().parent.parent


def run(args):
    subprocess.run([str(a) for a in args], check=True, timeout=600)


def desktop_entry():
    return ('[Desktop Entry]\nType=Application\nName=SuperLink\n'
            'Comment=Database, SSH and notes workspace\n'
            'Exec=/opt/superlink/superlink\nIcon=superlink\nTerminal=false\n'
            'Categories=Development;Database;\n')


def create_windows_icon(destination):
    png = (ROOT / 'internal/branding/assets/superlink.png').read_bytes()
    if png[:8] != b'\x89PNG\r\n\x1a\n' or struct.unpack('>II', png[16:24]) != (256, 256):
        raise ValueError('installer icon must be a 256px PNG')
    destination.write_bytes(struct.pack('<HHH', 0, 1, 1) +
                            struct.pack('<BBBBHHII', 0, 0, 0, 0, 1, 32, len(png), 22) + png)


def macos_installer(bundle, dist, version, arch, temporary):
    source = temporary / 'disk'
    source.mkdir()
    shutil.copytree(bundle, source / 'SuperLink.app')
    (source / 'Applications').symlink_to('/Applications', target_is_directory=True)
    (source / '安装说明.txt').write_text('将 SuperLink 拖入 Applications 后打开。\n'
        '用户数据保存在系统配置目录，不在安装包中。\n'
        '未使用开发者证书签名及 Apple 公证时，首次打开可能需要在系统设置中允许。\n')
    target = dist / f'SuperLink-{version}-macos-{arch}.dmg'
    run(['hdiutil', 'create', '-ov', '-format', 'UDZO', '-volname',
         f'SuperLink {version}', '-srcfolder', source, target])
    run(['hdiutil', 'verify', target])
    return [(target, 'dmg')]


def windows_installer(bundle, dist, version, arch, temporary):
    icon = temporary / 'superlink.ico'
    create_windows_icon(icon)
    compiler = shutil.which('ISCC') or shutil.which('ISCC.exe')
    if not compiler:
        candidate = Path(os.environ.get('ProgramFiles(x86)', r'C:\Program Files (x86)')) / 'Inno Setup 6/ISCC.exe'
        if candidate.is_file():
            compiler = str(candidate)
    if not compiler:
        raise RuntimeError('Inno Setup 6.3+ is required; install it before packaging')
    run([compiler, f'/DVersion={version}', f'/DArchitecture={arch}',
         f'/DSourceDir={bundle}', f'/DOutputDirectory={dist}', f'/DIconFile={icon}',
         ROOT / 'packaging/windows.iss'])
    target = dist / f'SuperLink-{version}-windows-{arch}-setup.exe'
    if not target.is_file() or target.read_bytes()[:2] != b'MZ':
        raise RuntimeError('Windows installer was not generated')
    return [(target, 'setup')]


def linux_installers(bundle, dist, version, arch, temporary):
    # Include an opt-in user installation script in the tar; never touch user data.
    portable = temporary / 'portable/SuperLink'
    shutil.copytree(bundle, portable)
    shutil.copy2(ROOT / 'packaging/install-linux.sh', portable / 'install.sh')
    portable.joinpath('install.sh').chmod(0o755)
    tar = dist / f'SuperLink-{version}-linux-{arch}.tar.gz'
    with tarfile.open(tar, 'w:gz', format=tarfile.PAX_FORMAT) as archive:
        archive.add(portable, arcname='SuperLink')
    root = temporary / 'deb'
    shutil.copytree(bundle, root / 'opt/superlink')
    control = root / 'DEBIAN'; control.mkdir()
    installed_kib = sum(p.stat().st_size for p in bundle.rglob('*') if p.is_file()) // 1024 + 1
    control.joinpath('control').write_text(
        'Package: superlink\n' + f'Version: {version.replace("-", "~", 1)}\nArchitecture: {arch}\n'
        'Maintainer: SuperLink <noreply@github.com>\nSection: devel\nPriority: optional\n'
        f'Installed-Size: {installed_kib}\n'
        'Depends: libgl1, libx11-6, libxcursor1, libxrandr2, libxinerama1, libxi6, libxxf86vm1, libwayland-client0, libwayland-cursor0, libwayland-egl1, libxkbcommon0\n'
        'Homepage: https://github.com/ealink1/super-link\n'
        'Description: Native database, SSH and notes workspace\n'
        ' Includes an offline SQLite driver.\n')
    desktop = root / 'usr/share/applications'; desktop.mkdir(parents=True)
    desktop.joinpath('superlink.desktop').write_text(desktop_entry())
    icons = root / 'usr/share/icons/hicolor/256x256/apps'; icons.mkdir(parents=True)
    shutil.copy2(bundle / 'superlink.png', icons / 'superlink.png')
    deb = dist / f'SuperLink-{version}-linux-{arch}.deb'
    run(['dpkg-deb', '--build', '--root-owner-group', root, deb])
    run(['dpkg-deb', '--info', deb])
    return [(tar, 'tar.gz'), (deb, 'deb')]


def package(dist, binary, version, goos, arch):
    version = version_text(version)
    if (goos, arch) not in PLATFORMS:
        raise ValueError('unsupported platform')
    bundle = binary / ('SuperLink.app' if goos == 'darwin' else 'SuperLink')
    resources = bundle / 'Contents/Resources' if goos == 'darwin' else bundle
    marker = json.loads((resources / 'superlink.package.json').read_text())
    if (marker['version'], marker['os'], marker['arch']) != (version, goos, arch):
        raise ValueError('bundle identity differs from installer target')
    zip_path = dist / f'superlink_{version}_{goos}_{arch}.zip'
    if not zip_path.is_file():
        raise ValueError('build the application ZIP before installers')
    makers = {'darwin': macos_installer, 'windows': windows_installer, 'linux': linux_installers}
    with tempfile.TemporaryDirectory(prefix='superlink-package-') as name:
        products = makers[goos](bundle, dist, version, arch, Path(name))
    records = [download_record(zip_path, version, goos, arch, 'zip')]
    records += [download_record(p, version, goos, arch, fmt) for p, fmt in products]
    (dist / f'downloads-{goos}-{arch}.json').write_text(json.dumps(records, indent=2) + '\n')
    print(f'Created {len(products)} installers for {goos}/{arch}')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--dist', type=Path, default=ROOT / 'dist')
    parser.add_argument('--bin', type=Path, default=ROOT / 'bin')
    parser.add_argument('--os', choices=['darwin', 'windows', 'linux'])
    parser.add_argument('--arch', choices=['amd64', 'arm64'])
    args = parser.parse_args()
    goos = args.os or subprocess.check_output(['go', 'env', 'GOOS'], text=True).strip()
    arch = args.arch or subprocess.check_output(['go', 'env', 'GOARCH'], text=True).strip()
    package(args.dist.resolve(), args.bin.resolve(), args.version, goos, arch)


if __name__ == '__main__':
    main()
