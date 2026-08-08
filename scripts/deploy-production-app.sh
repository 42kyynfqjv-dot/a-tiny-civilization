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

compose_command=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_command=(docker-compose)
fi

compose_args=(--env-file "$environment_file" -f compose.yaml -f compose.hindsight.yaml)
cd "$project_root"

# Validate the exact file that Compose will consume. Keeping one validator prevents the
# deployment helper and documented/manual preflight from silently accepting different setups.
"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file"

"${compose_command[@]}" "${compose_args[@]}" config --quiet
"${compose_command[@]}" "${compose_args[@]}" build migrate api projector runner web
"${compose_command[@]}" "${compose_args[@]}" up -d \
  db migrate local-cognition hindsight api projector runner memory-worker cognition-worker
# Avoid recreating API dependencies with accidental Compose defaults while updating the
# public web container. Never use --remove-orphans: the application and any separately
# managed tunnel may span more than one Compose profile.
"${compose_command[@]}" "${compose_args[@]}" up --no-deps -d web

"${project_root}/scripts/backend-status.sh" --env-file "$environment_file" --wait-seconds 60
