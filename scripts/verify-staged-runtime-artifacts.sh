#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runtime_root="$(realpath -m "${1:-${project_root}/runtime-artifacts}")"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"
composition_relative="data/provisional/full-earth-breadth-first-0.1.2.json"

if (($# > 1)); then
  echo "usage: $0 [RUNTIME_ARTIFACT_ROOT]" >&2
  exit 2
fi
if [[ ! -d "$runtime_root" || -L "$runtime_root" ]]; then
  echo "runtime artifact root is absent or unsafe: $runtime_root" >&2
  exit 2
fi
if [[ ! -x "$data_executable" ]]; then
  echo "civilization-data executable is absent: $data_executable" >&2
  exit 2
fi
if [[ -n "$(find "$runtime_root" -type l -print -quit)" ]]; then
  echo "runtime artifact tree contains a symbolic link" >&2
  exit 1
fi
if [[ -n "$(find "$runtime_root" -perm /0022 -print -quit)" ]]; then
  echo "runtime artifact tree contains a group- or world-writable path" >&2
  exit 1
fi

"$data_executable" validate-provisional \
  --artifact-root "$runtime_root" "$runtime_root/$composition_relative" >/dev/null

celestial_artifacts=(
  "data/source-cache/jpl-de441/de441_part-1.bsp|13757827f5db41b835a24bbd637488636ce79a8ca754062fed17844f7d5b618e|1651119104"
  "data/source-cache/jpl-de441/de441_part-2.bsp|3abb17dae2d78dd34880377544aacb54892104a0d4462b322cb9f4454d4887f6|1656830976"
)
for artifact in "${celestial_artifacts[@]}"; do
  IFS='|' read -r relative expected_digest expected_bytes <<<"$artifact"
  path="$runtime_root/$relative"
  if [[ ! -f "$path" || -L "$path" || "$(stat -c '%s' "$path")" != "$expected_bytes" ]]; then
    echo "staged celestial artifact is absent, unsafe, or has the wrong length: $relative" >&2
    exit 1
  fi
  digest="$(sha256sum -- "$path")"
  digest="${digest%% *}"
  if [[ "$digest" != "$expected_digest" ]]; then
    echo "staged celestial artifact digest mismatch: $relative" >&2
    exit 1
  fi
done

echo "Staged full-Earth and DE441 runtime artifacts are complete, immutable, and verified."
