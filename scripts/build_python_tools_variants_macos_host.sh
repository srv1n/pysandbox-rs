#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

KEY_DIR="$ROOT/.secrets/plugin-signing"
PRIV="$KEY_DIR/ed25519.private"

arch="$(uname -m)"
case "$arch" in
  x86_64)
    platform="macos_x86_64"
    worker_env="RZN_PYTHON_WORKER_BIN_MACOS_X86_64"
    ;;
  arm64|aarch64)
    platform="macos_aarch64"
    worker_env="RZN_PYTHON_WORKER_BIN_MACOS_AARCH64"
    ;;
  *)
    echo "unsupported macOS architecture: $arch" >&2
    exit 2
    ;;
esac

echo "building python-tools variants ($platform)"

if [[ ! -f "$PRIV" ]]; then
  echo "no plugin signing key found; generating dev keypair at $KEY_DIR"
  cargo run --bin rzn-plugin-devkit -- keygen --out "$KEY_DIR"
fi

echo "building host worker..."
cargo build --release --bin rzn-python-worker

echo "building plugin devkit..."
cargo build --release --bin rzn-plugin-devkit

export RZN_PLUGIN_DEVKIT_BIN="$ROOT/target/release/rzn-plugin-devkit"
export "$worker_env=$ROOT/target/release/rzn-python-worker"

if [[ ! -d "$ROOT/python-bundle-minimal" ]]; then
  echo "python bundle missing; building minimal bundle to $ROOT/python-bundle-minimal (this may take a while)"
  bash scripts/build-python-bundle.sh --minimal --output-dir "$ROOT/python-bundle-minimal"
fi

if [[ ! -d "$ROOT/python-bundle-ds" ]]; then
  echo "python bundle missing; building data science bundle to $ROOT/python-bundle-ds (this may take a while)"
  bash scripts/build-python-bundle.sh --datascience --output-dir "$ROOT/python-bundle-ds"
fi

echo "building signed plugin zip(s)..."
python3 scripts/plugins/build_bundle.py --config scripts/plugins/config/python-tools.json --platform "$platform" --key "$PRIV"
python3 scripts/plugins/build_bundle.py --config scripts/plugins/config/python-tools-system.json --platform "$platform" --key "$PRIV"
python3 scripts/plugins/build_bundle.py --config scripts/plugins/config/python-tools-ds.json --platform "$platform" --key "$PRIV"

echo "done"
