#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

required=(
  'Dockerfile|FROM rust:1.97.1-bookworm@sha256:14bc9c5966e7b3a385794b3d5389a8765668342025fbcc7b2e3d2866ac4bd8c3 AS builder'
  'Dockerfile|FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241 AS runtime'
  'web/Dockerfile|FROM node:24.19.0-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03'
  'Dockerfile.postgres-walg|FROM postgres:17-bookworm@sha256:9b18b78397054fce88a9552e9d5a3ad5bb7fd258c5b3cc1c5028e46373d6ea8f'
  'compose.yaml|image: postgres:17-alpine@sha256:742f40ea20b9ff2ff31db5458d127452988a2164df9e17441e191f3b72252193'
  'compose.hindsight.yaml|image: ollama/ollama@sha256:b88c73ace3e115f8ec53dc8761ae1c0aabfa675406e3681786b98757ce050f42'
  'compose.hindsight.yaml|image: ghcr.io/vectorize-io/hindsight:0.8.6@sha256:ffa391a77284e49f6b55e32c86f33529ac4257831407b14038a72b6a0a232039'
)
for entry in "${required[@]}"; do
  IFS='|' read -r path line <<<"$entry"
  if ! rg -qF "$line" "$path"; then
    echo "container image pin is absent or changed: $path: $line" >&2
    exit 1
  fi
done

for dockerfile in Dockerfile Dockerfile.postgres-walg web/Dockerfile; do
  if rg -n '^FROM [^@[:space:]]+( AS [A-Za-z0-9._-]+)?$' "$dockerfile"; then
    echo "Dockerfile base image is not digest-pinned: $dockerfile" >&2
    exit 1
  fi
done

echo "All production base and third-party service images are digest-pinned."
