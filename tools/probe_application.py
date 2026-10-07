#!/usr/bin/env python3
"""Execute the target binary before uploading a package (no browser required)."""
import argparse
from pathlib import Path
import subprocess
from release_platforms import version_text


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--version', required=True)
    args = parser.parse_args()
    version = version_text(args.version)
    goos = subprocess.check_output(['go', 'env', 'GOOS'], text=True).strip()
    executable = Path('bin/superlink.exe' if goos == 'windows' else 'bin/superlink').resolve()
    actual = subprocess.check_output([str(executable), '--version'], text=True, timeout=30).strip()
    if actual != version:
        raise SystemExit('built application version mismatch')
    print('Native application version probe passed:', actual)


if __name__ == '__main__':
    main()
