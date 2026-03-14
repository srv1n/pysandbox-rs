#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCAL_URL="${RZN_BACKEND_BASE_URL_LOCAL:-http://localhost:8082}"
PROD_URL="${RZN_BACKEND_BASE_URL_PROD:-https://rzn.ai}"
LOCAL_TOKEN="${RZN_PLATFORM_ADMIN_TOKEN_LOCAL:-${RZN_PLATFORM_ADMIN_TOKEN:-}}"
PROD_TOKEN="${RZN_PLATFORM_ADMIN_TOKEN_PROD:-${RZN_PLATFORM_ADMIN_TOKEN:-}}"

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'EOF'
Usage: bash scripts/publish_python_tools_variants_local_and_prod.sh [publish_python_tools_variants.py args...]

Publishes python-tools variants to:
  1. local backend   (default: http://localhost:8082)
  2. production      (default: https://rzn.ai)

Environment:
  RZN_BACKEND_BASE_URL_LOCAL   Optional local base URL override
  RZN_BACKEND_BASE_URL_PROD    Optional production base URL override
  RZN_PLATFORM_ADMIN_TOKEN_LOCAL
  RZN_PLATFORM_ADMIN_TOKEN_PROD

Fallback:
  RZN_PLATFORM_ADMIN_TOKEN may be used for both targets if appropriate.
EOF
  exit 0
fi

if [[ -z "${LOCAL_TOKEN}" ]]; then
  echo "missing RZN_PLATFORM_ADMIN_TOKEN_LOCAL (or shared RZN_PLATFORM_ADMIN_TOKEN)" >&2
  exit 2
fi

if [[ -z "${PROD_TOKEN}" ]]; then
  echo "missing RZN_PLATFORM_ADMIN_TOKEN_PROD (or shared RZN_PLATFORM_ADMIN_TOKEN)" >&2
  exit 2
fi

echo "==> publishing python-tools variants to local backend: ${LOCAL_URL}"
if ! env \
  RZN_BACKEND_BASE_URL="${LOCAL_URL}" \
  RZN_PLATFORM_ADMIN_TOKEN="${LOCAL_TOKEN}" \
  python3 "${ROOT}/scripts/publish_python_tools_variants.py" "$@"; then
  echo "local publish failed: ${LOCAL_URL}" >&2
  exit 1
fi

echo "==> publishing python-tools variants to production backend: ${PROD_URL}"
if ! env \
  RZN_BACKEND_BASE_URL="${PROD_URL}" \
  RZN_PLATFORM_ADMIN_TOKEN="${PROD_TOKEN}" \
  python3 "${ROOT}/scripts/publish_python_tools_variants.py" --skip-build --skip-upload "$@"; then
  echo "production publish failed: ${PROD_URL}" >&2
  exit 1
fi

echo "publish completed for local and production"
