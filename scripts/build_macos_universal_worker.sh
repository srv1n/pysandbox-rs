#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIN_NAME="rzn-python-worker"
OUT_DIR="$ROOT/target/universal-apple-darwin/release"
OUT_BIN="$OUT_DIR/$BIN_NAME"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macos_universal build requires macOS"
  exit 1
fi

mkdir -p "$OUT_DIR"

echo "building $BIN_NAME for aarch64-apple-darwin..."
cargo build --release --bin "$BIN_NAME" --target aarch64-apple-darwin

echo "building $BIN_NAME for x86_64-apple-darwin..."
cargo build --release --bin "$BIN_NAME" --target x86_64-apple-darwin

ARM_BIN="$ROOT/target/aarch64-apple-darwin/release/$BIN_NAME"
X86_BIN="$ROOT/target/x86_64-apple-darwin/release/$BIN_NAME"

if [[ ! -f "$ARM_BIN" ]]; then
  echo "error: missing $ARM_BIN"
  exit 2
fi
if [[ ! -f "$X86_BIN" ]]; then
  echo "error: missing $X86_BIN"
  exit 2
fi

echo "creating universal binary..."
/usr/bin/lipo -create -output "$OUT_BIN" "$ARM_BIN" "$X86_BIN"

echo "wrote $OUT_BIN"
/usr/bin/lipo -info "$OUT_BIN" || true

