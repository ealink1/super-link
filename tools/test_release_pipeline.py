"""Release contract tests use real archives and deliberate tampering."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from package_installers import create_windows_icon, desktop_entry, package
from publish_release import verify_uploaded, upload_files
from release_notes import render, changes
from release_platforms import PLATFORMS, download_record, platform_drivers, version_text
from verify_release import verify, write_checksums

VERSION = '0.1.0'


def write_json(path, data):
    path.write_text(json.dumps(data))


def fixture(root, goos, arch):
    driver_bytes = b'test driver payload'
    root_name = 'SuperLink.app/' if goos == 'darwin' else 'SuperLink/'
    resources = root_name + 'Contents/Resources/' if goos == 'darwin' else root_name
    executables = root_name + 'Contents/MacOS/' if goos == 'darwin' else root_name
    prefix = 'Contents/MacOS/' if goos == 'darwin' else ''
    suffix = '.exe' if goos == 'windows' else ''
    marker = {'id': 'io.github.ealink1.superlink', 'version': VERSION, 'os': goos,
              'arch': arch, 'executable': prefix + 'superlink' + suffix, 'helper': prefix + 'update-helper' + suffix}
    app = root / f'superlink_{VERSION}_{goos}_{arch}.zip'
    with zipfile.ZipFile(app, 'w') as z:
        for name in ['superlink', 'update-helper']:
            info = zipfile.ZipInfo(executables + name + suffix)
            info.external_attr = (0o100755 << 16)
            z.writestr(info, b'fixture executable')
        z.writestr(resources + 'superlink.package.json', json.dumps(marker))
        z.writestr(resources + 'drivers/sqlite-driver-agent' + suffix, driver_bytes)
        z.writestr(resources + 'drivers/bundle.json', json.dumps({'schema': 1, 'os': goos, 'arch': arch,
                   'drivers': [{'type': 'sqlite', 'sha256': hashlib.sha256(driver_bytes).hexdigest()}]}))
    app_record = download_record(app, VERSION, goos, arch, 'zip')
    assets = [dict(app_record, kind='app', id='superlink')]
    assets[0].pop('format')
    for driver in platform_drivers(goos, arch):
        p = root / f'{driver}-agent_{VERSION}_{goos}_{arch}{suffix}'
        p.write_bytes(driver_bytes)
        record = download_record(p, VERSION, goos, arch, 'agent')
        record.pop('format')
        assets.append(dict(record, kind='driver', id=driver, revision='reviewed', protocol='json-lines-v2'))
    write_json(root / f'assets-{goos}-{arch}.json', assets)
    formats = {'darwin': ['dmg'], 'windows': ['setup'], 'linux': ['tar.gz', 'deb']}[goos]
    downloads = [app_record]
    for fmt in formats:
        p = root / f'SuperLink-{VERSION}-{goos}-{arch}.{fmt}'
        p.write_bytes(b'installer fixture')
        downloads.append(download_record(p, VERSION, goos, arch, fmt))
    write_json(root / f'downloads-{goos}-{arch}.json', downloads)
    write_json(root / f'build-info-{goos}-{arch}.json', {'version': VERSION, 'os': goos, 'arch': arch, 'releasePublicKey': ''})


class ReleaseContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for goos, arch in PLATFORMS:
            fixture(self.root, goos, arch)

    def test_complete_matrix_and_notes(self):
        downloads = verify(self.root, VERSION, public_key='')
        self.assertEqual(len(downloads), 14)
        notes = render(VERSION, downloads, {'feat': ['系统字体'], 'fix': [], 'perf': [], 'other': []})
        self.assertEqual(notes.count('| ZIP'), 0)
        for d in downloads:
            self.assertIn(d['url'], notes)
        self.assertIn('Windows ARM64 暂不提供 DuckDB', notes)
        self.assertIn('在线更新与驱动安装不可用', notes)
        write_checksums(self.root)
        before = (self.root / 'SHA256SUMS.txt').read_bytes()
        write_checksums(self.root)
        self.assertEqual(before, (self.root / 'SHA256SUMS.txt').read_bytes())

    def test_missing_platform_rejected(self):
        (self.root / 'downloads-linux-arm64.json').unlink()
        with self.assertRaises(FileNotFoundError):
            verify(self.root, VERSION)

    def test_corruption_rejected(self):
        p = self.root / f'sqlite-agent_{VERSION}_linux_arm64'
        p.write_bytes(b'corrupt')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            verify(self.root, VERSION)

    def test_stale_version_and_key_rejected(self):
        with self.assertRaisesRegex(ValueError, 'stale'):
            verify(self.root, '0.2.0')
        with self.assertRaisesRegex(ValueError, 'key mismatch'):
            verify(self.root, VERSION, public_key='different')

    def test_missing_driver_rejected(self):
        p = self.root / 'assets-darwin-arm64.json'
        assets = json.loads(p.read_text()); assets.pop()
        write_json(p, assets)
        with self.assertRaisesRegex(ValueError, 'inventory incomplete'):
            verify(self.root, VERSION)

    def test_path_traversal_rejected_even_after_rehash(self):
        p = self.root / f'superlink_{VERSION}_darwin_arm64.zip'
        with zipfile.ZipFile(p, 'a') as archive:
            archive.writestr('SuperLink.app/../../evil', b'payload')
        for filename in ['assets-darwin-arm64.json', 'downloads-darwin-arm64.json']:
            q = self.root / filename; records = json.loads(q.read_text())
            records[0].update(download_record(p, VERSION, 'darwin', 'arm64', 'zip'))
            write_json(q, records)
        with self.assertRaisesRegex(ValueError, 'unsafe application ZIP'):
            verify(self.root, VERSION)

    def test_bundled_driver_tampering_rejected(self):
        p = self.root / f'superlink_{VERSION}_linux_arm64.zip'
        with zipfile.ZipFile(p) as z:
            entries = [(i, z.read(i)) for i in z.infolist()]
        with zipfile.ZipFile(p, 'w') as z:
            for info, data in entries:
                if info.filename.endswith('sqlite-driver-agent'):
                    data = b'tampered'
                z.writestr(info, data)
        q = self.root / 'assets-linux-arm64.json'; records = json.loads(q.read_text())
        records[0].update(download_record(p, VERSION, 'linux', 'arm64', 'zip')); write_json(q, records)
        with self.assertRaisesRegex(ValueError, 'bundled SQLite checksum'):
            verify(self.root, VERSION)

    def test_github_uploaded_digest_verification(self):
        p = self.root / 'asset.bin'; p.write_bytes(b'uploaded')
        from release_platforms import digest
        pages = [[{'name': p.name, 'size': p.stat().st_size, 'digest': 'sha256:' + digest(p)}]]
        verify_uploaded([p], pages)
        pages[0][0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaisesRegex(ValueError, 'SHA256'):
            verify_uploaded([p], pages)

    def test_invalid_versions_and_platform_support(self):
        for value in ['../v0.1.0', 'v0.1.0\noutput=bad', '01.2.3', '1.0.0-..', '1.0.0-01', 'latest']:
            with self.assertRaises(ValueError): version_text(value)
        self.assertNotIn('duckdb', platform_drivers('windows', 'arm64'))
        self.assertIn('sqlite', platform_drivers('windows', 'arm64'))
        self.assertIn('duckdb', platform_drivers('linux', 'arm64'))

    def test_package_wrong_bundle_target_rejected(self):
        binary = self.root / 'bin'
        resources = binary / 'SuperLink.app/Contents/Resources'; resources.mkdir(parents=True)
        write_json(resources / 'superlink.package.json', {'version': '0.2.0', 'os': 'darwin', 'arch': 'arm64'})
        with self.assertRaisesRegex(ValueError, 'bundle identity'):
            package(self.root, binary, VERSION, 'darwin', 'arm64')

    def test_unknown_upload_files_rejected(self):
        (self.root / 'RELEASE_NOTES.md').write_text('release')
        write_checksums(self.root)
        self.assertTrue(upload_files(self.root, VERSION))
        (self.root / 'private.key').write_text('must never be uploaded')
        write_checksums(self.root)
        with self.assertRaisesRegex(ValueError, 'unexpected'):
            upload_files(self.root, VERSION)

    def test_linux_user_install_and_repeat_protection(self):
        import os, subprocess, shutil
        if os.name == 'nt':
            self.skipTest('Linux shell installer is verified on POSIX hosts')
        source = self.root / 'portable'; source.mkdir()
        source.joinpath('superlink').write_text('application')
        source.joinpath('superlink.png').write_text('icon')
        script = Path(__file__).resolve().parent.parent / 'packaging/install-linux.sh'
        shutil.copy2(script, source / 'install.sh')
        home = self.root / 'user home'
        env = dict(os.environ, SUPERLINK_INSTALL_HOME=str(home), XDG_DATA_HOME=str(home / 'data'))
        subprocess.run(['sh', str(source / 'install.sh')], env=env, check=True, capture_output=True)
        self.assertEqual((home / 'data/superlink/app/superlink').read_text(), 'application')
        self.assertTrue((home / '.local/bin/superlink').is_symlink())
        self.assertIn(f'Exec="{home}/data/superlink/app/superlink"', (home / 'data/applications/superlink.desktop').read_text())
        (home / 'data/superlink/app/superlink').write_text('existing version')
        repeat = subprocess.run(['sh', str(source / 'install.sh')], env=env, capture_output=True)
        self.assertNotEqual(repeat.returncode, 0)
        self.assertEqual((home / 'data/superlink/app/superlink').read_text(), 'existing version')

    def test_platform_branding(self):
        icon = self.root / 'icon.ico'; create_windows_icon(icon)
        self.assertEqual(icon.read_bytes()[:6], b'\x00\x00\x01\x00\x01\x00')
        self.assertIn('Exec=/opt/superlink/superlink', desktop_entry())

    def test_commit_classification_and_markdown_escape(self):
        subjects = '✨ feat(ui): 系统字体\n🐛 fix: [link](bad)\n⚡ perf: faster\nchore: build\n'
        with patch('release_notes.subprocess.run'), patch('release_notes.subprocess.check_output', return_value=subjects):
            groups = changes('HEAD')
        self.assertEqual(groups['feat'], ['系统字体'])
        self.assertEqual(groups['fix'], [r'\[link\](bad)'])
        self.assertEqual(groups['perf'], ['faster'])


if __name__ == '__main__':
    unittest.main()
