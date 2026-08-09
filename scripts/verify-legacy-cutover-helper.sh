#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
helper="${project_root}/scripts/stop-legacy-public-stack-for-cutover.sh"

required=(
  "legacy_project='emergent-civilization'"
  "production_project='a-tiny-civilization'"
  '--check'
  '--confirm-legacy-public-cutover'
  'com.docker.compose.project.working_dir'
  'service_is_allowed'
  'private_production_service_is_allowed'
  'allowed_private_production_services=(db migrate)'
  'refusing to stop unknown service'
  'refusing ambiguous legacy service'
  'refusing legacy cutover while production service'
  'no state changed'
  'Planned stop order:'
  'docker stop --time 60'
  'production-port-preflight.sh'
  'without removing containers or volumes'
)
for contract in "${required[@]}"; do
  if ! grep -Fq -- "$contract" "$helper"; then
    echo "legacy cutover helper lost required contract: $contract" >&2
    exit 1
  fi
done

if rg -n '\bdocker (compose )?(down|rm)\b|\bdocker volume (rm|prune)\b|\bdocker container rm\b' "$helper"; then
  echo "legacy cutover helper gained a destructive Docker operation" >&2
  exit 1
fi

if "$helper" >/dev/null 2>&1; then
  echo "legacy cutover helper accepted a call without literal confirmation" >&2
  exit 1
fi

echo "Legacy public cutover is label-scoped, confirmation-gated, and non-destructive."
