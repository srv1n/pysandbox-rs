#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import zipfile
from collections import OrderedDict
from dataclasses import dataclass
from pathlib import Path


FIXED_ZIP_DT = (1980, 1, 1, 0, 0, 0)
REPO_ROOT = Path(__file__).resolve().parents[2]
MACOS_MACHO_MAGICS = {
    b"\xfe\xed\xfa\xce",  # MH_MAGIC
    b"\xce\xfa\xed\xfe",  # MH_CIGAM
    b"\xfe\xed\xfa\xcf",  # MH_MAGIC_64
    b"\xcf\xfa\xed\xfe",  # MH_CIGAM_64
    b"\xca\xfe\xba\xbe",  # FAT_MAGIC
    b"\xbe\xba\xfe\xca",  # FAT_CIGAM
}


@dataclass(frozen=True)
class PayloadFile:
    source: Path
    dest: str
    mode: int


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def expand_env(value: str) -> str:
    if "$" not in value:
        return value
    expanded = os.path.expandvars(value)
    if "$" in expanded:
        raise ValueError(f"unresolved env in path: {value}")
    return expanded


def normalize_dest(path: Path) -> str:
    return path.as_posix().lstrip("./")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def is_macho_binary(path: Path) -> bool:
    try:
        with path.open("rb") as f:
            magic = f.read(4)
    except OSError:
        return False
    return magic in MACOS_MACHO_MAGICS


def stage_payloads(payloads: dict, stage_root: Path) -> dict:
    staged = {}
    for dest, payload in payloads.items():
        # Only stage Mach-O payloads so we can codesign without mutating source files.
        if is_macho_binary(payload.source):
            dest_path = (stage_root / dest).resolve()
            dest_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(payload.source, dest_path, follow_symlinks=True)
            os.chmod(dest_path, payload.mode)
            staged_src = dest_path
        else:
            staged_src = payload.source

        staged[dest] = PayloadFile(source=staged_src, dest=payload.dest, mode=payload.mode)
    return staged


def codesign_macos_payloads(payloads: dict, identity: str) -> None:
    worker_entitlements = Path(
        os.environ.get(
            "RZN_MACOS_WORKER_ENTITLEMENTS",
            str(REPO_ROOT / "entitlements" / "RznPythonWorker.entitlements"),
        )
    ).expanduser()
    python_entitlements = Path(
        os.environ.get(
            "RZN_MACOS_PYTHON_INHERIT_ENTITLEMENTS",
            str(REPO_ROOT / "entitlements" / "RznPythonInherit.entitlements"),
        )
    ).expanduser()

    if not worker_entitlements.exists():
        raise ValueError(f"missing worker entitlements: {worker_entitlements}")
    if not python_entitlements.exists():
        raise ValueError(f"missing python entitlements: {python_entitlements}")

    macho_entries = []
    for dest in sorted(payloads.keys()):
        src = payloads[dest].source
        if is_macho_binary(src):
            macho_entries.append((dest, src))

    if not macho_entries:
        return

    print(f"🔏 codesign (macOS): {len(macho_entries)} Mach-O payload files")

    for dest, src in macho_entries:
        entitlements = None
        if dest.endswith("bin/macos/universal/rzn-python-worker"):
            entitlements = worker_entitlements
        elif dest in ("resources/python/bin/python3", "resources/python/bin/python"):
            entitlements = python_entitlements

        requested_mode = src.stat().st_mode & 0o777
        os.chmod(src, requested_mode | 0o200)
        try:
            cmd = [
                "/usr/bin/codesign",
                "--force",
                "--sign",
                identity,
                "--timestamp=none",
            ]
            if entitlements is not None:
                cmd += ["--entitlements", str(entitlements)]
            cmd.append(str(src))
            subprocess.run(cmd, check=True)
        finally:
            os.chmod(src, requested_mode)


def add_payload_file(payloads: dict, source: Path, dest: str, mode: int) -> None:
    # Allow symlinks in source trees (e.g., python bundles). We always embed file bytes
    # in the ZIP, not symlink metadata.
    if dest in payloads:
        raise ValueError(f"duplicate payload path: {dest}")
    payloads[dest] = PayloadFile(source=source, dest=dest, mode=mode)


def collect_payloads(config: dict, platform: str) -> dict:
    payloads: dict[str, PayloadFile] = {}

    def handle_item(item: dict) -> None:
        platforms = item.get("platforms")
        if platforms and platform not in platforms:
            return
        source_raw = item.get("source")
        if not source_raw:
            raise ValueError("payload item missing source")
        source_path = Path(expand_env(str(source_raw))).expanduser().resolve()
        dest_root = Path(item.get("dest", "")).as_posix()
        mode = int(str(item.get("mode", "644")), 8)
        if source_path.is_dir():
            for root, dirs, files in os.walk(source_path):
                dirs.sort()
                root_path = Path(root)
                files_sorted = sorted(files)
                for fname in files_sorted:
                    file_path = root_path / fname
                    rel = file_path.relative_to(source_path)
                    dest = normalize_dest(Path(dest_root) / rel)
                    file_mode = mode if mode != 0o644 else (file_path.stat().st_mode & 0o777)
                    add_payload_file(payloads, file_path, dest, file_mode)
        else:
            dest = normalize_dest(Path(dest_root))
            if not dest:
                raise ValueError(f"payload dest missing for file {source_path}")
            add_payload_file(payloads, source_path, dest, mode)

    for item in config.get("payloads", []):
        handle_item(item)
    for item in config.get("shared_payloads", []):
        handle_item(item)

    return payloads


def build_manifest(config: dict, platform: str, sha_map: OrderedDict) -> OrderedDict:
    manifest = OrderedDict()
    manifest["v"] = 1
    manifest["id"] = config["id"]
    manifest["version"] = config["version"]
    manifest["name"] = config["name"]
    if config.get("description"):
        manifest["description"] = config["description"]
    if config.get("min_host_version"):
        manifest["min_host_version"] = config["min_host_version"]
    if config.get("mcp_protocol_version"):
        manifest["mcp_protocol_version"] = config["mcp_protocol_version"]

    workers_out = []
    for worker in config.get("workers", []):
        entrypoints = worker.get("entrypoints", {})
        entry = entrypoints.get(platform)
        if not entry:
            raise ValueError(f"missing entrypoint for worker {worker['id']} platform {platform}")
        worker_out = OrderedDict()
        worker_out["id"] = worker["id"]
        worker_out["kind"] = worker.get("kind", "mcp_stdio")
        worker_out["auto_start"] = bool(worker.get("auto_start", False))
        worker_out["entrypoint"] = OrderedDict([(platform, entry)])
        worker_out["args"] = worker.get("args", [])
        worker_out["env"] = worker.get("env", {})
        if worker.get("tools_namespace"):
            worker_out["tools_namespace"] = worker["tools_namespace"]
        workers_out.append(worker_out)
    manifest["workers"] = workers_out
    manifest["resources"] = config.get("resources", [])
    manifest["sha256"] = sha_map
    return manifest


def write_manifest(path: Path, manifest: OrderedDict) -> None:
    content = json.dumps(manifest, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    path.write_text(content + "\n", encoding="utf-8")


def resolve_devkit_bin() -> list[str]:
    override = os.environ.get("RZN_PLUGIN_DEVKIT_BIN", "").strip()
    if override:
        return [override]

    for cand in [
        REPO_ROOT / "target" / "release" / "rzn-plugin-devkit",
        REPO_ROOT / "target" / "debug" / "rzn-plugin-devkit",
    ]:
        if cand.exists():
            return [str(cand)]

    return ["cargo", "run", "-q", "--bin", "rzn-plugin-devkit", "--"]


def sign_manifest(key_path: Path, manifest_path: Path, sig_path: Path) -> None:
    cmd = resolve_devkit_bin() + [
        "sign",
        "--key",
        str(key_path),
        "--input",
        str(manifest_path),
        "--output",
        str(sig_path),
    ]
    subprocess.run(cmd, check=True)


def write_zip(zip_path: Path, manifest_path: Path, sig_path: Path, payloads: dict) -> None:
    with zipfile.ZipFile(
        zip_path,
        "w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as zf:
        for rel_path, src_path, mode in [
            ("plugin.json", manifest_path, 0o644),
            ("plugin.sig", sig_path, 0o644),
        ]:
            info = zipfile.ZipInfo(rel_path, FIXED_ZIP_DT)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (mode & 0o777) << 16
            zf.writestr(info, src_path.read_bytes())
        for dest in sorted(payloads.keys()):
            payload = payloads[dest]
            info = zipfile.ZipInfo(dest, FIXED_ZIP_DT)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (payload.mode & 0o777) << 16
            zf.writestr(info, payload.source.read_bytes())


def build_bundle(config_path: Path, platform: str, key_path: Path, out_dir: Path) -> Path:
    config = load_json(config_path)
    payloads = collect_payloads(config, platform)

    staging = out_dir / config["id"] / config["version"] / platform
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True, exist_ok=True)

    # Optional: codesign macOS payloads in a staging copy so we don't mutate source files.
    codesign_identity = os.environ.get("RZN_MACOS_CODESIGN_IDENTITY", "").strip()
    if platform.startswith("macos") and codesign_identity:
        with tempfile.TemporaryDirectory(prefix="rzn-plugin-payloads-") as tmpdir:
            staged_root = Path(tmpdir)
            payloads = stage_payloads(payloads, staged_root)
            codesign_macos_payloads(payloads, codesign_identity)

            sha_map = OrderedDict()
            for dest in sorted(payloads.keys()):
                sha_map[dest] = sha256_file(payloads[dest].source)
            for res in config.get("resources", []):
                res_path = res.get("path") if isinstance(res, dict) else res
                if res_path and res_path not in sha_map:
                    raise ValueError(f"resource path missing from payloads: {res_path}")

            manifest = build_manifest(config, platform, sha_map)
            manifest_path = staging / "plugin.json"
            sig_path = staging / "plugin.sig"
            write_manifest(manifest_path, manifest)
            sign_manifest(key_path, manifest_path, sig_path)

            zip_name = f"{config['id']}-{config['version']}-{platform}.zip"
            zip_path = staging / zip_name
            write_zip(zip_path, manifest_path, sig_path, payloads)
            return zip_path

    sha_map = OrderedDict()
    for dest in sorted(payloads.keys()):
        sha_map[dest] = sha256_file(payloads[dest].source)
    for res in config.get("resources", []):
        res_path = res.get("path") if isinstance(res, dict) else res
        if res_path and res_path not in sha_map:
            raise ValueError(f"resource path missing from payloads: {res_path}")
    manifest = build_manifest(config, platform, sha_map)

    manifest_path = staging / "plugin.json"
    sig_path = staging / "plugin.sig"
    write_manifest(manifest_path, manifest)
    sign_manifest(key_path, manifest_path, sig_path)

    zip_name = f"{config['id']}-{config['version']}-{platform}.zip"
    zip_path = staging / zip_name
    write_zip(zip_path, manifest_path, sig_path, payloads)
    return zip_path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build signed plugin bundle.")
    parser.add_argument("--config", required=True, help="Path to plugin config JSON.")
    parser.add_argument("--platform", help="Target platform key (e.g., macos_universal).")
    parser.add_argument("--all", action="store_true", help="Build all platforms in config.")
    parser.add_argument("--key", required=True, help="Path to base64 Ed25519 private seed.")
    parser.add_argument("--out", default="dist/plugins", help="Output directory.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    config_path = Path(args.config).resolve()
    config = load_json(config_path)
    platforms = config.get("platforms", [])
    if args.all:
        targets = platforms
    else:
        if not args.platform:
            raise SystemExit("must specify --platform or --all")
        targets = [args.platform]
    if not targets:
        raise SystemExit("no platforms defined")
    out_dir = Path(args.out).resolve()
    key_path = Path(args.key).resolve()
    for platform in targets:
        if platforms and platform not in platforms:
            raise SystemExit(f"platform {platform} not in config platforms")
        zip_path = build_bundle(config_path, platform, key_path, out_dir)
        print(f"built {zip_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

