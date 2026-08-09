#!/usr/bin/env bash
set -euo pipefail

# Commit the admitted tick-zero world into an already prepared private production database. This
# wrapper loads credentials without printing them and cannot deploy or start any service.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
genesis_directory="${ATINY_CANONICAL_GENESIS_DIRECTORY:-}"
evidence_directory="${ATINY_QUALIFICATION_EVIDENCE_DIRECTORY:-}"
quality_admission="${ATINY_QUALITY_ADMISSION_FILE:-${project_root}/docs/operations/QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json}"
confirmed=0

while (($#)); do
  case "$1" in
    --env-file)
      environment_file="${2:-}"
      shift 2
      ;;
    --genesis-directory)
      genesis_directory="${2:-}"
      shift 2
      ;;
    --evidence-directory)
      evidence_directory="${2:-}"
      shift 2
      ;;
    --confirm-experimental-genesis)
      confirmed=1
      shift
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path/to/production.env] --genesis-directory /absolute/path --evidence-directory /absolute/path --confirm-experimental-genesis" >&2
      exit 2
      ;;
  esac
done
if ((confirmed != 1)); then
  echo "production genesis requires the literal --confirm-experimental-genesis argument" >&2
  exit 2
fi
if [[ -z "$genesis_directory" || -z "$evidence_directory" ]]; then
  echo "production genesis requires the exact qualified genesis and evidence directories" >&2
  exit 2
fi
if [[ "$quality_admission" != /* || ! -f "$quality_admission" || -L "$quality_admission" ]]; then
  echo "production genesis requires an absolute, regular quality-admission file" >&2
  exit 2
fi
if ((EUID != 0)); then
  echo "run production genesis as root; it reads a root-protected environment file" >&2
  exit 2
fi

cd "$project_root"
"${project_root}/scripts/verify-production-checkout.sh"
ATINY_QUALITY_ADMISSION_FILE="$quality_admission" \
  "${project_root}/scripts/public-genesis-preflight.sh" \
  --env-file "$environment_file" \
  --genesis-directory "$genesis_directory" \
  --evidence-directory "$evidence_directory" \
  --runtime-root "${project_root}/runtime-artifacts"

# This parser validates ownership, permissions, literal values, production settings, and Compose
# interpolation before exporting the protected values into this process. It never prints them.
# shellcheck source=/dev/null
source "${project_root}/scripts/production-preflight.sh" --env-file "$environment_file"

database_port="${POSTGRES_PORT:-5432}"
if [[ ! "$database_port" =~ ^[1-9][0-9]{0,4}$ ]] || ((database_port > 65535)); then
  echo "POSTGRES_PORT must be an integer from 1 through 65535" >&2
  exit 2
fi
export DATABASE_URL="$(python3 -c '
import os
from urllib.parse import quote
user = quote(os.environ["POSTGRES_USER"], safe="")
password = quote(os.environ["POSTGRES_PASSWORD"], safe="")
database = quote(os.environ["POSTGRES_DB"], safe="")
port = os.environ.get("POSTGRES_PORT", "5432")
print(f"postgres://{user}:{password}@127.0.0.1:{port}/{database}")
')"

"${project_root}/scripts/activate-qualified-canonical-world.sh" activate \
  "${project_root}/docs/operations/CANONICAL_SEED_COMMITMENT.json" \
  "${project_root}/docs/operations/CANONICAL_SEED_RESOLUTION.json" \
  "$genesis_directory" \
  "$evidence_directory" \
  "$quality_admission" \
  --confirm-experimental-genesis

unset DATABASE_URL POSTGRES_PASSWORD
echo "Production tick-zero genesis is committed privately; no service or public route was started."
