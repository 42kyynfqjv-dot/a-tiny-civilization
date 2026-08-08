#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
deployment="${project_root}/scripts/deploy-production-app.sh"
runtime_verifier="${project_root}/scripts/verify-staged-runtime-artifacts.sh"

preflight_line="$(rg -n -m1 'public-genesis-preflight\.sh' "$deployment")"
mutation_line="$(rg -n -m1 'compose_args\[@.*build migrate' "$deployment")"
public_edge_line="$(rg -n -m1 'verify-public-edge\.sh.*https://atinycivilization\.com' "$deployment")"
private_foundation_line="$(rg -n -m1 'up -d db migrate local-cognition hindsight' "$deployment")"
world_guard_line="$(rg -n -m1 'public deployment requires exactly one privately activated qualified world' "$deployment")"
canonical_start_line="$(rg -n -m1 '^  api projector runner memory-worker cognition-worker' "$deployment")"
preflight_number="${preflight_line%%:*}"
mutation_number="${mutation_line%%:*}"
public_edge_number="${public_edge_line%%:*}"
private_foundation_number="${private_foundation_line%%:*}"
world_guard_number="${world_guard_line%%:*}"
canonical_start_number="${canonical_start_line%%:*}"
if ((preflight_number >= mutation_number)); then
  echo "composed public-genesis preflight must precede every Compose mutation" >&2
  exit 1
fi
if ((public_edge_number <= mutation_number)); then
  echo "public edge verification must run after deployment mutation and local smoke checks" >&2
  exit 1
fi
if ((private_foundation_number >= world_guard_number || world_guard_number >= canonical_start_number)); then
  echo "deployment must validate the privately activated world before canonical services start" >&2
  exit 1
fi
for contract in \
  'deployed_world_id.*expected_world_id' \
  'deployed_ruleset.*expected_ruleset' \
  'deployed_status.*running' \
  'deployment lost its single running world after service startup'; do
  if ! rg -q "$contract" "$deployment"; then
    echo "production deployment lost qualified-world contract: $contract" >&2
    exit 1
  fi
done
if ! rg -q 'requires the literal --confirm-public-deployment argument' "$deployment"; then
  echo "production deployment lost its explicit confirmation boundary" >&2
  exit 1
fi
for contract in '--genesis-directory' '--evidence-directory' '--runtime-root'; do
  if ! rg -q -- "$contract" "$deployment"; then
    echo "production deployment lost required public-genesis input: $contract" >&2
    exit 1
  fi
done

required_contracts=(
  'validate-provisional'
  'find.*-type l'
  'find.*-perm /0022'
  'de441_part-1\.bsp.*13757827f5db41b835a24bbd637488636ce79a8ca754062fed17844f7d5b618e.*1651119104'
  'de441_part-2\.bsp.*3abb17dae2d78dd34880377544aacb54892104a0d4462b322cb9f4454d4887f6.*1656830976'
)
for contract in "${required_contracts[@]}"; do
  if ! rg -q "$contract" "$runtime_verifier"; then
    echo "runtime artifact verifier lost required contract: $contract" >&2
    exit 1
  fi
done

echo "Production deployment revalidates immutable inputs before mutation and the public edge after smoke checks."
