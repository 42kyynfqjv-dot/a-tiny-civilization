#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
preflight="${project_root}/scripts/production-port-preflight.sh"
private_preflight="${project_root}/scripts/production-private-database-preflight.sh"
deployment="${project_root}/scripts/deploy-production-app.sh"
database_preparation="${project_root}/scripts/prepare-production-genesis-database.sh"

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
  'failure_count=$((failure_count + 1))'
  'conflicting listener or protected-volume consumer(s)'
)
for contract in "${required[@]}"; do
  if ! grep -Fq "$contract" "$preflight"; then
    echo "production port preflight lost required contract: $contract" >&2
    exit 1
  fi
done

for contract in \
  'production-preflight.sh" --env-file' \
  'config --format json' \
  'int(port.get("target", 0)) == 5432' \
  'mapping.get("host_ip") != "127.0.0.1"' \
  "postgres_volume='a-tiny-civilization-postgres-v1'" \
  'docker ps --filter "publish=${database_port}"' \
  'docker ps --filter "volume=${postgres_volume}"'; do
  if ! grep -Fq "$contract" "$private_preflight"; then
    echo "private database port preflight lost required contract: $contract" >&2
    exit 1
  fi
done
if rg -q "'3000:web'|'8080:api'|atiny-ollama|hindsight-data" "$private_preflight"; then
  echo "private database preflight gained a public-service or cognition cutover dependency" >&2
  exit 1
fi
if ! rg -q 'production-private-database-preflight\.sh.*--env-file' "$database_preparation"; then
  echo "private database preparation does not use its narrow port/volume guard" >&2
  exit 1
fi
if rg -q 'production-port-preflight\.sh' "$database_preparation"; then
  echo "private database preparation still requires the full public cutover guard" >&2
  exit 1
fi

port_line="$(rg -n -m1 'production-port-preflight\.sh' "$deployment")"
mutation_line="$(rg -n -m1 'compose_args\[@.*build api web' "$deployment")"
port_number="${port_line%%:*}"
mutation_number="${mutation_line%%:*}"
if ((port_number >= mutation_number)); then
  echo "production port preflight must precede every Compose mutation" >&2
  exit 1
fi

echo "Production cutover rejects foreign loopback listeners and protected-volume consumers before mutation."
