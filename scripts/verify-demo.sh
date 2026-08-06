#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

generated_bundle="$(mktemp)"
trap 'rm -f "$generated_bundle"' EXIT

cargo run --quiet --locked -p civilization-verify -- demo --output "$generated_bundle"
cmp --silent verification/demo-bundle.json "$generated_bundle"
cargo run --quiet --locked -p civilization-verify -- verify verification/demo-bundle.json

echo "Committed verification bundle is deterministic and valid."
