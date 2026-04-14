#!/usr/bin/env python3
import argparse
import hashlib
import json
import shutil
import subprocess
import tarfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DIST_ROOT = REPO_ROOT / "dist" / "install"
PLATFORM = "macos_universal"
PACKAGE_NAME = "rzn-python-tools"

VARIANTS = {
    "minimal": {
        "plugin_id": "python-tools",
        "python_bundle": REPO_ROOT / "python-bundle-minimal",
        "python_bundle_build": ["bash", "scripts/build-python-bundle.sh", "--minimal", "--output-dir", "python-bundle-minimal"],
        "system_metadata": REPO_ROOT / "resources" / "systems" / "python_sandbox" / "system.metadata.yaml",
        "description": "Bundled minimal runtime.",
    },
    "ds": {
        "plugin_id": "python-tools-ds",
        "python_bundle": REPO_ROOT / "python-bundle-ds",
        "python_bundle_build": ["bash", "scripts/build-python-bundle.sh", "--datascience", "--output-dir", "python-bundle-ds"],
        "system_metadata": REPO_ROOT / "resources" / "systems" / "python_sandbox" / "system.metadata.ds.yaml",
        "description": "Bundled data-science runtime with the full quick-start pack.",
    },
    "system": {
        "plugin_id": "python-tools-system",
        "python_bundle": None,
        "python_bundle_build": None,
        "system_metadata": REPO_ROOT / "resources" / "systems" / "python_sandbox" / "system.metadata.system.yaml",
        "description": "System-Python install with packaged workflows but no bundled runtime.",
    },
}


def sh(cmd: list[str]) -> None:
    subprocess.run(cmd, cwd=REPO_ROOT, check=True)


def cargo_version() -> str:
    cargo_toml = (REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines()
    for line in cargo_toml:
        stripped = line.strip()
        if stripped.startswith("version = "):
            return stripped.split('"')[1]
    raise RuntimeError("failed to determine Cargo version")


def ensure_universal_binary(bin_name: str) -> Path:
    path = REPO_ROOT / "target" / "universal-apple-darwin" / "release" / bin_name
    if not path.exists():
        sh(["bash", "scripts/build_macos_universal_binary.sh", bin_name])
    return path


def ensure_variant_inputs(variant: str) -> None:
    info = VARIANTS[variant]
    if info["python_bundle"] is not None and not info["python_bundle"].exists():
        sh(info["python_bundle_build"])


def copy_tree(src: Path, dest: Path) -> None:
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(src, dest)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def plugin_version(variant: str) -> str:
    config_name = {
        "minimal": "python-tools.json",
        "system": "python-tools-system.json",
        "ds": "python-tools-ds.json",
    }[variant]
    config = json.loads((REPO_ROOT / "scripts" / "plugins" / "config" / config_name).read_text(encoding="utf-8"))
    return str(config["version"]).strip()


def build_variant(variant: str) -> Path:
    ensure_variant_inputs(variant)
    cli_bin = ensure_universal_binary(PACKAGE_NAME)
    worker_bin = ensure_universal_binary("rzn-python-worker")
    version = plugin_version(variant)

    staging_root = DIST_ROOT / variant / version / PLATFORM
    package_root = staging_root / PACKAGE_NAME
    if package_root.exists():
        shutil.rmtree(package_root)
    package_root.mkdir(parents=True, exist_ok=True)

    (package_root / "bin").mkdir(parents=True, exist_ok=True)
    (package_root / "libexec").mkdir(parents=True, exist_ok=True)
    (package_root / "resources" / "systems" / "python_sandbox").mkdir(parents=True, exist_ok=True)

    shutil.copy2(cli_bin, package_root / "bin" / PACKAGE_NAME)
    shutil.copy2(worker_bin, package_root / "libexec" / "rzn-python-worker")
    shutil.copy2(VARIANTS[variant]["system_metadata"], package_root / "resources" / "systems" / "python_sandbox" / "system.metadata.yaml")
    copy_tree(REPO_ROOT / "examples" / "python_sandbox", package_root / "examples" / "python_sandbox")

    python_bundle = VARIANTS[variant]["python_bundle"]
    if python_bundle is not None:
        copy_tree(python_bundle, package_root / "resources" / "python")

    manifest = {
        "schema_version": 1,
        "artifact": PACKAGE_NAME,
        "plugin_variant_id": VARIANTS[variant]["plugin_id"],
        "variant": variant,
        "version": version,
        "cargo_version": cargo_version(),
        "platform": PLATFORM,
        "description": VARIANTS[variant]["description"],
        "paths": {
            "cli": f"bin/{PACKAGE_NAME}",
            "worker": "libexec/rzn-python-worker",
            "examples": "examples/python_sandbox",
            "workflows": "examples/python_sandbox/quick_starts",
            "system_metadata": "resources/systems/python_sandbox/system.metadata.yaml",
        },
        "bundled_python": python_bundle is not None,
    }
    (package_root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    archive_name = f"{PACKAGE_NAME}-{version}-{PLATFORM}-{variant}.tar.gz"
    archive_path = staging_root / archive_name
    if archive_path.exists():
        archive_path.unlink()
    with tarfile.open(archive_path, "w:gz") as tar:
        tar.add(package_root, arcname=PACKAGE_NAME)

    (staging_root / f"{archive_name}.sha256").write_text(f"{sha256_file(archive_path)}  {archive_name}\n", encoding="utf-8")
    shutil.copy2(REPO_ROOT / "scripts" / "install_rzn_python_tools.sh", staging_root / "install.sh")
    return archive_path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build local-install release artifacts for rzn-python-tools.")
    parser.add_argument("--variant", choices=sorted(VARIANTS.keys()), default="ds")
    parser.add_argument("--all-variants", action="store_true")
    parser.add_argument("--print-artifact-path", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    variants = sorted(VARIANTS.keys()) if args.all_variants else [args.variant]
    built: list[Path] = []
    for variant in variants:
        artifact = build_variant(variant)
        built.append(artifact)
        if not args.print_artifact_path:
            print(f"built {artifact}")
    if args.print_artifact_path:
        if len(built) != 1:
            raise SystemExit("--print-artifact-path requires a single variant build")
        print(str(built[0]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
