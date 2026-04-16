#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import platform as py_platform
import shutil
import subprocess
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

from versioning import ensure_synced, read_cargo_version


REPO_ROOT = Path(__file__).resolve().parents[1]
DIST_ROOT = REPO_ROOT / "dist" / "install"
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


@dataclass(frozen=True)
class HostPlatform:
    key: str
    archive_ext: str
    binary_suffix: str


def sh(cmd: list[str]) -> None:
    subprocess.run(cmd, cwd=REPO_ROOT, check=True)


def detect_host_platform() -> HostPlatform:
    system = py_platform.system()
    machine = py_platform.machine().lower()
    if machine in {"amd64", "x64"}:
        machine = "x86_64"
    if machine in {"arm64"}:
        machine = "aarch64"

    if system == "Darwin" and machine == "x86_64":
        return HostPlatform(key="macos_x86_64", archive_ext=".tar.gz", binary_suffix="")
    if system == "Darwin" and machine == "aarch64":
        return HostPlatform(key="macos_aarch64", archive_ext=".tar.gz", binary_suffix="")
    if system == "Linux" and machine == "x86_64":
        return HostPlatform(key="linux_x86_64", archive_ext=".tar.gz", binary_suffix="")
    if system == "Linux" and machine == "aarch64":
        return HostPlatform(key="linux_aarch64", archive_ext=".tar.gz", binary_suffix="")
    if system == "Windows" and machine == "x86_64":
        return HostPlatform(key="windows_x86_64", archive_ext=".zip", binary_suffix=".exe")
    raise SystemExit(f"unsupported host platform: system={system} machine={machine}")


def ensure_release_binaries(host: HostPlatform) -> dict[str, Path]:
    binaries = {
        PACKAGE_NAME: REPO_ROOT / "target" / "release" / f"{PACKAGE_NAME}{host.binary_suffix}",
        "rzn-python-worker": REPO_ROOT / "target" / "release" / f"rzn-python-worker{host.binary_suffix}",
    }
    missing = [name for name, path in binaries.items() if not path.exists()]
    if missing:
        sh(["cargo", "build", "--release", "--bin", PACKAGE_NAME, "--bin", "rzn-python-worker"])
    return binaries


def ensure_variant_inputs(variant: str, host: HostPlatform) -> None:
    info = VARIANTS[variant]
    if info["python_bundle"] is None:
        return
    if host.key.startswith("windows"):
        raise SystemExit(f"variant '{variant}' is not supported on {host.key}; use variant=system")
    if not info["python_bundle"].exists():
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


def write_zip(zip_path: Path, package_root: Path) -> None:
    with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for file_path in sorted(package_root.rglob("*")):
            if not file_path.is_file():
                continue
            rel_path = file_path.relative_to(package_root.parent).as_posix()
            archive.write(file_path, arcname=rel_path)


def write_tar_gz(tar_path: Path, package_root: Path) -> None:
    subprocess.run(
        ["tar", "-C", str(package_root.parent), "-czf", str(tar_path), package_root.name],
        cwd=REPO_ROOT,
        check=True,
    )


def build_variant(variant: str) -> Path:
    version = ensure_synced()
    host = detect_host_platform()
    ensure_variant_inputs(variant, host)
    binaries = ensure_release_binaries(host)

    staging_root = DIST_ROOT / variant / version / host.key
    archive_name = f"{PACKAGE_NAME}-{version}-{host.key}-{variant}{host.archive_ext}"
    archive_path = staging_root / archive_name
    staging_root.mkdir(parents=True, exist_ok=True)
    if archive_path.exists():
        archive_path.unlink()

    with tempfile.TemporaryDirectory(prefix=f"{PACKAGE_NAME}-stage-") as temp_dir:
        package_root = Path(temp_dir) / PACKAGE_NAME
        package_root.mkdir(parents=True, exist_ok=True)

        (package_root / "bin").mkdir(parents=True, exist_ok=True)
        (package_root / "libexec").mkdir(parents=True, exist_ok=True)
        (package_root / "resources" / "systems" / "python_sandbox").mkdir(parents=True, exist_ok=True)

        cli_name = f"{PACKAGE_NAME}{host.binary_suffix}"
        worker_name = f"rzn-python-worker{host.binary_suffix}"
        shutil.copy2(binaries[PACKAGE_NAME], package_root / "bin" / cli_name)
        shutil.copy2(binaries["rzn-python-worker"], package_root / "libexec" / worker_name)
        shutil.copy2(
            VARIANTS[variant]["system_metadata"],
            package_root / "resources" / "systems" / "python_sandbox" / "system.metadata.yaml",
        )
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
            "cargo_version": read_cargo_version(),
            "platform": host.key,
            "description": VARIANTS[variant]["description"],
            "paths": {
                "cli": f"bin/{cli_name}",
                "worker": f"libexec/{worker_name}",
                "examples": "examples/python_sandbox",
                "workflows": "examples/python_sandbox/quick_starts",
                "system_metadata": "resources/systems/python_sandbox/system.metadata.yaml",
            },
            "bundled_python": python_bundle is not None,
        }
        (package_root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

        if host.archive_ext == ".tar.gz":
            write_tar_gz(archive_path, package_root)
        else:
            write_zip(archive_path, package_root)

    checksum_path = staging_root / f"{archive_name}.sha256"
    checksum_path.write_text(f"{sha256_file(archive_path)}  {archive_name}\n", encoding="utf-8")
    installer = "install_rzn_python_tools.ps1" if host.key.startswith("windows") else "install_rzn_python_tools.sh"
    target_name = "install.ps1" if host.key.startswith("windows") else "install.sh"
    shutil.copy2(REPO_ROOT / "scripts" / installer, staging_root / target_name)
    return archive_path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build host-native release artifacts for rzn-python-tools.")
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
