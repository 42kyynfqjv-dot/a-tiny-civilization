#!/usr/bin/env bash
set -euo pipefail

required=(
  POSTGRES_DB
  POSTGRES_USER
  POSTGRES_PASSWORD
  CLOUDFLARE_TUNNEL_TOKEN
)

for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    echo "missing required production setting: ${name}" >&2
    exit 2
  fi
done

if [[ "${POSTGRES_PASSWORD}" == "local-development-only" ]]; then
  echo "POSTGRES_PASSWORD must not use the documented local development value" >&2
  exit 2
fi

if [[ "${APP_ENV:-production}" != "production" ]]; then
  echo "APP_ENV must be production" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

"${compose_command[@]}" -f compose.yaml -f compose.tunnel.yaml config --quiet

echo "Production configuration passes static preflight."
