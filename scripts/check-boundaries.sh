#!/usr/bin/env bash
set -euo pipefail

check_tree_excludes() {
  local package="$1"
  local forbidden="$2"
  local tree

  tree="$(cargo tree --locked --edges normal --prefix none -p "$package")"
  if printf '%s\n' "$tree" | rg --line-number "$forbidden"; then
    echo "Architecture boundary violation in dependency tree for $package." >&2
    exit 1
  fi
}

check_tree_excludes \
  world-domain \
  '^(application|hindsight-adapter|observer-|supporter-|payment-|auth-|postgres-store|axum|sqlx|tokio|reqwest) '
check_tree_excludes \
  sim-engine \
  '^(application|hindsight-adapter|observer-|supporter-|payment-|auth-|postgres-store|axum|sqlx|tokio|reqwest) '
check_tree_excludes \
  world-data \
  '^(application|sim-engine|hindsight-adapter|observer-|supporter-|payment-|auth-|postgres-store|axum|sqlx|tokio|reqwest) '
check_tree_excludes \
  world-data-filesystem \
  '^(application|sim-engine|hindsight-adapter|observer-|supporter-|payment-|auth-|postgres-store|axum|sqlx|tokio|reqwest) '
check_tree_excludes \
  civilization-runner \
  '^(observer-api|supporter-|payment-|auth-) '

if rg --line-number 'observer-projection|supporter-|payment-|auth-' \
  apps/runner/Cargo.toml; then
  echo "Simulation runner must not directly import observer/supporter/payment/auth ports." >&2
  exit 1
fi

if rg --ignore-case --line-number \
  '\b(update[[:space:]]+event_batches|delete[[:space:]]+from[[:space:]]+event_batches|truncate([[:space:]]+table)?[[:space:]]+event_batches)\b' \
  db/migrations; then
  echo "Canonical event migrations must not rewrite or delete event batches." >&2
  exit 1
fi

./scripts/check-production-boundaries.sh

echo "Architecture boundary checks passed."
