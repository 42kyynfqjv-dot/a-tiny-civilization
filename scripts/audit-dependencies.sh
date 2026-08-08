#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mode="${1:-all}"
if [[ "$mode" != "all" && "$mode" != "rust" && "$mode" != "web" ]]; then
  echo "usage: $0 [all|rust|web]" >&2
  exit 2
fi

if [[ "$mode" == "all" || "$mode" == "rust" ]]; then
  if ! command -v cargo-audit >/dev/null; then
    echo "cargo-audit 0.22.2 is required" >&2
    exit 2
  fi
  if [[ "$(cargo-audit --version)" != "cargo-audit 0.22.2" ]]; then
    echo "dependency audit requires exactly cargo-audit 0.22.2" >&2
    exit 2
  fi
  rsa_tree="$(cd "$project_root" && cargo tree --locked --target all -i rsa@0.9.10 2>/dev/null)"
  if [[ -n "$rsa_tree" ]]; then
    echo "RUSTSEC-2023-0071 exception is invalid because rsa became reachable" >&2
    echo "$rsa_tree" >&2
    exit 1
  fi
  (cd "$project_root" && cargo audit --ignore RUSTSEC-2023-0071)
fi

if [[ "$mode" == "all" || "$mode" == "web" ]]; then
  (cd "$project_root/web" && npm audit --omit=dev --audit-level=low)
fi

echo "Locked dependency advisory audit passed for ${mode}."
