#!/usr/bin/env bash
set -euo pipefail

api_url="${API_SMOKE_URL:-http://127.0.0.1:8080}"
web_url="${WEB_SMOKE_URL:-http://127.0.0.1:3000}"

curl --fail --silent --show-error "$api_url/health/live" >/dev/null
curl --fail --silent --show-error "$api_url/health/ready" >/dev/null
curl --fail --silent --show-error "$api_url/api/v1/status" >/dev/null
curl --fail --silent --show-error "$web_url/" >/dev/null
curl --fail --silent --show-error "$web_url/api/v1/status" >/dev/null

echo "Foundation smoke checks passed."
