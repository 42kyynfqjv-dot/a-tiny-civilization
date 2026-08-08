#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gate="${project_root}/scripts/verify-live-genesis.sh"

required=(
  'current_tick'
  'current_sequence'
  'memory_outbox'
  'completed_at IS NULL'
  'last_error IS NOT NULL'
  'observer-candidate-smoke\.sh'
  'civilization-runner verify-world'
  'backend-status\.sh'
)
for contract in "${required[@]}"; do
  if ! rg -q "$contract" "$gate"; then
    echo "live-genesis gate lost required contract: $contract" >&2
    exit 1
  fi
done
if rg -q 'initialize|advance-qualification|docker compose.*(up|build)|deploy-production-app' "$gate"; then
  echo "live-genesis verification gained a mutating operation" >&2
  exit 1
fi

echo "Live-genesis verification waits for real progress and remains read-only."
