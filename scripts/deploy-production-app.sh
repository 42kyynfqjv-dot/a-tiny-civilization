#!/usr/bin/env bash
set -euo pipefail

# Deploy the application containers on the single production host. This deliberately
# excludes the optional Docker tunnel and backup profiles: this host currently runs
# its Cloudflare tunnel as a separate system service. Hindsight and both asynchronous
# workers are part of the application deployment because cognition is a genesis gate.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"

if [[ "${1:-}" == "--env-file" ]]; then
  environment_file="${2:-}"
  shift 2
fi
if (($#)); then
  echo "usage: $0 [--env-file /absolute/path/to/production.env]" >&2
  exit 2
fi

if (( EUID != 0 )); then
  echo "run this deployment helper as root; it reads a root-protected environment file" >&2
  exit 2
fi
if [[ ! -f "$environment_file" || -L "$environment_file" ]]; then
  echo "production environment file is absent or unsafe: $environment_file" >&2
  exit 2
fi
if [[ "$environment_file" != /* ]]; then
  echo "production environment file must be an absolute path" >&2
  exit 2
fi

required=(APP_ENV POSTGRES_DB POSTGRES_USER POSTGRES_PASSWORD)
for name in "${required[@]}"; do
  if ! grep -qE "^${name}=.+$" "$environment_file"; then
    echo "production environment is missing $name" >&2
    exit 2
  fi
done
if ! grep -qx 'APP_ENV=production' "$environment_file"; then
  echo "APP_ENV must be production" >&2
  exit 2
fi
if grep -qx 'POSTGRES_PASSWORD=local-development-only' "$environment_file"; then
  echo "production database password uses the documented development value" >&2
  exit 2
fi
if ! grep -qE '^(CLOUDFLARE_WORKERS_AI_API_KEY|GROQ_API_KEY|CEREBRAS_API_KEY|OPENROUTER_API_KEY)=.+$' "$environment_file"; then
  echo "production environment requires at least one cognition provider key" >&2
  exit 2
fi
if grep -qx 'COGNITION_PAID_ENABLED=true' "$environment_file" \
   && ! grep -qE '^OPENROUTER_API_KEY=.+$' "$environment_file"; then
  echo "paid cognition requires OPENROUTER_API_KEY" >&2
  exit 2
fi
if grep -qE '^CLOUDFLARE_WORKERS_AI_API_KEY=.+$' "$environment_file" \
   && ! grep -qE '^CLOUDFLARE_WORKERS_AI_BASE_URL=.+$' "$environment_file"; then
  echo "Cloudflare Workers AI requires its account-scoped base URL" >&2
  exit 2
fi
if grep -qE '^CLOUDFLARE_WORKERS_AI_BASE_URL=.+$' "$environment_file" \
   && ! grep -qE '^CLOUDFLARE_WORKERS_AI_API_KEY=.+$' "$environment_file"; then
  echo "Cloudflare Workers AI requires its API key" >&2
  exit 2
fi
if grep -qE '^COGNITION_PAID_ENABLED=' "$environment_file" \
   && ! grep -qEx 'COGNITION_PAID_ENABLED=(true|false)' "$environment_file"; then
  echo "COGNITION_PAID_ENABLED must be true or false" >&2
  exit 2
fi

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)
cd "$project_root"

"${compose_command[@]}" "${compose_args[@]}" config --quiet
"${compose_command[@]}" "${compose_args[@]}" build migrate api projector runner web
"${compose_command[@]}" "${compose_args[@]}" up -d \
  db migrate hindsight api projector runner memory-worker cognition-worker
# Avoid recreating API dependencies with accidental Compose defaults while updating the
# public web container. Never use --remove-orphans: the application and any separately
# managed tunnel may span more than one Compose profile.
"${compose_command[@]}" "${compose_args[@]}" up --no-deps -d web

for _ in {1..30}; do
  if "${compose_command[@]}" "${compose_args[@]}" exec -T api \
    curl --fail --silent http://localhost:8080/health/ready >/dev/null; then
    echo "Production application deployment is ready."
    exit 0
  fi
  sleep 1
done

echo "application containers started but observer API readiness did not succeed within 30 seconds" >&2
exit 1
