#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path


def sh(cmd: list[str], *, env: dict | None = None) -> None:
    subprocess.run(cmd, check=True, env=env)


def sha256_hex(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def http_post_json(url: str, token: str, payload: dict) -> dict:
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, method="POST", data=body)
    req.add_header("Content-Type", "application/json")
    req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            return json.loads(raw) if raw.strip() else {}
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"POST {url} failed: {e.code} {raw}") from None


def ensure_file(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content.strip() + "\n", encoding="utf-8")


def load_config(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def aws_env_from_r2() -> dict:
    access_key = os.environ.get("R2_PLUGINS_ACCESS_KEY_ID", "").strip()
    secret_key = os.environ.get("R2_PLUGINS_SECRET_ACCESS_KEY", "").strip()
    region = os.environ.get("R2_PLUGINS_REGION", "auto").strip()
    if not access_key or not secret_key:
        raise RuntimeError("missing R2_PLUGINS_ACCESS_KEY_ID / R2_PLUGINS_SECRET_ACCESS_KEY")
    env = os.environ.copy()
    env["AWS_ACCESS_KEY_ID"] = access_key
    env["AWS_SECRET_ACCESS_KEY"] = secret_key
    env["AWS_DEFAULT_REGION"] = region
    return env


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Build + upload + register + publish python-tools to backend Option B."
    )
    ap.add_argument(
        "--config",
        default="scripts/plugins/config/python-tools.json",
        help="Plugin config JSON path",
    )
    ap.add_argument("--platform", default="macos_universal", help="Platform key")
    ap.add_argument("--channel", default="stable", choices=["stable", "beta", "nightly"])
    ap.add_argument(
        "--catalog-version",
        default="",
        help="Optional RFC3339 catalog version; default uses backend now()",
    )
    ap.add_argument("--skip-build", action="store_true", help="Skip build steps")
    ap.add_argument("--skip-upload", action="store_true", help="Skip R2 upload")
    ap.add_argument("--skip-publish", action="store_true", help="Skip catalog publish")
    args = ap.parse_args()

    root = Path(__file__).resolve().parents[1]
    config_path = (root / args.config).resolve()
    config = load_config(config_path)

    plugin_id = str(config["id"]).strip()
    version = str(config["version"]).strip()

    backend_base = os.environ.get("RZN_BACKEND_BASE_URL", "").strip().rstrip("/")
    admin_token = os.environ.get("RZN_PLATFORM_ADMIN_TOKEN", "").strip()
    if not backend_base:
        raise RuntimeError("missing RZN_BACKEND_BASE_URL (e.g. https://api.rzn.ai)")
    if not admin_token:
        raise RuntimeError("missing RZN_PLATFORM_ADMIN_TOKEN")

    r2_bucket = os.environ.get("R2_PLUGINS_BUCKET", "").strip()
    r2_endpoint = os.environ.get("R2_PLUGINS_ENDPOINT", "").strip()
    r2_prefix = os.environ.get("R2_PLUGINS_PREFIX", "plugins").strip().strip("/")
    if not r2_bucket:
        raise RuntimeError("missing R2_PLUGINS_BUCKET")
    if not r2_endpoint:
        raise RuntimeError("missing R2_PLUGINS_ENDPOINT")

    # 1) Build artifacts
    if not args.skip_build:
        sh(["bash", str(root / "scripts/build_python_tools_bundle_macos_universal.sh")])

    zip_name = f"{plugin_id}-{version}-{args.platform}.zip"
    zip_path = (
        root
        / "dist/plugins"
        / plugin_id
        / version
        / args.platform
        / zip_name
    )
    if not zip_path.exists():
        raise RuntimeError(f"missing built zip: {zip_path}")

    # 2) Upload to R2
    artifact_key = f"{r2_prefix}/{plugin_id}/{version}/{args.platform}/{zip_name}"
    if not args.skip_upload:
        env = aws_env_from_r2()
        # Force path-style addressing for R2.
        sh(["aws", "configure", "set", "default.s3.addressing_style", "path"], env=env)
        sh(
            [
                "aws",
                "s3api",
                "put-object",
                "--endpoint-url",
                r2_endpoint,
                "--bucket",
                r2_bucket,
                "--key",
                artifact_key,
                "--body",
                str(zip_path),
                "--content-type",
                "application/zip",
            ],
            env=env,
        )

    # 3) Register release
    digest = sha256_hex(zip_path)
    register_url = f"{backend_base}/admin/plugins/releases"
    reg = http_post_json(
        register_url,
        admin_token,
        {
            "plugin_id": plugin_id,
            "version": version,
            "platform": args.platform,
            "artifact_key": artifact_key,
            "artifact_sha256": digest,
            "notes": "pysandbox-rs publish",
        },
    )
    print("registered:", reg)

    # 4) Publish catalog
    if not args.skip_publish:
        publish_url = f"{backend_base}/admin/plugins/catalog/publish"
        base_url = f"{backend_base}/plugins/artifacts"
        payload = {"channel": args.channel, "base_url": base_url}
        if args.catalog_version.strip():
            payload["catalog_version"] = args.catalog_version.strip()
        pub = http_post_json(publish_url, admin_token, payload)
        print("published:", pub)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        raise

