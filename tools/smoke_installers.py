#!/usr/bin/env python3
"""Verify native installer extraction against the exact update ZIP bytes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import zipfile

from release_platforms import version_text


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, timeout=180, **kwargs)


def compare_package(directory, zip_path, goos):
    root = 'SuperLink.app/' if goos == 'darwin' else 'SuperLink/'
    with zipfile.ZipFile(zip_path) as z:
        for entry in z.infolist():
            if entry.is_dir():
                continue
            target = directory / entry.filename.removeprefix(root)
            if not target.is_file() or hashlib.sha256(target.read_bytes()).digest() != hashlib.sha256(z.read(entry)).digest():
                raise ValueError('installer content differs from verified ZIP: ' + entry.filename)


def smoke(dist, version, goos, arch):
    zip_path = dist / f'superlink_{version}_{goos}_{arch}.zip'
    with tempfile.TemporaryDirectory(prefix='superlink-installer-smoke-') as name:
        temp = Path(name)
        if goos == 'darwin':
            mount = temp / 'mount'; mount.mkdir()
            dmg = dist / f'SuperLink-{version}-macos-{arch}.dmg'
            run(['hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', mount, dmg], capture_output=True)
            try:
                compare_package(mount / 'SuperLink.app', zip_path, goos)
                if not (mount / 'Applications').is_symlink() or os.readlink(mount / 'Applications') != '/Applications':
                    raise ValueError('DMG Applications shortcut missing')
            finally:
                run(['hdiutil', 'detach', mount], capture_output=True)
        elif goos == 'linux':
            extracted = temp / 'deb'; extracted.mkdir()
            deb = dist / f'SuperLink-{version}-linux-{arch}.deb'
            run(['dpkg-deb', '--extract', deb, extracted])
            compare_package(extracted / 'opt/superlink', zip_path, goos)
            if not (extracted / 'usr/share/applications/superlink.desktop').is_file():
                raise ValueError('DEB application menu missing')
            extracted = temp / 'tar'; extracted.mkdir()
            with tarfile.open(dist / f'SuperLink-{version}-linux-{arch}.tar.gz') as archive:
                archive.extractall(extracted, filter='data')
            compare_package(extracted / 'SuperLink', zip_path, goos)
            install_home = temp / 'home'
            env = dict(os.environ, SUPERLINK_INSTALL_HOME=str(install_home), XDG_DATA_HOME=str(install_home / '.local/share'))
            run(['sh', extracted / 'SuperLink/install.sh'], env=env)
            installed = install_home / '.local/share/superlink/app'
            compare_package(installed, zip_path, goos)
            if not (install_home / '.local/bin/superlink').is_symlink():
                raise ValueError('portable application launcher missing')
            # Re-running refuses to overwrite an existing application directory.
            result = subprocess.run(['sh', str(extracted / 'SuperLink/install.sh')], env=env, capture_output=True, timeout=30)
            if result.returncode == 0:
                raise ValueError('installer silently overwrote an existing application')
        else:
            if os.environ.get('CI') != 'true':
                raise ValueError('Windows unattended installer smoke is restricted to ephemeral CI runners')
            installed = temp / 'installed'
            setup = dist / f'SuperLink-{version}-windows-{arch}-setup.exe'
            try:
                run([setup, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/DIR=' + str(installed)])
                compare_package(installed, zip_path, goos)
                actual = subprocess.check_output([str(installed / 'superlink.exe'), '--version'], text=True, timeout=30).strip()
                if actual != version:
                    raise ValueError('installed Windows application version mismatch')
            finally:
                uninstaller = installed / 'unins000.exe'
                if uninstaller.is_file():
                    run([uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'])
    print(f'Native {goos}/{arch} installer extraction and application bytes verified')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    parser.add_argument('--dist', type=Path, default=Path('dist'))
    args = parser.parse_args()
    goos = subprocess.check_output(['go', 'env', 'GOOS'], text=True).strip()
    arch = subprocess.check_output(['go', 'env', 'GOARCH'], text=True).strip()
    smoke(args.dist.resolve(), version_text(args.version), goos, arch)


if __name__ == '__main__':
    main()
