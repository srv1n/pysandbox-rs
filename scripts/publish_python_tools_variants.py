#!/usr/bin/env python3
import argparse
import os
import subprocess
import sys
from pathlib import Path


def sh(cmd: list[str]) -> None:
    subprocess.run(cmd, check=True)


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

    # Register all releases first; publish catalog once at the end.
    for i, variant in enumerate(variants):
        config_path = root / config_for[variant]
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
        if args.skip_publish or not is_last:
            cmd += ["--skip-publish"]

        sh(cmd)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        raise

