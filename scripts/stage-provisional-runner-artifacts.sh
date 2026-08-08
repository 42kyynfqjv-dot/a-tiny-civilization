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
destination="$(realpath -m "${1:-${project_root}/runtime-artifacts}")"
composition="data/provisional/full-earth-breadth-first-0.1.1.json"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"
if [[ -e "${destination}" ]]; then
  echo "staging destination already exists: ${destination}" >&2
  echo "choose a fresh destination; this tool never replaces staged artifacts" >&2
  exit 2
fi
if [[ ! -x "$data_executable" ]]; then
  echo "missing civilization-data executable: $data_executable" >&2
  exit 2
fi

cd "$project_root"
"$data_executable" validate-provisional --artifact-root . "$composition" >/dev/null

path_manifest="$(mktemp)"
staging_directory="${destination}.partial-${BASHPID}"
cleanup() {
  rm -f -- "$path_manifest"
  rm -rf -- "$staging_directory"
}
trap cleanup EXIT
if [[ -e "$staging_directory" ]]; then
  echo "temporary staging destination already exists: $staging_directory" >&2
  exit 2
fi
python3 scripts/list-provisional-runtime-artifacts.py "$composition" >"$path_manifest"

celestial_artifacts=( \
  "data/source-cache/jpl-de441/de441_part-1.bsp|13757827f5db41b835a24bbd637488636ce79a8ca754062fed17844f7d5b618e|1651119104" \
  "data/source-cache/jpl-de441/de441_part-2.bsp|3abb17dae2d78dd34880377544aacb54892104a0d4462b322cb9f4454d4887f6|1656830976"
)
verify_celestial_artifacts() {
  local artifact_root="$1"
  local extra path expected_hash expected_bytes
  for extra in "${celestial_artifacts[@]}"; do
  IFS='|' read -r path expected_hash expected_bytes <<<"$extra"
    [[ "$(stat -c '%s' "$artifact_root/$path")" == "$expected_bytes" ]]
    [[ "$(sha256sum "$artifact_root/$path" | awk '{print $1}')" == "$expected_hash" ]]
  done
}
verify_celestial_artifacts "$project_root"

install -d -m 0750 -o root -g 10001 "$staging_directory"
tar -C "$project_root" -cf - -T "$path_manifest" | tar -C "$staging_directory" -xf -
chown -R root:10001 "$staging_directory"
find "$staging_directory" -type d -exec chmod 0750 {} +
find "$staging_directory" -type f -exec chmod 0640 {} +

"$data_executable" validate-provisional \
  --artifact-root "$staging_directory" "$staging_directory/$composition" >/dev/null
verify_celestial_artifacts "$staging_directory"
mv -T -- "$staging_directory" "$destination"
artifact_count="$(wc -l < "$path_manifest")"
echo "staged and reverified $artifact_count provisional runtime artifacts at $destination"
