#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--env-file" ]]; then
  environment_file="${2:-}"
  shift 2
  if (($#)); then
    echo "usage: $0 [--env-file /absolute/path/to/production.env]" >&2
    exit 2
  fi
  if [[ -z "$environment_file" || "$environment_file" != /* \
        || ! -f "$environment_file" || -L "$environment_file" ]]; then
    echo "production environment file must be an absolute, regular, non-symlink file" >&2
    exit 2
  fi
  owner_id="$(stat -c '%u' "$environment_file")"
  permissions="$(stat -c '%a' "$environment_file")"
  if [[ "$owner_id" != "0" && "$owner_id" != "$EUID" ]]; then
    echo "production environment file must be owned by root or the current operator" >&2
    exit 2
  fi
  if [[ ! "$permissions" =~ ^[0-7]?[0-7]00$ ]]; then
    echo "production environment file must not be accessible by group or other users" >&2
    exit 2
  fi

  declare -A loaded_names=()
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
    if [[ "$line" == export\ * ]]; then
      line="${line#export }"
    fi
    if [[ ! "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
      echo "production environment contains an invalid assignment" >&2
      exit 2
    fi
    name="${BASH_REMATCH[1]}"
    value="${BASH_REMATCH[2]}"
    if [[ -n "${loaded_names[$name]:-}" ]]; then
      echo "production environment contains duplicate setting: $name" >&2
      exit 2
    fi
    loaded_names[$name]=1
    if [[ "$value" == \'*\' && ${#value} -ge 2 ]]; then
      value="${value:1:${#value}-2}"
    elif [[ "$value" == \"*\" && ${#value} -ge 2 ]]; then
      value="${value:1:${#value}-2}"
    elif [[ "$value" == *'${'* || "$value" == *'$('* || "$value" == *'`'* ]]; then
      echo "production environment values must be literal; quote metacharacters" >&2
      exit 2
    fi
    export "$name=$value"
  done < "$environment_file"
elif (($#)); then
  echo "usage: $0 [--env-file /absolute/path/to/production.env]" >&2
  exit 2
fi

required=(
  POSTGRES_DB
  POSTGRES_USER
  POSTGRES_PASSWORD
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

if [[ -z "${CLOUDFLARE_WORKERS_AI_API_KEY:-}" \
      && -z "${GROQ_API_KEY:-}" \
      && -z "${CEREBRAS_API_KEY:-}" \
      && -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "production requires at least one configured cognition provider" >&2
  exit 2
fi
if [[ "${COGNITION_EXTERNAL_EXPORT_APPROVED:-false}" != "true" ]]; then
  echo "production cognition providers require COGNITION_EXTERNAL_EXPORT_APPROVED=true" >&2
  exit 2
fi
if [[ -n "${CLOUDFLARE_WORKERS_AI_API_KEY:-}" \
      && -z "${CLOUDFLARE_WORKERS_AI_BASE_URL:-}" ]] \
   || [[ -z "${CLOUDFLARE_WORKERS_AI_API_KEY:-}" \
      && -n "${CLOUDFLARE_WORKERS_AI_BASE_URL:-}" ]]; then
  echo "Cloudflare Workers AI requires both its account-scoped base URL and API key" >&2
  exit 2
fi
case "${COGNITION_PAID_ENABLED:-false}" in
  true)
    if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
      echo "paid cognition requires OPENROUTER_API_KEY" >&2
      exit 2
    fi
    ;;
  false) ;;
  *)
    echo "COGNITION_PAID_ENABLED must be true or false" >&2
    exit 2
    ;;
esac

if [[ -n "${GOOGLE_OAUTH_CLIENT_ID:-}" && -z "${GOOGLE_OAUTH_CLIENT_SECRET:-}" ]] \
   || [[ -z "${GOOGLE_OAUTH_CLIENT_ID:-}" && -n "${GOOGLE_OAUTH_CLIENT_SECRET:-}" ]]; then
  echo "Google OAuth client ID and secret must be configured together" >&2
  exit 2
fi

apple_auth_values=0
for name in APPLE_CLIENT_ID APPLE_TEAM_ID APPLE_KEY_ID APPLE_PRIVATE_KEY; do
  if [[ -n "${!name:-}" ]]; then
    apple_auth_values=$((apple_auth_values + 1))
  fi
done
if [[ "${apple_auth_values}" -gt 0 && "${apple_auth_values}" -ne 4 ]]; then
  echo "Sign in with Apple requires client, team, key ID, and private key together" >&2
  exit 2
fi

stripe_checkout_values=0
for name in STRIPE_SECRET_KEY STRIPE_SUPPORTER_PRICE_ID; do
  if [[ -n "${!name:-}" ]]; then
    stripe_checkout_values=$((stripe_checkout_values + 1))
  fi
done
if [[ "${stripe_checkout_values}" -gt 0 ]]; then
  if [[ "${stripe_checkout_values}" -ne 2 ]]; then
    echo "Stripe Checkout requires both the secret key and supporter price ID" >&2
    exit 2
  fi
  if [[ -z "${STRIPE_WEBHOOK_SECRET:-}" ]]; then
    echo "Stripe Checkout requires the signed webhook endpoint" >&2
    exit 2
  fi
  if [[ -z "${GOOGLE_OAUTH_CLIENT_ID:-}" && "${apple_auth_values}" -ne 4 ]]; then
    echo "Stripe Checkout requires observer sign-in" >&2
    exit 2
  fi
  if [[ ! "${ATINY_MODERATOR_ID:-}" =~ ^[A-Za-z0-9._:@/-]{1,128}$ ]]; then
    echo "Stripe Checkout requires a stable ATINY_MODERATOR_ID" >&2
    exit 2
  fi
fi

backup_names=(
  R2_ACCOUNT_ID
  R2_BACKUP_BUCKET
  R2_ACCESS_KEY_ID
  R2_SECRET_ACCESS_KEY
  WALG_LIBSODIUM_KEY
)
backup_values=0
for name in "${backup_names[@]}"; do
  if [[ -n "${!name:-}" ]]; then
    backup_values=$((backup_values + 1))
  fi
done
require_backup="${ATINY_REQUIRE_OFFSITE_BACKUP:-0}"
if [[ "${require_backup}" != "0" && "${require_backup}" != "1" ]]; then
  echo "ATINY_REQUIRE_OFFSITE_BACKUP must be 0 or 1" >&2
  exit 2
fi
if [[ "${require_backup}" == "1" || "${backup_values}" -gt 0 ]]; then
  if [[ "${backup_values}" -ne "${#backup_names[@]}" ]]; then
    echo "offsite backup configuration must provide every R2/WAL-G setting" >&2
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
fi

cloudflared_image="${CLOUDFLARED_IMAGE:-cloudflare/cloudflared:2026.7.2@sha256:4f6655284ab3d252b7f28fedb19fe6c8fc82ee5b1295c20ac74d475e5398a52d}"
if [[ ! "${cloudflared_image}" =~ @sha256:[[:xdigit:]]{64}$ ]]; then
  echo "CLOUDFLARED_IMAGE must include an immutable sha256 manifest digest" >&2
  exit 2
fi

require_compose_tunnel="${ATINY_REQUIRE_COMPOSE_TUNNEL:-0}"
if [[ "${require_compose_tunnel}" != "0" && "${require_compose_tunnel}" != "1" ]]; then
  echo "ATINY_REQUIRE_COMPOSE_TUNNEL must be 0 or 1" >&2
  exit 2
fi
if [[ "${require_compose_tunnel}" == "1" && -z "${CLOUDFLARE_TUNNEL_TOKEN:-}" ]]; then
  echo "CLOUDFLARE_TUNNEL_TOKEN is required for the Compose-managed tunnel" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

compose_files=()
if [[ -n "${environment_file:-}" ]]; then
  compose_files+=(--env-file "$environment_file")
fi
compose_files+=(-f compose.yaml -f compose.hindsight.yaml)
if [[ "${require_compose_tunnel}" == "1" || -n "${CLOUDFLARE_TUNNEL_TOKEN:-}" ]]; then
  compose_files+=(-f compose.tunnel.yaml)
fi
if [[ "${backup_values}" -eq "${#backup_names[@]}" ]]; then
  compose_files+=(-f compose.backup.yaml)
fi
"${compose_command[@]}" "${compose_files[@]}" config --quiet

if [[ "${backup_values}" -eq "${#backup_names[@]}" ]]; then
  RESTORE_WORLD_ID=00000000-0000-0000-0000-000000000001 \
    "${compose_command[@]}" -f compose.restore.yaml config --quiet
else
  echo "Offsite backup is intentionally not configured for this deployment."
fi

echo "Production configuration passes static preflight."
