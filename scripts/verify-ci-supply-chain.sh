#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workflow="${project_root}/.github/workflows/ci.yml"

if rg -n 'uses: [^ ]+@(v[0-9]+|master|main|stable)([[:space:]]|$)' "$workflow"; then
  echo "CI contains a mutable GitHub Action reference" >&2
  exit 1
fi
required=(
  'cargo install cargo-audit --version 0.22.2 --locked'
  './scripts/audit-dependencies.sh rust'
  '../scripts/audit-dependencies.sh web'
  'postgres:17-alpine@sha256:742f40ea20b9ff2ff31db5458d127452988a2164df9e17441e191f3b72252193'
  './scripts/verify-container-image-pins.sh'
)
for contract in "${required[@]}"; do
  if ! rg -qF "$contract" "$workflow"; then
    echo "CI supply-chain contract is absent: $contract" >&2
    exit 1
  fi
done

echo "CI actions, service images, and dependency advisory gates are immutable and enforced."
