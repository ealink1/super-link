#!/usr/bin/env python3
"""Build/restart the repository's application using the shared local workspace."""
import argparse
import os
from pathlib import Path
import plistlib
import signal
import platform
import time
import shutil
import subprocess
import json
from release_platforms import digest, platform_drivers

ROOT = Path(__file__).resolve().parent.parent
DEV = ROOT / 'bin' / 'dev'


def owned_children(root_pid):
    """Record descendants before shutdown, including start times against PID reuse."""
    rows = {}
    for line in subprocess.check_output(['ps', '-axo', 'pid=,ppid=,lstart='], text=True).splitlines():
        parts = line.split(maxsplit=2)
        if len(parts) == 3:
            rows[int(parts[0])] = (int(parts[1]), parts[2])
    pending, found = [root_pid], {}
    while pending:
        parent = pending.pop()
        for pid, (owner, started) in rows.items():
            if owner == parent and pid not in found:
                found[pid] = started
                pending.append(pid)
    return found


def release_children(children):
    """Release only the previously verified development process's descendants."""
    def unchanged(pid, started):
        probe = subprocess.run(['ps', '-p', str(pid), '-o', 'lstart=,stat='], text=True, capture_output=True)
        text = probe.stdout.strip()
        return probe.returncode == 0 and text.startswith(started) and 'Z' not in text[len(started):]

    for sig, rounds in [(signal.SIGTERM, 40), (signal.SIGKILL, 1)]:
        for pid, started in children.items():
            if unchanged(pid, started):
                try:
                    os.kill(pid, sig)
                except ProcessLookupError:
                    pass
        for _ in range(rounds):
            if not any(unchanged(pid, started) for pid, started in children.items()):
                return
            time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--no-build', action='store_true')
    parser.add_argument('--isolated-data', action='store_true',
                        help='use bin/dev/data instead of the shared application workspace')
    args = parser.parse_args()
    if not args.no_build:
        subprocess.run(['cargo', 'build', '--locked', '--manifest-path', str(ROOT / 'rust/Cargo.toml'),
                        '-p', 'superlink', '-p', 'superlink-driver-agent', '-j', '2'], cwd=ROOT, check=True)
        agent = ROOT / 'rust/target/debug' / ('superlink-driver-agent.exe' if os.name == 'nt' else 'superlink-driver-agent')
        agent_digest = digest(agent)
        agents = ROOT / 'bin/drivers'
        agents.mkdir(parents=True, exist_ok=True)
        system = {'Darwin': 'darwin', 'Windows': 'windows', 'Linux': 'linux'}[platform.system()]
        arch = {'arm64': 'arm64', 'aarch64': 'arm64', 'x86_64': 'amd64', 'AMD64': 'amd64'}[platform.machine()]
        records = []
        for kind in platform_drivers(system, arch):
            output = agents / (kind + '-driver-agent' + ('.exe' if os.name == 'nt' else ''))
            if output.exists():
                output.unlink()
            try:
                os.link(agent, output)
            except OSError:
                shutil.copy2(agent, output)
            probe = subprocess.run([str(output)], input='{"id":1,"method":"metadata"}\n', text=True, capture_output=True, check=True, timeout=15)
            metadata = json.loads(probe.stdout)['data']
            if metadata.get('implementation') != 'rust' or metadata.get('driverType') != kind or metadata.get('protocolSchema') != 'json-lines-v3-rust':
                raise RuntimeError('development driver identity/protocol mismatch')
            records.append({'type': kind, 'sha256': agent_digest, 'revision': metadata['agentRevision'], 'protocol': metadata['protocolSchema'], 'source': 'application-bundle'})
        system = {'Darwin': 'darwin', 'Windows': 'windows', 'Linux': 'linux'}[platform.system()]
        arch = {'arm64': 'arm64', 'aarch64': 'arm64', 'x86_64': 'amd64', 'AMD64': 'amd64'}[platform.machine()]
        (agents / 'bundle.json').write_text(json.dumps({'schema': 1, 'os': system, 'arch': arch, 'drivers': records}, indent=2) + '\n')
    source = ROOT / 'rust/target/debug' / ('superlink.exe' if os.name == 'nt' else 'superlink')
    DEV.mkdir(parents=True, exist_ok=True)
    executable = source
    if platform.system() == 'Darwin':
        bundle = DEV / 'SuperLink.app' / 'Contents'
        (bundle / 'MacOS').mkdir(parents=True, exist_ok=True)
        executable = bundle / 'MacOS/superlink'
        plist = {'CFBundleIdentifier': 'io.github.ealink1.superlink.development',
                 'CFBundleName': 'SuperLink', 'CFBundleDisplayName': 'SuperLink',
                 'CFBundleExecutable': 'superlink', 'CFBundlePackageType': 'APPL',
                 'CFBundleShortVersionString': '0.2.0', 'NSHighResolutionCapable': True}
        (bundle / 'Info.plist').write_bytes(plistlib.dumps(plist))
    pid_file = DEV / 'superlink.pid'
    if pid_file.exists():
        try:
            pid = int(pid_file.read_text())
            command = subprocess.check_output(['ps', '-p', str(pid), '-o', 'command='], text=True).strip()
            if command == str(executable):
                children = owned_children(pid)
                os.kill(pid, signal.SIGTERM)
                for _ in range(100):
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        break
                    time.sleep(0.05)
                else:
                    raise RuntimeError("owned development process did not stop; leaving executable intact")
                release_children(children)
        except (ValueError, ProcessLookupError, subprocess.CalledProcessError):
            pass
    if executable != source:
        shutil.copy2(source, executable)
    environment = os.environ.copy()
    if args.isolated_data:
        environment['SUPERLINK_DATA_ROOT'] = str(DEV / 'data')
    environment['SUPERLINK_DRIVERS_DIR'] = str(ROOT / 'bin/drivers')
    with (DEV / 'superlink.log').open('ab') as log:
        process = subprocess.Popen([str(executable)], cwd=ROOT, env=environment,
                                   stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                                   start_new_session=True)
    # A busy workspace exits immediately; do not report a dead PID as a restart.
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass
    else:
        raise RuntimeError('开发版未能启动，请检查 bin/dev/superlink.log；共用工作区前须退出原版应用')
    pid_file.write_text(str(process.pid))
    workspace = environment.get('SUPERLINK_DATA_ROOT', '系统默认 SuperLink 工作区')
    print(f'SuperLink development PID {process.pid}; workspace {workspace}')


if __name__ == '__main__':
    main()
