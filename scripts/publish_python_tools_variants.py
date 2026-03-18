#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def sh(cmd: list[str]) -> None:
    subprocess.run(cmd, check=True)


def _has_non_empty(value: str | None) -> bool:
    return bool(value and value.strip())


def load_env_file_values(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"')
    return values


def has_scoped_publisher_setup(root: Path, plugin_id: str) -> bool:
    if _has_non_empty(os.environ.get("RZN_PLUGIN_PRODUCT_ID")) and _has_non_empty(
        os.environ.get("RZN_PUBLISHER_KEY")
    ):
        return True

    candidates = [
        root.parent / "backend" / ".secrets" / "plugin-publishers" / f"{plugin_id}.env",
        root / ".secrets" / f"plugin-publisher-{plugin_id}.env",
        root / ".secrets" / "plugin-publisher.env",
    ]
    for candidate in candidates:
        values = load_env_file_values(candidate)
        if _has_non_empty(values.get("RZN_PLUGIN_PRODUCT_ID")) and _has_non_empty(
            values.get("RZN_PUBLISHER_KEY")
        ):
            return True

    return False


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Build + upload + register + publish multiple python-tools plugin variants."
    )
    ap.add_argument("--platform", default="macos_universal", help="Platform key")
    ap.add_argument("--channel", default="stable", choices=["stable", "beta", "nightly"])
    ap.add_argument("--catalog-version", default="", help="Optional RFC3339 catalog version")
    ap.add_argument("--skip-build", action="store_true", help="Skip building zips")
    ap.add_argument("--skip-upload", action="store_true", help="Skip R2 upload")
    ap.add_argument("--skip-publish", action="store_true", help="Skip catalog publish")
    ap.add_argument(
        "--variants",
        default="minimal,system,ds",
        help="Comma list: minimal,system,ds",
    )
    args = ap.parse_args()

    root = Path(__file__).resolve().parents[1]
    variants = [v.strip().lower() for v in args.variants.split(",") if v.strip()]
    known = {"minimal", "system", "ds"}
    unknown = [v for v in variants if v not in known]
    if unknown:
        raise SystemExit(f"unknown variants: {unknown} (known: {sorted(known)})")
    if not variants:
        raise SystemExit("no variants selected")

    config_for = {
        "minimal": "scripts/plugins/config/python-tools.json",
        "system": "scripts/plugins/config/python-tools-system.json",
        "ds": "scripts/plugins/config/python-tools-ds.json",
    }

    if not args.skip_build:
        sh(["bash", str(root / "scripts/build_python_tools_variants_macos_universal.sh")])

    extra = ["--platform", args.platform, "--channel", args.channel]
    if args.catalog_version.strip():
        extra += ["--catalog-version", args.catalog_version.strip()]
    if args.skip_upload:
        extra += ["--skip-upload"]

    publish_script = root / "scripts/publish_python_tools_release.py"

    # Legacy admin publishing updates the shared catalog once at the end.
    # Scoped publisher flow publishes each release independently.
    for i, variant in enumerate(variants):
        config_path = root / config_for[variant]
        config = json.loads(config_path.read_text(encoding="utf-8"))
        scoped_publish = has_scoped_publisher_setup(root, str(config["id"]).strip())
        cmd = [
            "python3",
            str(publish_script),
            "--config",
            str(config_path),
            "--platform",
            args.platform,
            "--channel",
            args.channel,
            "--skip-build",
        ]
        if args.catalog_version.strip():
            cmd += ["--catalog-version", args.catalog_version.strip()]
        if args.skip_upload:
            cmd += ["--skip-upload"]

        is_last = i == (len(variants) - 1)
        should_publish = scoped_publish or is_last
        if args.skip_publish or not should_publish:
            cmd += ["--skip-publish"]

        sh(cmd)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        raise
