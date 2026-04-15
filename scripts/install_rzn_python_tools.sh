#!/usr/bin/env sh
set -eu

ARTIFACT_PATH=""
ARTIFACT_URL=""
ARTIFACT_BASENAME=""
GITHUB_REPO="${RZN_PYTHON_TOOLS_GITHUB_REPO:-srv1n/pysandbox-rs}"
BASE_URL="${RZN_PYTHON_TOOLS_BASE_URL:-}"
VERSION="${RZN_PYTHON_TOOLS_VERSION:-}"
VARIANT="${RZN_PYTHON_TOOLS_VARIANT:-system}"
PLATFORM="${RZN_PYTHON_TOOLS_PLATFORM:-auto}"
INSTALL_ROOT="${RZN_PYTHON_TOOLS_INSTALL_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/rzn-python-tools}"
BIN_DIR="${RZN_PYTHON_TOOLS_BIN_DIR:-$HOME/.local/bin}"
WORKFLOWS_DIR="${RZN_PYTHON_TOOLS_WORKFLOWS_DIR:-$HOME/.rzn/python-tools/workflows}"
SYNC_WORKFLOWS=1

usage() {
  cat <<EOF
Usage:
  sh scripts/install_rzn_python_tools.sh --artifact-path <tar.gz>

Remote install:
  sh install.sh --version <version> [--github-repo owner/repo] [--variant system]

Options:
  --artifact-path PATH
  --artifact-url URL
  --github-repo owner/repo
  --base-url URL
  --version VERSION
  --variant minimal|ds|system
  --platform auto|linux_x86_64|linux_aarch64|macos_x86_64|macos_aarch64
  --install-root PATH
  --bin-dir PATH
  --workflows-dir PATH
  --skip-workflows-sync
EOF
}

detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os:$arch" in
    Darwin:x86_64) echo "macos_x86_64" ;;
    Darwin:arm64|Darwin:aarch64) echo "macos_aarch64" ;;
    Linux:x86_64) echo "linux_x86_64" ;;
    Linux:aarch64|Linux:arm64) echo "linux_aarch64" ;;
    *)
      echo "unsupported platform: $os/$arch" >&2
      exit 2
      ;;
  esac
}

artifact_ext() {
  case "$1" in
    windows_*) echo "zip" ;;
    *) echo "tar.gz" ;;
  esac
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --artifact-path)
      ARTIFACT_PATH="$2"
      shift 2
      ;;
    --artifact-url)
      ARTIFACT_URL="$2"
      shift 2
      ;;
    --base-url)
      BASE_URL="$2"
      shift 2
      ;;
    --github-repo)
      GITHUB_REPO="$2"
      shift 2
      ;;
    --version)
      VERSION="$2"
      shift 2
      ;;
    --variant)
      VARIANT="$2"
      shift 2
      ;;
    --platform)
      PLATFORM="$2"
      shift 2
      ;;
    --install-root)
      INSTALL_ROOT="$2"
      shift 2
      ;;
    --bin-dir)
      BIN_DIR="$2"
      shift 2
      ;;
    --workflows-dir)
      WORKFLOWS_DIR="$2"
      shift 2
      ;;
    --skip-workflows-sync)
      SYNC_WORKFLOWS=0
      shift 1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ "$PLATFORM" = "auto" ]; then
  PLATFORM="$(detect_platform)"
fi

if [ -z "$ARTIFACT_PATH" ] && [ -z "$ARTIFACT_URL" ]; then
  if [ -n "$BASE_URL" ] && [ -n "$VERSION" ]; then
    ext="$(artifact_ext "$PLATFORM")"
    ARTIFACT_URL="${BASE_URL%/}/rzn-python-tools-${VERSION}-${PLATFORM}-${VARIANT}.${ext}"
  elif [ -n "$VERSION" ]; then
    ext="$(artifact_ext "$PLATFORM")"
    ARTIFACT_URL="https://github.com/${GITHUB_REPO}/releases/download/v${VERSION}/rzn-python-tools-${VERSION}-${PLATFORM}-${VARIANT}.${ext}"
  else
    echo "artifact source missing: use --artifact-path, --artifact-url, or --version" >&2
    exit 2
  fi
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/rzn-python-tools-install.XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

ARTIFACT_FILE="$TMP_DIR/artifact.tar.gz"
if [ -n "$ARTIFACT_PATH" ]; then
  ARTIFACT_BASENAME="$(basename "$ARTIFACT_PATH")"
  cp "$ARTIFACT_PATH" "$ARTIFACT_FILE"
else
  ARTIFACT_BASENAME="$(basename "$ARTIFACT_URL")"
  curl -fsSL "$ARTIFACT_URL" -o "$ARTIFACT_FILE"
fi

mkdir -p "$TMP_DIR/extract"
tar -xzf "$ARTIFACT_FILE" -C "$TMP_DIR/extract"

PACKAGE_DIR="$TMP_DIR/extract/rzn-python-tools"
if [ ! -d "$PACKAGE_DIR" ]; then
  echo "invalid archive: missing rzn-python-tools/ root" >&2
  exit 2
fi

RELEASE_NAME="${ARTIFACT_BASENAME%.tar.gz}"
RELEASE_DIR="$INSTALL_ROOT/releases/$RELEASE_NAME"
CURRENT_LINK="$INSTALL_ROOT/current"

mkdir -p "$INSTALL_ROOT/releases" "$BIN_DIR"
rm -rf "$RELEASE_DIR"
mkdir -p "$RELEASE_DIR"
cp -R "$PACKAGE_DIR"/. "$RELEASE_DIR"/

rm -f "$CURRENT_LINK"
ln -s "$RELEASE_DIR" "$CURRENT_LINK"

rm -f "$BIN_DIR/rzn-python-tools"
ln -s "$CURRENT_LINK/bin/rzn-python-tools" "$BIN_DIR/rzn-python-tools"

cat > "$BIN_DIR/rzn-python-worker" <<EOF
#!/usr/bin/env sh
set -eu
ROOT="\${RZN_PYTHON_TOOLS_HOME:-$CURRENT_LINK}"
export RZN_PLUGIN_DIR="\$ROOT"
exec "\$ROOT/libexec/rzn-python-worker" "\$@"
EOF
chmod +x "$BIN_DIR/rzn-python-worker"

if [ "$SYNC_WORKFLOWS" -eq 1 ]; then
  "$BIN_DIR/rzn-python-tools" workflows sync --dest "$WORKFLOWS_DIR" --force >/dev/null
fi

echo "installed rzn-python-tools into $RELEASE_DIR"
echo "cli: $BIN_DIR/rzn-python-tools"
echo "worker: $BIN_DIR/rzn-python-worker"
echo "workflows: $WORKFLOWS_DIR"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    echo "warning: $BIN_DIR is not on PATH" >&2
    ;;
esac
