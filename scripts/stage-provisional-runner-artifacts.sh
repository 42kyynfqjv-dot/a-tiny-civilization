#!/usr/bin/env bash
set -euo pipefail

# Create a new, service-readable copy of exactly the artifacts pinned by the
# current provisional composition. The original data tree is never chmodded or
# modified. Run as root on the deployment host.

if (( EUID != 0 )); then
  echo "run this staging tool as root so staged artifacts can be owned by GID 10001" >&2
  exit 2
fi

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
destination="${1:-${project_root}/runtime-artifacts}"
if [[ -e "${destination}" ]]; then
  echo "staging destination already exists: ${destination}" >&2
  echo "choose a fresh destination; this tool never replaces staged artifacts" >&2
  exit 2
fi

entries=(
  "data/provisional/full-earth-breadth-first-0.1.0.json|4187ceb79a1e19e9479a61a97a454399446c0808300d23c168f84bed5feea6b4|8946"
  "data/derived-cache/etopo-2022-v1-l6-l10-q4-atomic/layers/bedrock-relief/root.index|0794832d533a81e0889779a78aa39d730a3b09a98edff37b57ef76f394504876|7741506"
  "data/derived-cache/chelsa-bioclim-plus-v2.1-tas-annual-l6-l10-v1/layers/near-surface-air-temperature-normal/root.index|7ff41b785e85f6314689bd31fcddd6546608dc9bd23ed60456a9b4826671cd9e|8257711"
  "data/derived-cache/natural-earth-10m-land-v5.1.2-l6-l10-reference/layers/land-reference/root.index|d8ac669b89f2903987766a2f55763b415bd7234097307ff63fcb7771099580ac|7716930"
  "data/derived-cache/copernicus-land-cover-2022-l6-l10-q32/layers/observed-land-cover/root.index|ca93fa8f3c6d2876bdb4e45f4a4229ddad3e34167e9652cd1cb019f00cc186cc|7995328"
  "data/derived-cache/jrc-global-surface-water-v1-5-2024-occurrence-l6-l10-v1/layers/observed-water-occurrence-source-code/root.index|82d77b6cdfa56109fee93560e60790890b1e276a11d22c067cce01b16024e02f|8257625"
  "data/derived-cache/soilgrids-2-0-topsoil-overviews-l6-l10-v1/layers/soilgrids-topsoil/root.index|4bda39813eb6a6faaf3b286ec3aeea0ad260108e38b626320a4dda878d91db2e|7913541"
  "data/source-inspections/jpl-de441-inventory.json|a253715e23e547d07f2e7be066a3fa437974b54f1c8a78f876f144ff8be22742|1851"
  "data/source-cache/jpl-de441/de441_part-1.bsp|13757827f5db41b835a24bbd637488636ce79a8ca754062fed17844f7d5b618e|1651119104"
  "data/source-cache/jpl-de441/de441_part-2.bsp|3abb17dae2d78dd34880377544aacb54892104a0d4462b322cb9f4454d4887f6|1656830976"
  "data/derived-cache/gbif-animalia-2023-08-28-v1.bin|b0597d47bc616b8ed2c18e7ba625a460538e9bac4bbae920f3f016095b966fa0|256508217"
  "data/source-inspections/fauna-traits-v1-inventory.json|b03ce7a3bf08188ba756e256f353f11b6f5d651b652e132a829a60bb844e0499|5640"
)

install -d -m 0750 -o root -g 10001 "${destination}"
for entry in "${entries[@]}"; do
  IFS='|' read -r relative expected_hash expected_bytes <<<"${entry}"
  source_path="${project_root}/${relative}"
  target_path="${destination}/${relative}"
  if [[ ! -f "${source_path}" || -L "${source_path}" ]]; then
    echo "required source artifact is absent or unsafe: ${source_path}" >&2
    exit 1
  fi
  actual_bytes="$(stat -c '%s' "${source_path}")"
  actual_hash="$(sha256sum "${source_path}" | awk '{print $1}')"
  if [[ "${actual_bytes}" != "${expected_bytes}" || "${actual_hash}" != "${expected_hash}" ]]; then
    echo "source artifact failed pinned verification: ${relative}" >&2
    exit 1
  fi
  install -D -m 0640 -o root -g 10001 "${source_path}" "${target_path}"
done

echo "staged ${#entries[@]} provisional artifacts at ${destination}"
