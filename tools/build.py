#!/usr/bin/env python3
"""Build native Rust/GPUI application and independently versioned driver agents."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import base64
import re
import shutil
import subprocess
import sys
import zipfile
import tomllib
import platform

ROOT = Path(__file__).resolve().parent.parent
from release_platforms import DRIVER_BASE_URL, DRIVERS, platform_drivers, version_text



def run(args, **kwargs):
    subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--all-drivers', action='store_true')
    parser.add_argument('--application-only', action='store_true')
    parser.add_argument('--driver', action='append', choices=DRIVERS)
    parser.add_argument('--version', default=tomllib.loads((ROOT/'rust/Cargo.toml').read_text())['workspace']['package']['version'])
    parser.add_argument('--package', action='store_true')
    parser.add_argument('--dist', type=Path, default=ROOT / 'dist')
    parser.add_argument('--binary-dir', type=Path, default=ROOT / 'bin')
    parser.add_argument('--skip-app', action='store_true')
    args = parser.parse_args()
    if args.application_only and (args.all_drivers or args.driver or args.skip_app or not args.package):
        parser.error('--application-only requires --package and the default bundled SQLite')
    goos = {'Darwin':'darwin','Windows':'windows','Linux':'linux'}.get(platform.system())
    # Windows ARM64 runner 的 platform.machine() 返回大写 'ARM64'，统一小写后再映射。
    machine = platform.machine().lower()
    arch = {'arm64':'arm64','aarch64':'arm64','x86_64':'amd64','amd64':'amd64'}.get(machine)
    if not goos or not arch:
        raise SystemExit('unsupported native build platform')
    suffix = '.exe' if goos == 'windows' else ''
    binary = args.binary_dir.resolve()
    agents = binary / 'drivers'
    agents.mkdir(parents=True, exist_ok=True)
    artifacts, records = [], []
    previous = agents / 'bundle.json'
    if previous.is_file():
        bundle = json.loads(previous.read_text())
        if bundle.get('os') == goos and bundle.get('arch') == arch:
            for record in bundle.get('drivers', []):
                name = record.get('type', '')
                if name not in DRIVERS or not record.get('revision','').startswith('rust-'):
                    continue
                existing = agents / f'{name}-driver-agent{suffix}'
                if existing.is_file() and hashlib.sha256(existing.read_bytes()).hexdigest() == record.get('sha256'):
                    records.append(record)
    available = platform_drivers(goos, arch)
    chosen = available if args.all_drivers else (args.driver or ['sqlite'])
    if any(driver not in available for driver in chosen):
        raise SystemExit('requested driver is unavailable on this platform')
    if len(chosen) != len(set(chosen)):
        raise SystemExit('duplicate driver selection')
    if args.all_drivers and len(available) != len(DRIVERS):
        print('DuckDB is unavailable on Windows ARM64; other drivers will be built.', flush=True)
    version = version_text(args.version)
    public_key = os.environ.get('SUPERLINK_RELEASE_PUBLIC_KEY','').strip()
    if public_key and len(base64.b64decode(public_key,validate=True)) != 32:
        raise SystemExit('release public key must be a base64 Ed25519 32-byte key')
    release_url = f'https://github.com/ealink1/super-link/releases/download/v{version}/'
    dist = args.dist.resolve()
    dist.mkdir(parents=True, exist_ok=True)
    run(['cargo','build','--locked','--manifest-path','rust/Cargo.toml','--release','-p','superlink-driver-agent','-j','2'])
    rust_agent = ROOT/'rust/target/release'/f'superlink-driver-agent{suffix}'
    for driver in chosen:
        print(f'Building {driver} agent ({goos}/{arch})', flush=True)
        output = agents / f'{driver}-driver-agent{suffix}'
        if output.exists():
            output.unlink()
        try:
            os.link(rust_agent,output)
        except OSError:
            shutil.copy2(rust_agent,output)
        probe = subprocess.run([str(output)], input='{"id":1,"method":"metadata"}\n',
                               capture_output=True, text=True, timeout=45, check=True)
        response = json.loads(probe.stdout.strip())
        if not response.get('success'):
            raise RuntimeError(f'{driver} agent metadata probe failed')
        metadata = response['data']
        if metadata.get('driverType') != driver or metadata.get('protocolSchema') != 'json-lines-v3-rust' or metadata.get('implementation') != 'rust':
            raise RuntimeError(f'{driver} agent protocol/identity mismatch')
        digest = hashlib.sha256(output.read_bytes()).hexdigest()
        record = {'type':driver,'sha256':digest,'revision':metadata['agentRevision'],
                  'protocol':metadata['protocolSchema'],'source':'application-bundle'}
        records = [item for item in records if item["type"] != driver]
        records.append(record)
        if not args.application_only:
            filename = f'{driver}-agent_{version}_{goos}_{arch}{suffix}'
            shutil.copy2(output, dist / filename)
            artifacts.append({'id':driver,'kind':'driver','os':goos,'arch':arch,'filename':filename,
                              'url':DRIVER_BASE_URL+'v'+version+'/'+filename,'size':output.stat().st_size,'sha256':digest,
                              'revision':record['revision'],'protocol':record['protocol']})
    (agents / 'bundle.json').write_text(json.dumps({'schema':1,'os':goos,'arch':arch,'drivers':records},indent=2)+'\n')
    if not args.skip_app:
        environment = os.environ.copy()
        environment['SUPERLINK_RELEASE_PUBLIC_KEY'] = public_key
        environment['SUPERLINK_VERSION'] = version
        run(['cargo', 'build', '--locked', '--manifest-path', 'rust/Cargo.toml', '--release', '-p', 'superlink', '-p', 'superlink-update-helper', '-j', '2'], env=environment)
        for name in ['superlink', 'update-helper']:
            shutil.copy2(ROOT/'rust/target/release'/f'{name}{suffix}', binary/f'{name}{suffix}')

    if args.package:
        if args.skip_app:
            raise RuntimeError('--package requires the application build')
        package_root = binary / ('SuperLink.app' if goos=='darwin' else 'SuperLink')
        if package_root.exists():
            shutil.rmtree(package_root)
        executable_dir = package_root / 'Contents/MacOS' if goos=='darwin' else package_root
        resources = package_root / 'Contents/Resources' if goos=='darwin' else package_root
        executable_dir.mkdir(parents=True, exist_ok=True)
        resources.mkdir(parents=True, exist_ok=True)
        for name in [f'superlink{suffix}',f'update-helper{suffix}']:
            shutil.copy2(binary/name,executable_dir/name)
        # SQLite is offline in every application package; optional agents remain
        # separate release assets. The unified Rust agent contains all protocol libraries.
        bundled = [record for record in records if record['type']=='sqlite']
        if not bundled:
            raise RuntimeError('an application package must include SQLite')
        destination=resources/'drivers';destination.mkdir()
        shutil.copy2(agents/f'sqlite-driver-agent{suffix}',destination/f'sqlite-driver-agent{suffix}')
        (destination/'bundle.json').write_text(json.dumps({'schema':1,'os':goos,'arch':arch,'drivers':bundled},indent=2)+'\n')
        shutil.copytree(ROOT/'assets', resources/'assets')
        for name in ['LICENSE','NOTICE','UPSTREAM.md','THIRD_PARTY_NOTICES.md']:
            shutil.copy2(ROOT/name,resources/name)
        license_dir=resources/'licenses';license_dir.mkdir()
        shutil.copy2(ROOT/'assets/OFL.txt',license_dir/'NotoSansSC-OFL.txt')
        for source in (ROOT/'third_party/fonts').glob('*LICENSE*'):
            shutil.copy2(source, license_dir/source.name)
        for source in (ROOT/'third_party/fonts').glob('*OFL*'):
            shutil.copy2(source, license_dir/source.name)
        shutil.copy2(ROOT/'assets/superlink/sources.json', resources/'superlink-ui-assets.json')
        shutil.copy2(ROOT/'assets/shell/sources.json', resources/'shell-ui-assets.json')
        shutil.copy2(ROOT/'assets/shell/LICENSE', license_dir/'Lucide-LICENSE.txt')
        shutil.copy2(ROOT/'assets/branding/superlink.png', resources/'superlink.png')
        for item in (ROOT/'third_party/protocol-licenses').glob('*'):
            if item.is_file():
                shutil.copy2(item,license_dir/item.name)
        if goos=='darwin':
            import plistlib
            shutil.copy2(ROOT/'assets/branding/superlink.icns', resources/'SuperLink.icns')
            (package_root/'Contents/Info.plist').write_bytes(plistlib.dumps({
                'CFBundleName':'SuperLink','CFBundleDisplayName':'SuperLink',
                'CFBundleIdentifier':'io.github.ealink1.superlink','CFBundleExecutable':'superlink',
                'CFBundleIconFile':'SuperLink.icns',
                'CFBundlePackageType':'APPL','CFBundleShortVersionString':version,
                'CFBundleVersion':version,'NSHighResolutionCapable':True,
                'NSLocalNetworkUsageDescription':'SuperLink 需要访问本地网络，以连接你配置的数据库、SSH 服务器及其他数据源。',
                'NSHumanReadableCopyright':'Apache-2.0; see included licenses'}))
        prefix='Contents/MacOS/' if goos=='darwin' else ''
        marker={'id':'io.github.ealink1.superlink','version':version,'os':goos,'arch':arch,
                'executable':prefix+f'superlink{suffix}','helper':prefix+f'update-helper{suffix}'}
        (resources/'superlink.package.json').write_text(json.dumps(marker,indent=2)+'\n')
        # Distribution signing/notarization happens before zip creation in the
        # release workflow when its platform signing credentials are available.
        if goos=='darwin':
            identity=os.environ.get('SUPERLINK_MAC_SIGN_IDENTITY') or '-'
            signing_options=['--options','runtime','--timestamp'] if identity != '-' else []
            for target in [destination/'sqlite-driver-agent',executable_dir/'update-helper',executable_dir/'superlink']:
                run(['codesign','--force',*signing_options,'--identifier',
                     'io.github.ealink1.superlink.'+target.name,'--sign',identity,str(target)])
            # Signing changes executable bytes. The package's trusted bundle
            # checksum must describe the signed copy before sealing the bundle.
            bundled = [dict(record) for record in bundled]
            bundled[0]['sha256'] = hashlib.sha256((destination/'sqlite-driver-agent').read_bytes()).hexdigest()
            (destination/'bundle.json').write_text(json.dumps({'schema':1,'os':goos,'arch':arch,'drivers':bundled},indent=2)+'\n')
            # Seal Info.plist and resources even without a distribution certificate.
            # Ad-hoc signing does not provide a persistent privacy identity across
            # updates; Apple-issued signing credentials are still recommended.
            run(['codesign','--force',*signing_options,'--identifier',
                 'io.github.ealink1.superlink','--sign',identity,str(package_root)])
            run(['codesign','--verify','--deep','--strict',str(package_root)])
        filename=f'SuperLink_{version}_{goos}_{arch}.zip'
        output=dist/filename
        with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
            for item in sorted(package_root.rglob('*')):
                if item.is_file():
                    archive.write(item,item.relative_to(binary).as_posix())
        artifacts.append({'id':'superlink','kind':'app','os':goos,'arch':arch,'filename':filename,
                          'url':release_url+filename,'size':output.stat().st_size,
                          'sha256':hashlib.sha256(output.read_bytes()).hexdigest()})
    (dist/f'build-info-{goos}-{arch}.json').write_text(json.dumps({'version': version, 'os': goos, 'arch': arch, 'releasePublicKey': public_key, 'source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()}, indent=2)+'\n')
    (dist/f'assets-{goos}-{arch}.json').write_text(json.dumps(artifacts,indent=2)+'\n')
    print(f'Built {len(chosen)} driver(s). Artifacts: {dist}',flush=True)


if __name__=='__main__':
    main()
