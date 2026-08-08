#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

head_commit="$(git rev-parse --verify HEAD)"
if [[ ! "$head_commit" =~ ^[0-9a-f]{40}$ ]]; then
  echo "production checkout HEAD is not a full Git commit" >&2
  exit 1
fi
if ! git diff --quiet --ignore-submodules=none HEAD --; then
  echo "production checkout has staged or unstaged tracked changes" >&2
  exit 1
fi
untracked="$(git ls-files --others --exclude-standard)"
if [[ -n "$untracked" ]]; then
  echo "production checkout has untracked files" >&2
  exit 1
fi

echo "Production checkout is clean at ${head_commit}."
