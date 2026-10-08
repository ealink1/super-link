#!/usr/bin/env python3
"""Build native Fyne application and independently versioned driver agents."""
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

ROOT = Path(__file__).resolve().parent.parent
from release_platforms import DRIVER_BASE_URL, DRIVERS, platform_drivers, version_text



def run(args, **kwargs):
    subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--all-drivers', action='store_true')
    parser.add_argument('--driver', action='append', choices=DRIVERS)
    parser.add_argument('--version', default='0.1.0')
    parser.add_argument('--package', action='store_true')
    parser.add_argument('--skip-app', action='store_true')
    args = parser.parse_args()
    goos = subprocess.check_output(['go','env','GOOS'], cwd=ROOT, text=True).strip()
    arch = subprocess.check_output(['go','env','GOARCH'], cwd=ROOT, text=True).strip()
    suffix = '.exe' if goos == 'windows' else ''
    binary = ROOT / 'bin'
    agents = binary / 'drivers'
    agents.mkdir(parents=True, exist_ok=True)
    artifacts, records = [], []
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
    dist = ROOT / 'dist'
    dist.mkdir(exist_ok=True)
    for driver in chosen:
        print(f'Building {driver} agent ({goos}/{arch})', flush=True)
        output = agents / f'{driver}-driver-agent{suffix}'
        run(['go','build','-trimpath','-ldflags','-s -w','-tags',f'gonavi_{driver}_driver','-o',str(output),'./cmd/driver-agent'])
        probe = subprocess.run([str(output)], input='{"id":1,"method":"metadata"}\n',
                               capture_output=True, text=True, timeout=45, check=True)
        response = json.loads(probe.stdout.strip())
        if not response.get('success'):
            raise RuntimeError(f'{driver} agent metadata probe failed')
        metadata = response['data']
        if metadata.get('driverType') != driver or metadata.get('protocolSchema') != 'json-lines-v2':
            raise RuntimeError(f'{driver} agent protocol/identity mismatch')
        digest = hashlib.sha256(output.read_bytes()).hexdigest()
        record = {'type':driver,'sha256':digest,'revision':metadata['agentRevision'],
                  'protocol':metadata['protocolSchema'],'source':'application-bundle'}
        records.append(record)
        filename = f'{driver}-agent_{version}_{goos}_{arch}{suffix}'
        shutil.copy2(output, dist / filename)
        artifacts.append({'id':driver,'kind':'driver','os':goos,'arch':arch,'filename':filename,
                          'url':DRIVER_BASE_URL+'v'+version+'/'+filename,'size':output.stat().st_size,'sha256':digest,
                          'revision':record['revision'],'protocol':record['protocol']})
    (agents / 'bundle.json').write_text(json.dumps({'schema':1,'os':goos,'arch':arch,'drivers':records},indent=2)+'\n')
    if not args.skip_app:
        flags = f'-s -w -X main.version={version}'
        if goos == 'windows':
            flags += ' -H windowsgui'
        if public_key:
            flags += f' -X main.releasePublicKey={public_key}'
        run(['go','build','-trimpath','-ldflags',flags,'-o',str(binary/f'superlink{suffix}'),'./cmd/superlink'])
        run(['go','build','-trimpath','-ldflags','-s -w','-o',str(binary/f'update-helper{suffix}'),'./cmd/update-helper'])
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
        # separate release assets so the base app does not grow with all SDKs.
        bundled = [record for record in records if record['type']=='sqlite']
        if not bundled:
            raise RuntimeError('an application package must include SQLite')
        destination=resources/'drivers';destination.mkdir()
        shutil.copy2(agents/f'sqlite-driver-agent{suffix}',destination/f'sqlite-driver-agent{suffix}')
        (destination/'bundle.json').write_text(json.dumps({'schema':1,'os':goos,'arch':arch,'drivers':bundled},indent=2)+'\n')
        for name in ['LICENSE','NOTICE','UPSTREAM.md','THIRD_PARTY_NOTICES.md']:
            shutil.copy2(ROOT/name,resources/name)
        license_dir=resources/'licenses';license_dir.mkdir()
        shutil.copy2(ROOT/'internal/ui/assets/OFL.txt',license_dir/'NotoSansSC-OFL.txt')
        for source in (ROOT/'third_party/fonts').glob('*LICENSE*'):
            shutil.copy2(source, license_dir/source.name)
        for source in (ROOT/'third_party/fonts').glob('*OFL*'):
            shutil.copy2(source, license_dir/source.name)
        shutil.copy2(ROOT/'internal/ui/assets/gonavi/sources.json', resources/'gonavi-ui-assets.json')
        shutil.copy2(ROOT/'internal/ui/assets/shell/sources.json', resources/'shell-ui-assets.json')
        shutil.copy2(ROOT/'internal/ui/assets/shell/LICENSE', license_dir/'Lucide-LICENSE.txt')
        shutil.copy2(ROOT/'internal/branding/assets/superlink.png', resources/'superlink.png')
        for name in ['highgo-pq','go-irisnative']:
            source=ROOT/'third_party'/name
            for item in source.glob('*LICENSE*'):
                shutil.copy2(item,license_dir/f'{name}-{item.name}')
        if goos=='darwin':
            import plistlib
            shutil.copy2(ROOT/'internal/branding/assets/superlink.icns', resources/'SuperLink.icns')
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
    (dist/f'build-info-{goos}-{arch}.json').write_text(json.dumps({'version': version, 'os': goos, 'arch': arch, 'releasePublicKey': public_key}, indent=2)+'\n')
    (dist/f'assets-{goos}-{arch}.json').write_text(json.dumps(artifacts,indent=2)+'\n')
    print(f'Built {len(chosen)} driver(s). Artifacts: {dist}',flush=True)


if __name__=='__main__':
    main()
