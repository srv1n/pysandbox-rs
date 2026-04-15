#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
CARGO_TOML = REPO_ROOT / "Cargo.toml"
PLUGIN_CONFIGS = [
    REPO_ROOT / "scripts" / "plugins" / "config" / "python-tools.json",
    REPO_ROOT / "scripts" / "plugins" / "config" / "python-tools-system.json",
    REPO_ROOT / "scripts" / "plugins" / "config" / "python-tools-ds.json",
]
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")


def validate_version(version: str) -> str:
    version = version.strip()
    if not SEMVER_RE.fullmatch(version):
        raise SystemExit(f"invalid version '{version}'; expected semver like 0.2.3")
    return version


def read_cargo_version() -> str:
    in_package = False
    for raw_line in CARGO_TOML.read_text(encoding="utf-8").splitlines():
        stripped = raw_line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[package]"
            continue
        if in_package and stripped.startswith("version = "):
            return stripped.split('"')[1]
    raise RuntimeError(f"failed to determine package version from {CARGO_TOML}")


def write_cargo_version(version: str) -> None:
    version = validate_version(version)
    lines = CARGO_TOML.read_text(encoding="utf-8").splitlines()
    in_package = False
    updated = False
    for index, raw_line in enumerate(lines):
        stripped = raw_line.strip()
        if stripped.startswith("["):
            in_package = stripped == "[package]"
            continue
        if in_package and stripped.startswith("version = "):
            lines[index] = f'version = "{version}"'
            updated = True
            break
    if not updated:
        raise RuntimeError(f"failed to update package version in {CARGO_TOML}")
    CARGO_TOML.write_text("\n".join(lines) + "\n", encoding="utf-8")


def read_plugin_versions() -> dict[Path, str]:
    versions: dict[Path, str] = {}
    for path in PLUGIN_CONFIGS:
        data = json.loads(path.read_text(encoding="utf-8"))
        versions[path] = str(data["version"]).strip()
    return versions


def write_plugin_versions(version: str) -> None:
    version = validate_version(version)
    for path in PLUGIN_CONFIGS:
        data = json.loads(path.read_text(encoding="utf-8"))
        data["version"] = version
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def set_version(version: str) -> None:
    version = validate_version(version)
    write_cargo_version(version)
    write_plugin_versions(version)


def ensure_synced(expected: str | None = None) -> str:
    cargo_version = read_cargo_version().strip()
    if expected is not None and cargo_version != expected:
        raise SystemExit(f"Cargo.toml version {cargo_version} does not match expected {expected}")

    plugin_versions = read_plugin_versions()
    mismatches = [path for path, version in plugin_versions.items() if version != cargo_version]
    if mismatches:
        joined = ", ".join(str(path.relative_to(REPO_ROOT)) for path in mismatches)
        raise SystemExit(f"version mismatch: Cargo.toml={cargo_version}, out-of-sync configs: {joined}")
    return cargo_version


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Version helpers for release automation.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("show", help="Print the current synced version.")

    check = subparsers.add_parser("check", help="Verify Cargo.toml and plugin config versions match.")
    check.add_argument("--expected", help="Require the synced version to match this exact value.")

    set_cmd = subparsers.add_parser("set", help="Write a new version into Cargo.toml and plugin configs.")
    set_cmd.add_argument("--version", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "show":
        print(ensure_synced())
        return 0
    if args.command == "check":
        print(ensure_synced(args.expected))
        return 0
    if args.command == "set":
        set_version(args.version)
        print(ensure_synced(args.version))
        return 0
    raise AssertionError(f"unexpected command {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
