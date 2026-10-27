#!/usr/bin/env python3
"""Check boundaries and authored source size without running a browser."""
from pathlib import Path
import ast
import json
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
errors = []
for path in ROOT.glob('internal/**/*.go'):
    if 'upstream' in path.parts or path.name.endswith('_test.go'):
        continue
    source = path.read_text()
    relative = path.relative_to(ROOT).as_posix()
    if len(source.splitlines()) > 800:
        errors.append(f'{relative}: exceeds 800 authored lines')
    if any(part in path.parts for part in ['application','domain','infra','bootstrap']):
        if re.search(r'"(?:fyne\.io/fyne|github\.com/wailsapp)',source):
            errors.append(f'{relative}: UI dependency crosses the application boundary')
for path in ROOT.glob('tools/*.py'):
    ast.parse(path.read_text(),filename=str(path))
packages = json.loads(subprocess.check_output(['go','list','-json','./cmd/driver-agent'],cwd=ROOT,text=True))
for package in packages.get('Deps',[]):
    if 'wailsapp' in package or package.endswith('/jvm'):
        errors.append(f'excluded runtime dependency: {package}')
if (ROOT/'internal/upstream/jvm').exists():
    errors.append('JVM connector must not be present')
if errors:
    raise SystemExit('\n'.join(errors))
print('Architecture checks passed: driver agents only, no Wails/JVM runtime, no Fyne UI.')
