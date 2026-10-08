"""Exercise app-only release contracts, conservative planning and tampering."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import driver_reuse
import prepare_release
import publish_release
import test_release_pipeline as fixtures
from release_platforms import APP_REPO, DRIVER_REPO, PLATFORMS, digest
from verify_release import verify, write_checksums

TARGET = '0.2.0'
SOURCE = 'b' * 40


class DriverReuseContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        records = []
        for goos, arch in PLATFORMS:
            fixtures.fixture(self.root, goos, arch)
            assets = json.loads((self.root / f'assets-{goos}-{arch}.json').read_text())
            records.extend(a for a in assets if a['kind'] == 'driver')
        for path in self.root.iterdir():
            path.unlink()
        for goos, arch in PLATFORMS:
            with patch.object(fixtures, 'VERSION', TARGET):
                fixtures.fixture(self.root, goos, arch)
            path = self.root / f'assets-{goos}-{arch}.json'
            assets = json.loads(path.read_text())
            for asset in assets:
                if asset['kind'] == 'driver':
                    (self.root / asset['filename']).unlink()
            assets = [a for a in assets if a['kind'] == 'app'] + [a for a in records if (a['os'], a['arch']) == (goos, arch)]
            fixtures.write_json(path, assets)
            fixtures.write_json(self.root / f'build-info-{goos}-{arch}.json',
                                {'version': TARGET, 'os': goos, 'arch': arch, 'source': SOURCE, 'releasePublicKey': 'trust-key'})
        fixtures.write_json(self.root / driver_reuse.FILES[1], {'version': '0.1.0', 'artifacts': records})
        (self.root / driver_reuse.FILES[2]).write_text('signed fixture')
        self.proof = {'schema': 1, 'base_app_version': '0.1.0', 'driver_version': '0.1.0',
                      'base_source': 'a' * 40, 'source': SOURCE, 'target_version': TARGET,
                      'public_key': 'trust-key', 'manifest_sha256': digest(self.root / driver_reuse.FILES[1]),
                      'signature_sha256': digest(self.root / driver_reuse.FILES[2])}
        fixtures.write_json(self.root / driver_reuse.FILES[0], self.proof)
        self.auth = patch.object(driver_reuse, 'authenticate')
        self.auth.start()
        self.addCleanup(self.auth.stop)

    def test_complete_matrix_without_local_driver_binaries(self):
        self.assertEqual(len(verify(self.root, TARGET, public_key='trust-key')), 14)
        (self.root / 'RELEASE_NOTES.md').write_text('release notes')
        write_checksums(self.root)
        with tempfile.TemporaryDirectory() as directory:
            stage = Path(directory) / 'app'
            files = publish_release.stage_release(self.root, TARGET, APP_REPO, stage)
            self.assertEqual(len(files), 16)  # 14 installers, notes, checksums (unsigned fixture)
            self.assertFalse(any('agent_' in f.name or f.name in driver_reuse.FILES for f in files))
        (self.root / 'secret.key').write_text('unexpected')
        with self.assertRaisesRegex(ValueError, 'unexpected'):
            publish_release.upload_files(self.root, TARGET)

    def test_changed_record_missing_driver_source_key_and_digest_rejected(self):
        path = self.root / 'assets-linux-amd64.json'
        original = path.read_text()
        assets = json.loads(original)
        assets[1]['sha256'] = 'f' * 64
        fixtures.write_json(path, assets)
        with self.assertRaisesRegex(ValueError, 'signed base'):
            verify(self.root, TARGET)
        assets.pop(1)
        fixtures.write_json(path, assets)
        with self.assertRaisesRegex(ValueError, 'inventory incomplete'):
            verify(self.root, TARGET)
        path.write_text(original)
        with self.assertRaisesRegex(ValueError, 'key mismatch'):
            verify(self.root, TARGET, public_key='different')
        info = self.root / 'build-info-linux-amd64.json'
        data = json.loads(info.read_text()); data['source'] = 'c' * 40
        fixtures.write_json(info, data)
        with self.assertRaisesRegex(ValueError, 'source or key'):
            verify(self.root, TARGET)
        (self.root / driver_reuse.FILES[1]).write_text('{}')
        with self.assertRaisesRegex(ValueError, 'digest mismatch'):
            driver_reuse.load(self.root, TARGET)

    def test_incomplete_unknown_proof_and_remote_corruption_rejected(self):
        self.proof['unknown'] = 'field'
        fixtures.write_json(self.root / driver_reuse.FILES[0], self.proof)
        with self.assertRaisesRegex(ValueError, 'identity'):
            driver_reuse.load(self.root, TARGET)
        del self.proof['unknown']; fixtures.write_json(self.root / driver_reuse.FILES[0], self.proof)
        reuse = driver_reuse.load(self.root, TARGET)
        remote = [{'name': a['filename'], 'size': a['size'], 'digest': 'sha256:' + a['sha256']} for a in reuse[1].values()]
        info = {'id': 123, 'draft': False, 'prerelease': False, 'tag_name': 'v0.1.0'}
        with patch.object(driver_reuse, 'gh_json', side_effect=[info, remote[:100], remote[100:]]):
            driver_reuse.verify_remote(reuse)
        remote[0]['digest'] = 'sha256:' + 'f' * 64
        with patch.object(driver_reuse, 'gh_json', side_effect=[info, remote[:100], remote[100:]]):
            with self.assertRaisesRegex(ValueError, 'checksum'):
                driver_reuse.verify_remote(reuse)
        (self.root / driver_reuse.FILES[0]).unlink()
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            driver_reuse.load(self.root, TARGET)

    def test_publish_reuse_never_mutates_driver_repository(self):
        (self.root / 'RELEASE_NOTES.md').write_text('notes')
        (self.root / 'manifest.json').write_text('{}')
        (self.root / 'manifest.json.sig').write_text('new signature')
        write_checksums(self.root)
        with patch('sys.argv', ['publish', '--version', TARGET, '--source', SOURCE, '--dist', str(self.root)]), \
             patch.dict(os.environ, {'CI': 'true', 'SUPERLINK_PUBLISH': 'true', 'SUPERLINK_DRIVER_RELEASE_TOKEN': ''}), \
             patch.object(publish_release, 'verify_remote') as remote, \
             patch.object(publish_release, 'driver_tag_source') as tag, \
             patch.object(publish_release, 'create_draft', return_value={'id': 5}) as draft, \
             patch.object(publish_release, 'publish_draft') as publish:
            publish_release.main()
        remote.assert_called_once()
        tag.assert_not_called()
        self.assertEqual(draft.call_count, 1)
        self.assertEqual(draft.call_args.args[0], APP_REPO)
        publish.assert_called_once_with(APP_REPO, 'v' + TARGET, {'id': 5})

    def test_unavailable_base_falls_back_and_removes_partial_inputs(self):
        output = self.root / 'job-output'
        with patch('sys.argv', ['prepare', '--version', TARGET, '--source', SOURCE,
                                '--directory', str(self.root / 'partial')]), \
             patch.dict(os.environ, {'GITHUB_OUTPUT': str(output)}), \
             patch.object(prepare_release, 'prepare', side_effect=ValueError('invalid base')):
            partial = self.root / 'partial'; partial.mkdir()
            (partial / 'manifest.json').write_text('incomplete')
            prepare_release.main()
        self.assertFalse(partial.exists())
        self.assertEqual(output.read_text(), 'reuse_drivers=false\n')

    def test_release_notes_link_original_driver_version(self):
        from release_notes import render
        text = render(TARGET, verify(self.root, TARGET), {}, signed=True, driver_version='0.1.0')
        self.assertIn(f'https://github.com/{DRIVER_REPO}/releases/tag/v0.1.0', text)
        self.assertNotIn(f'https://github.com/{DRIVER_REPO}/releases/tag/v{TARGET}', text)

    def test_planner_allowlist_renames_and_ancestry(self):
        with patch.object(prepare_release, 'git', return_value='internal/ui/tab.go\0docs/release.md'), \
             patch.object(prepare_release.subprocess, 'run'):
            self.assertTrue(prepare_release.eligible('a' * 40, SOURCE))
        for path in ['go.mod', 'cmd/driver-agent/main.go', 'internal/infra/release/client.go',
                     'tools/build.py', '.github/workflows/build.yml', 'internal/upstream/db/db.go',
                     'internal/ui/new.go\0internal/upstream/old.go']:
            with patch.object(prepare_release, 'git', return_value=path), patch.object(prepare_release.subprocess, 'run'):
                self.assertFalse(prepare_release.eligible('a' * 40, SOURCE), path)
        with patch.object(prepare_release.subprocess, 'run', side_effect=subprocess.CalledProcessError(1, 'git')):
            with self.assertRaises(subprocess.CalledProcessError):
                prepare_release.eligible('a' * 40, SOURCE)


if __name__ == '__main__':
    unittest.main()
