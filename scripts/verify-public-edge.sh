#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
origin="${1:-https://atinycivilization.com}"
if [[ "$origin" != "https://atinycivilization.com" ]]; then
  echo "public edge verification accepts only https://atinycivilization.com" >&2
  exit 2
fi

verify_headers() {
  local path="$1"
  local headers
  headers="$(curl --fail --silent --show-error \
    --connect-timeout 10 --max-time 20 \
    --dump-header - --output /dev/null "${origin}${path}")"
  python3 "${project_root}/scripts/verify-public-edge-headers.py" "$path" <<<"$headers"
}

redirect="$(curl --silent --show-error --head \
  --connect-timeout 10 --max-time 20 \
  --write-out '%{http_code}|%{redirect_url}' --output /dev/null \
  http://atinycivilization.com/)"
IFS='|' read -r redirect_status redirect_url <<<"$redirect"
if [[ "$redirect_status" != "301" && "$redirect_status" != "302" \
   && "$redirect_status" != "307" && "$redirect_status" != "308" ]]; then
  echo "public HTTP endpoint did not redirect: ${redirect_status:-none}" >&2
  exit 1
fi
if [[ "$redirect_url" != "https://atinycivilization.com/" ]]; then
  echo "public HTTP endpoint redirected outside the canonical origin: ${redirect_url:-none}" >&2
  exit 1
fi

verify_headers "/"
verify_headers "/wiki"
verify_headers "/api/v1/status"

homepage="$(curl --fail --silent --show-error --max-time 20 "${origin}/")"
if [[ "$homepage" != *"A Tiny Civilization"* ]]; then
  echo "public homepage does not identify A Tiny Civilization" >&2
  exit 1
fi
curl --fail --silent --show-error --max-time 20 "${origin}/api/v1/status" \
  | python3 -c 'import json, sys; document=json.load(sys.stdin); assert isinstance(document, dict)'

echo "Public HTTPS redirect, security headers, pages, and observer status are healthy."
