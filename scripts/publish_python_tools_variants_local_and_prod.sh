#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCAL_URL="${RZN_BACKEND_BASE_URL_LOCAL:-http://localhost:8082}"
CLOUD_URL="${RZN_BACKEND_BASE_URL_CLOUD:-${RZN_BACKEND_BASE_URL_PROD:-https://cloud.rzn.ai}}"
LOCAL_PUBLIC_URL="${RZN_PLUGIN_PUBLIC_BASE_URL_LOCAL:-${LOCAL_URL}}"
CLOUD_PUBLIC_URL="${RZN_PLUGIN_PUBLIC_BASE_URL_CLOUD:-${RZN_PLUGIN_PUBLIC_BASE_URL_PROD:-https://cloud.rzn.ai}}"
LOCAL_TOKEN="${RZN_PLATFORM_ADMIN_TOKEN_LOCAL:-${RZN_PLATFORM_ADMIN_TOKEN:-}}"
CLOUD_TOKEN="${RZN_PLATFORM_ADMIN_TOKEN_CLOUD:-${RZN_PLATFORM_ADMIN_TOKEN_PROD:-${RZN_PLATFORM_ADMIN_TOKEN:-}}}"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'EOF'
Usage: bash scripts/publish_python_tools_variants_local_and_prod.sh [publish_python_tools_variants.py args...]

Publishes python-tools variants to:
  1. local backend   (default: http://localhost:8082)
  2. cloud backend   (default: https://cloud.rzn.ai)

Environment:
  RZN_BACKEND_BASE_URL_LOCAL   Optional local base URL override
  RZN_BACKEND_BASE_URL_CLOUD   Optional cloud base URL override
  RZN_BACKEND_BASE_URL_PROD    Legacy alias for cloud base URL override
  RZN_PLUGIN_PUBLIC_BASE_URL_LOCAL Optional local public catalog base override
  RZN_PLUGIN_PUBLIC_BASE_URL_CLOUD Optional cloud public catalog base override
  RZN_PLUGIN_PUBLIC_BASE_URL_PROD  Legacy alias for cloud public catalog base override
  RZN_PLATFORM_ADMIN_TOKEN_LOCAL
  RZN_PLATFORM_ADMIN_TOKEN_CLOUD
  RZN_PLATFORM_ADMIN_TOKEN_PROD Legacy alias for cloud admin token

Fallback:
  RZN_PLATFORM_ADMIN_TOKEN may be used for both targets if appropriate.
  Scoped publisher credentials may be auto-loaded per variant from:
    ../backend/.secrets/plugin-publishers/<plugin_id>.env
EOF
  exit 0
fi

echo "==> publishing python-tools variants to local backend: ${LOCAL_URL}"
if ! env \
  RZN_BACKEND_BASE_URL="${LOCAL_URL}" \
  RZN_PLUGIN_PUBLIC_BASE_URL="${LOCAL_PUBLIC_URL}" \
  RZN_PLATFORM_ADMIN_TOKEN="${LOCAL_TOKEN}" \
  python3 "${ROOT}/scripts/publish_python_tools_variants.py" "$@"; then
  echo "local publish failed: ${LOCAL_URL}" >&2
  exit 1
fi

echo "==> publishing python-tools variants to cloud backend: ${CLOUD_URL}"
if ! env \
  RZN_BACKEND_BASE_URL="${CLOUD_URL}" \
  RZN_PLUGIN_PUBLIC_BASE_URL="${CLOUD_PUBLIC_URL}" \
  RZN_PLATFORM_ADMIN_TOKEN="${CLOUD_TOKEN}" \
  python3 "${ROOT}/scripts/publish_python_tools_variants.py" --skip-build "$@"; then
  echo "cloud publish failed: ${CLOUD_URL}" >&2
  exit 1
fi

echo "publish completed for local and cloud"
