#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

KEY_DIR="$ROOT/.secrets/plugin-signing"
PRIV="$KEY_DIR/ed25519.private"

if [[ ! -d "$ROOT/python-bundle" ]]; then
  echo "python bundle missing; building to $ROOT/python-bundle (this may take a while)"
  bash scripts/build-python-bundle.sh --output-dir "$ROOT/python-bundle"
fi

if [[ ! -f "$PRIV" ]]; then
  echo "no plugin signing key found; generating dev keypair at $KEY_DIR"
  cargo run --bin rzn-plugin-devkit -- keygen --out "$KEY_DIR"
fi

echo "building universal worker..."
bash scripts/build_macos_universal_worker.sh

echo "building plugin devkit..."
cargo build --release --bin rzn-plugin-devkit

export RZN_PLUGIN_DEVKIT_BIN="$ROOT/target/release/rzn-plugin-devkit"
export RZN_PYTHON_WORKER_BIN_MACOS="$ROOT/target/universal-apple-darwin/release/rzn-python-worker"

echo "building signed plugin zip..."
python3 scripts/plugins/build_bundle.py \
  --config scripts/plugins/config/python-tools.json \
  --platform macos_universal \
  --key "$PRIV"
