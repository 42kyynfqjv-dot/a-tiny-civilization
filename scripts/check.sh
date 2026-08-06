#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

export DATABASE_URL="${TEST_DATABASE_URL:-${DATABASE_URL:-postgres://civilization:local-development-only@127.0.0.1:5432/civilization}}"

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
./scripts/check-boundaries.sh
./scripts/verify-demo.sh

cd web
npm run lint
npm test
