#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 WORLD_ID GENESIS_DIRECTORY OUTPUT_DIRECTORY" >&2
  exit 2
fi
if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required" >&2
  exit 2
fi

world_id="$1"
genesis_directory="$(realpath "$2")"
output_directory="$3"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
qualification_executable="${ATINY_QUALIFICATION_STATUS_EXECUTABLE:-${project_root}/scripts/qualification-status.sh}"

if [[ ! "$world_id" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; then
  echo "WORLD_ID must be a UUID" >&2
  exit 2
fi
if [[ ! -d "$genesis_directory" || -L "$genesis_directory" ]]; then
  echo "GENESIS_DIRECTORY must be a regular directory" >&2
  exit 2
fi
if [[ ! -f "${genesis_directory}/SHA256SUMS" || -L "${genesis_directory}/SHA256SUMS" ]]; then
  echo "GENESIS_DIRECTORY must contain a regular SHA256SUMS file" >&2
  exit 2
fi
if [[ -e "$output_directory" || -L "$output_directory" ]]; then
  echo "refusing to replace qualification evidence: $output_directory" >&2
  exit 2
fi
if [[ ! -x "$qualification_executable" ]]; then
  echo "missing qualification status executable: $qualification_executable" >&2
  exit 2
fi

source_commit="${ATINY_SOURCE_COMMIT:-}"
if [[ -z "$source_commit" ]]; then
  if [[ -n "$(git -C "$project_root" status --porcelain)" ]]; then
    echo "qualification evidence requires a clean committed worktree" >&2
    exit 2
  fi
  source_commit="$(git -C "$project_root" rev-parse HEAD)"
fi
if [[ ! "$source_commit" =~ ^[0-9a-f]{40}$ ]]; then
  echo "ATINY_SOURCE_COMMIT must be a lowercase 40-character Git commit" >&2
  exit 2
fi

(cd "$genesis_directory" && sha256sum --check --strict SHA256SUMS)

parent_directory="$(dirname "$output_directory")"
mkdir -p "$parent_directory"
staging_directory="$(mktemp -d "${parent_directory}/.qualification-evidence.XXXXXXXX")"
cleanup() {
  if [[ -d "$staging_directory" ]]; then
    rm -rf -- "$staging_directory"
  fi
}
trap cleanup EXIT
mkdir "${staging_directory}/genesis"

while IFS= read -r source_file; do
  relative_name="${source_file#${genesis_directory}/}"
  install -m 0644 "$source_file" "${staging_directory}/genesis/${relative_name}"
done < <(find "$genesis_directory" -maxdepth 1 -type f \( -name '*.json' -o -name 'SHA256SUMS' \) -print | LC_ALL=C sort)

"$qualification_executable" "$world_id" >"${staging_directory}/qualification-status.json"
qualification_hash="$(sha256sum "${staging_directory}/qualification-status.json" | cut -d ' ' -f 1)"
genesis_manifest_hash="$(sha256sum "${staging_directory}/genesis/SHA256SUMS" | cut -d ' ' -f 1)"

python3 - "$world_id" "$source_commit" "$qualification_hash" "$genesis_manifest_hash" \
  >"${staging_directory}/evidence.json" <<'PY'
import json
import sys

world_id, source_commit, qualification_hash, genesis_manifest_hash = sys.argv[1:]
document = {
    "schema_version": 1,
    "world_id": world_id,
    "source_commit": source_commit,
    "qualification_status_sha256": qualification_hash,
    "genesis_sha256s_sha256": genesis_manifest_hash,
    "contains_canonical_event_payloads": False,
    "purpose": "pre-genesis mechanical qualification evidence",
}
print(json.dumps(document, sort_keys=True, separators=(",", ":")))
PY

(cd "$staging_directory" && find . -type f ! -path './SHA256SUMS' -print0 \
  | LC_ALL=C sort -z | xargs -0 sha256sum > SHA256SUMS)
chmod 0444 "${staging_directory}/SHA256SUMS" \
  "${staging_directory}/evidence.json" \
  "${staging_directory}/qualification-status.json" \
  "${staging_directory}/genesis/"*
mv -T --no-clobber "$staging_directory" "$output_directory"
if [[ -d "$staging_directory" ]]; then
  echo "qualification evidence destination appeared during publication" >&2
  exit 1
fi
trap - EXIT

echo "created qualification evidence: $output_directory"
