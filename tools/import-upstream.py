#!/usr/bin/env python3
"""Import the fixed, locally available GoNavi driver dependency closure.

This tool only reads a local checkout. It does not fetch or execute upstream
scripts. Re-import deliberately refuses to overwrite an existing upstream tree.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re
import shutil
import subprocess

COMMIT = "6e20b6ddf56b2ae76f7e5d5c6a3505cef1edc871"
MODULE = "github.com/ealink1/super-link"
ROOTS = ["internal/db", "internal/nacos", "tools/gen-i18n-catalog-zip"]


def rewrite(text):
    text = text.replace('"GoNavi-Wails/internal/', f'"{MODULE}/internal/upstream/')
    text = text.replace('"GoNavi-Wails/shared/i18n"', f'"{MODULE}/internal/upstream/i18n"')
    text = text.replace("GONAVI_", "SUPERLINK_")
    text = text.replace(".gonavi", ".superlink")
    text = text.replace(".GoNavi", ".SuperLink")
    text = text.replace("gonavi-driver-agent-", "superlink-driver-agent-")
    text = text.replace('"gonavi.log"', '"superlink.log"')
    text = text.replace("GoNavi", "SuperLink").replace("Gonavi", "SuperLink").replace("gonavi", "superlink")
    return text


def closure(source):
    seen, pending = set(), ROOTS + ["cmd/optional-driver-agent"]
    while pending:
        package = pending.pop()
        if package in seen:
            continue
        seen.add(package)
        for file in (source / package).glob("*.go"):
            pending.extend(re.findall(r'"GoNavi-Wails/([^\"]+)"', file.read_text()))
    return sorted(seen)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    source = args.source.resolve()
    target = Path(__file__).resolve().parent.parent
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if actual != COMMIT:
        raise SystemExit(f"expected {COMMIT}, found {actual}")
    if (target / "internal/upstream").exists():
        raise SystemExit("upstream tree already exists; review incremental changes instead")
    manifest = []
    for package in closure(source):
        if package == "cmd/optional-driver-agent":
            destination = target / "cmd/driver-agent"
        elif package.startswith("tools/"):
            destination = target / package
        else:
            name = "i18n" if package == "shared/i18n" else package.removeprefix("internal/")
            destination = target / "internal/upstream" / name
        for original in sorted((source / package).rglob("*")):
            if not original.is_file() or ".git" in original.parts:
                continue
            if original.relative_to(source).as_posix() in {"internal/logger/wails_adapter.go","internal/logger/wails_adapter_test.go"}:
                continue
            relative = original.relative_to(source / package)
            output = destination / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            raw = original.read_bytes()
            transformed = rewrite(raw.decode()).encode() if original.suffix in {".go", ".json", ".md"} else raw
            output.write_bytes(transformed)
            manifest.append({"source":str(original.relative_to(source)), "target":str(output.relative_to(target)),
                             "source_sha256":hashlib.sha256(raw).hexdigest(),
                             "imported_sha256":hashlib.sha256(transformed).hexdigest()})
    for directory in ["third_party/highgo-pq", "third_party/go-irisnative"]:
        shutil.copytree(source / directory, target / directory, ignore=shutil.ignore_patterns(".git", "*.key"))
    shutil.copyfile(source / "LICENSE", target / "LICENSE")
    (target / "go.mod").write_text((source / "go.mod").read_text().replace("module GoNavi-Wails", f"module {MODULE}"))
    shutil.copyfile(source / "go.sum", target / "go.sum")
    (target / "testdata").mkdir(exist_ok=True)
    shutil.copyfile(source / "testdata/issue-1326-oracle11g.sql", target / "testdata/issue-1326-oracle11g.sql")
    (target / "docs/upstream-import.json").write_text(json.dumps({"repository":"Syngnat/GoNavi", "commit":COMMIT, "files":manifest}, indent=2)+"\n")
    print(f"Imported {len(manifest)} files from {COMMIT}; third-party licenses preserved.")


if __name__ == "__main__":
    main()
