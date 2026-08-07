#!/usr/bin/env bash
set -euo pipefail

required=(
  POSTGRES_DB
  POSTGRES_USER
  POSTGRES_PASSWORD
  CLOUDFLARE_TUNNEL_TOKEN
  R2_ACCOUNT_ID
  R2_BACKUP_BUCKET
  R2_ACCESS_KEY_ID
  R2_SECRET_ACCESS_KEY
  WALG_LIBSODIUM_KEY
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

if [[ ! "${R2_ACCOUNT_ID}" =~ ^[[:xdigit:]]{32}$ ]]; then
  echo "R2_ACCOUNT_ID must be a 32-character hexadecimal Cloudflare account ID" >&2
  exit 2
fi

if [[ ! "${R2_BACKUP_BUCKET}" =~ ^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$ ]]; then
  echo "R2_BACKUP_BUCKET must be a valid 3-63 character bucket name" >&2
  exit 2
fi

if [[ ! "${WALG_LIBSODIUM_KEY}" =~ ^[[:xdigit:]]{64}$ ]]; then
  echo "WALG_LIBSODIUM_KEY must be a 32-byte key encoded as 64 hexadecimal characters" >&2
  exit 2
fi

cloudflared_image="${CLOUDFLARED_IMAGE:-cloudflare/cloudflared:2026.7.2@sha256:4f6655284ab3d252b7f28fedb19fe6c8fc82ee5b1295c20ac74d475e5398a52d}"
if [[ ! "${cloudflared_image}" =~ @sha256:[[:xdigit:]]{64}$ ]]; then
  echo "CLOUDFLARED_IMAGE must include an immutable sha256 manifest digest" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

"${compose_command[@]}" \
  -f compose.yaml \
  -f compose.backup.yaml \
  -f compose.tunnel.yaml \
  config --quiet

RESTORE_WORLD_ID=00000000-0000-0000-0000-000000000001 \
  "${compose_command[@]}" -f compose.restore.yaml config --quiet

echo "Production configuration passes static preflight."
