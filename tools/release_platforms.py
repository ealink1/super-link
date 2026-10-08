"""Shared release contract. Runtime updates continue using the application ZIP."""
import hashlib
import re
from pathlib import Path

PLATFORMS = [('darwin', 'amd64'), ('darwin', 'arm64'), ('windows', 'amd64'),
             ('windows', 'arm64'), ('linux', 'amd64'), ('linux', 'arm64')]
DRIVERS = ['mariadb', 'oceanbase', 'diros', 'starrocks', 'sphinx', 'sqlserver',
           'sqlite', 'duckdb', 'dameng', 'kingbase', 'highgo', 'vastbase',
           'opengauss', 'gaussdb', 'iris', 'cache', 'mongodb', 'tdengine',
           'iotdb', 'clickhouse', 'elasticsearch', 'trino']
LABELS = {'darwin': 'macOS', 'windows': 'Windows', 'linux': 'Linux'}
APP_REPO = 'ealink1/super-link'
DRIVER_REPO = 'ealink1/SuperLink-DriverAgents'
BASE_URL = f'https://github.com/{APP_REPO}/releases/download/'
DRIVER_BASE_URL = f'https://github.com/{DRIVER_REPO}/releases/download/'


def version_text(value):
    version = value.removeprefix('v')
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?', version):
        raise ValueError('use a semantic version such as 0.1.0 or 0.1.0-rc.1')
    if '-' in version:
        identifiers = version.split('-', 1)[1].split('.')
        if any(not part or (part.isdigit() and len(part) > 1 and part.startswith('0')) for part in identifiers):
            raise ValueError('invalid semantic prerelease identifier')
    return version


def platform_drivers(goos, arch):
    if (goos, arch) not in PLATFORMS:
        raise ValueError('unsupported release platform')
    return [d for d in DRIVERS if not (d == 'duckdb' and (goos, arch) == ('windows', 'arm64'))]


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def download_record(path, version, goos, arch, format_name):
    path = Path(path)
    return {'os': goos, 'arch': arch, 'format': format_name, 'filename': path.name,
            'size': path.stat().st_size, 'sha256': digest(path),
            'url': (DRIVER_BASE_URL if format_name == 'agent' else BASE_URL) + 'v' + version + '/' + path.name}
