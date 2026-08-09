#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
deployment="${project_root}/scripts/deploy-production-app.sh"
database_preparation="${project_root}/scripts/prepare-production-genesis-database.sh"
production_activation="${project_root}/scripts/activate-production-genesis.sh"
runtime_verifier="${project_root}/scripts/verify-staged-runtime-artifacts.sh"
checkout_verifier="${project_root}/scripts/verify-production-checkout.sh"

preflight_line="$(rg -n -m1 'public-genesis-preflight\.sh' "$deployment")"
checkout_line="$(rg -n -m1 'verify-production-checkout\.sh' "$deployment")"
mutation_line="$(rg -n -m1 'compose_args\[@.*build api web' "$deployment")"
public_edge_line="$(rg -n -m1 'verify-public-edge\.sh.*https://atinycivilization\.com' "$deployment")"
live_genesis_line="$(rg -n -m1 'verify-live-genesis\.sh' "$deployment")"
private_foundation_line="$(rg -n -m1 'up -d db migrate local-cognition hindsight' "$deployment")"
world_guard_line="$(rg -n -m1 -- '--mode require-running' "$deployment")"
canonical_start_line="$(rg -n -m1 '^  api projector runner memory-worker cognition-worker' "$deployment")"
preflight_number="${preflight_line%%:*}"
checkout_number="${checkout_line%%:*}"
mutation_number="${mutation_line%%:*}"
public_edge_number="${public_edge_line%%:*}"
live_genesis_number="${live_genesis_line%%:*}"
private_foundation_number="${private_foundation_line%%:*}"
world_guard_number="${world_guard_line%%:*}"
canonical_start_number="${canonical_start_line%%:*}"
if ((preflight_number >= mutation_number)); then
  echo "composed public-genesis preflight must precede every Compose mutation" >&2
  exit 1
fi
if ((checkout_number >= mutation_number)); then
  echo "production deployment must reject checkout drift before mutation" >&2
  exit 1
fi
if ((public_edge_number <= mutation_number)); then
  echo "public edge verification must run after deployment mutation and local smoke checks" >&2
  exit 1
fi
if ((live_genesis_number <= canonical_start_number || live_genesis_number >= public_edge_number)); then
  echo "deployment must replay-verify live tick-one progress before checking the public edge" >&2
  exit 1
fi
for contract in '--world-id.*expected_world_id' '--wait-seconds 300'; do
  if ! rg -q -- "$contract" "$deployment"; then
    echo "production deployment lost live-genesis contract: $contract" >&2
    exit 1
  fi
done
for contract in \
  '/privacy' \
  '/terms' \
  '/supporter-policy' \
  '/presentation-policy' \
  'edge-check=plaintext' \
  'route_markers'; do
  if ! rg -q -- "$contract" "${project_root}/scripts/verify-public-edge.sh"; then
    echo "public edge gate lost admitted-route contract: $contract" >&2
    exit 1
  fi
done
if ((private_foundation_number >= world_guard_number || world_guard_number >= canonical_start_number)); then
  echo "deployment must validate the privately activated world before canonical services start" >&2
  exit 1
fi
for contract in \
  'validate-production-world-state\.py' \
  'mode require-running' \
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
if rg -q 'build .*(migrate|projector|runner)' "$deployment"; then
  echo "production deployment redundantly builds aliases of the shared Rust image" >&2
  exit 1
fi
for contract in '--genesis-directory' '--evidence-directory' '--runtime-root' \
  'ATINY_RUNTIME_ARTIFACT_ROOT' 'ATINY_QUALITY_ADMISSION_FILE'; do
  if ! rg -q -- "$contract" "$deployment"; then
    echo "production deployment lost required public-genesis input: $contract" >&2
    exit 1
  fi
done

database_preflight_line="$(rg -n -m1 'public-genesis-preflight\.sh' "$database_preparation")"
database_checkout_line="$(rg -n -m1 'verify-production-checkout\.sh' "$database_preparation")"
database_mutation_line="$(rg -n -m1 'compose_args\[@.*up -d db migrate' "$database_preparation")"
if ((${database_preflight_line%%:*} >= ${database_mutation_line%%:*})); then
  echo "private database preparation must run the composed preflight before mutation" >&2
  exit 1
fi
if ((${database_checkout_line%%:*} >= ${database_mutation_line%%:*})); then
  echo "private database preparation must reject checkout drift before mutation" >&2
  exit 1
fi
for contract in \
  'requires the literal --confirm-private-database-preparation argument' \
  'up -d db migrate' \
  'migration-ready and empty for qualified activation' \
  'already contains the exact running qualified world' \
  'ATINY_QUALITY_ADMISSION_FILE' \
  'ATINY_RUNTIME_ARTIFACT_ROOT' \
  '--runtime-root' \
  'validate-production-world-state\.py' \
  'mode allow-empty'; do
  if ! rg -q -- "$contract" "$database_preparation"; then
    echo "private database preparation lost required contract: $contract" >&2
    exit 1
  fi
done
if rg -q 'up .*\b(api|projector|runner|web|memory-worker|cognition-worker|cloudflared)\b' \
  "$database_preparation"; then
  echo "private database preparation may start only db and migrate" >&2
  exit 1
fi

activation_preflight_line="$(rg -n -m1 'public-genesis-preflight\.sh' "$production_activation")"
activation_checkout_line="$(rg -n -m1 'verify-production-checkout\.sh' "$production_activation")"
activation_mutation_line="$(rg -n -m1 'activate-qualified-canonical-world\.sh.*activate' "$production_activation")"
if ((${activation_preflight_line%%:*} >= ${activation_mutation_line%%:*})); then
  echo "production activation must run the composed preflight before tick-zero mutation" >&2
  exit 1
fi
if ((${activation_checkout_line%%:*} >= ${activation_mutation_line%%:*})); then
  echo "production activation must reject checkout drift before tick-zero mutation" >&2
  exit 1
fi
for contract in \
  'requires the literal --confirm-experimental-genesis argument' \
  'production-preflight\.sh.*--env-file' \
  '127\.0\.0\.1' \
  'ATINY_QUALITY_ADMISSION_FILE' \
  'ATINY_RUNTIME_ARTIFACT_ROOT' \
  '--runtime-root' \
  'no service or public route was started'; do
  if ! rg -q -- "$contract" "$production_activation"; then
    echo "production activation lost required contract: $contract" >&2
    exit 1
  fi
done
if rg -q '\b(docker|cloudflared)\b.*\b(up|run|start|deploy|route)\b|deploy-production-app\.sh' \
  "$production_activation"; then
  echo "production activation may not deploy or start services" >&2
  exit 1
fi

for contract in \
  'git rev-parse --verify HEAD' \
  'git diff --quiet --ignore-submodules=none HEAD' \
  'git ls-files --others --exclude-standard'; do
  if ! rg -q -- "$contract" "$checkout_verifier"; then
    echo "production checkout verifier lost required contract: $contract" >&2
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

if ! rg -q 'ATINY_RUNTIME_ARTIFACT_ROOT.*runtime-artifacts.*:/runtime:ro' \
  "${project_root}/compose.yaml"; then
  echo "runner runtime mount is not controlled by the validated production runtime root" >&2
  exit 1
fi

for helper_and_confirmation in \
  'prepare-production-genesis-database.sh --confirm-private-database-preparation' \
  'activate-production-genesis.sh --confirm-experimental-genesis' \
  'deploy-production-app.sh --confirm-public-deployment'; do
  read -r helper confirmation <<<"$helper_and_confirmation"
  if "${project_root}/scripts/${helper}" \
    --genesis-directory /not-used/genesis \
    --evidence-directory /not-used/evidence \
    --runtime-root relative-runtime-root \
    "$confirmation" >"${TMPDIR:-/tmp}/atiny-runtime-root-rejection.txt" 2>&1; then
    echo "${helper} accepted a relative production runtime root" >&2
    exit 1
  fi
  if ! rg -q 'requires an absolute, existing, non-symlink runtime root' \
    "${TMPDIR:-/tmp}/atiny-runtime-root-rejection.txt"; then
    echo "${helper} rejected an unsafe runtime root for the wrong reason" >&2
    exit 1
  fi
done
rm -f "${TMPDIR:-/tmp}/atiny-runtime-root-rejection.txt"

echo "Production deployment revalidates immutable inputs before mutation and the public edge after smoke checks."
