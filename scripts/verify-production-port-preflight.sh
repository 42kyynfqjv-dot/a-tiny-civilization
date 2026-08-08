#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
preflight="${project_root}/scripts/production-port-preflight.sh"
deployment="${project_root}/scripts/deploy-production-app.sh"

required=(
  "expected_project='a-tiny-civilization'"
  "'3000:web'"
  "'5432:db'"
  "'8080:api'"
  "'atiny-ollama'"
  'docker ps --filter "publish=${port}"'
  'docker ps --filter "volume=${volume}"'
  'com.docker.compose.project'
  'stop the legacy/dev listener before deployment'
)
for contract in "${required[@]}"; do
  if ! grep -Fq "$contract" "$preflight"; then
    echo "production port preflight lost required contract: $contract" >&2
    exit 1
  fi
done

port_line="$(rg -n -m1 'production-port-preflight\.sh' "$deployment")"
mutation_line="$(rg -n -m1 'compose_args\[@.*build migrate' "$deployment")"
port_number="${port_line%%:*}"
mutation_number="${mutation_line%%:*}"
if ((port_number >= mutation_number)); then
  echo "production port preflight must precede every Compose mutation" >&2
  exit 1
fi

echo "Production cutover rejects foreign loopback listeners and protected-volume consumers before mutation."
