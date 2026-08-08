#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
preflight="${project_root}/scripts/public-genesis-preflight.sh"

required=(
  'production-preflight\.sh.*--env-file'
  'verify-launch-operations\.sh'
  'activate-qualified-canonical-world\.sh.*verify'
  'verify-public-observatory-admission\.py'
  'PUBLIC_OBSERVATORY_ADMISSION'
  'CANONICAL_SEED_COMMITMENT\.json'
  'CANONICAL_SEED_RESOLUTION\.json'
  'verify-staged-runtime-artifacts\.sh'
  'passed without creating a world, changing services, or deploying a site'
)
for contract in "${required[@]}"; do
  if ! rg -q "$contract" "$preflight"; then
    echo "public-genesis preflight lost required contract: $contract" >&2
    exit 1
  fi
done
if rg -q 'docker compose.*(up|build)|initialize-canonical-world|deploy-production-app' "$preflight"; then
  echo "public-genesis preflight gained a mutating operation" >&2
  exit 1
fi

operations="${project_root}/scripts/verify-launch-operations.sh"
for contract in \
  'verify-supporter-production-policy\.sh' \
  'verify-postgres-durability\.sh' \
  'verify-container-privilege-policy\.sh' \
  'verify-production-runtime-gate\.sh' \
  'verify-live-genesis-gate\.sh' \
  'verify-backend-status\.sh' \
  'verify-production-monitor-installer\.sh' \
  'verify-hindsight-runtime\.sh' \
  'Every read-only production operations gate passes'; do
  if ! rg -q "$contract" "$operations"; then
    echo "launch operations suite lost required contract: $contract" >&2
    exit 1
  fi
done
if rg -q 'docker compose.*(up|build)|initialize|activate|deploy-production-app' "$operations"; then
  echo "launch operations suite gained a mutating operation" >&2
  exit 1
fi
if rg -q '\bcargo\b|check-boundaries\.sh' "$operations"; then
  echo "host launch operations suite gained a CI-only Rust toolchain dependency" >&2
  exit 1
fi

echo "Public-genesis preflight composes every read-only launch gate without mutation."
