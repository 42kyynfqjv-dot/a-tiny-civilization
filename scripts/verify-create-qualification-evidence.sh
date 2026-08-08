#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
genesis_directory="${temporary_directory}/genesis"
mkdir "$genesis_directory"
printf '%s\n' '{"seed":"424242"}' >"${genesis_directory}/origin-selection.json"
(cd "$genesis_directory" && sha256sum origin-selection.json > SHA256SUMS)

cat >"${temporary_directory}/qualification-status" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' '{"passed":true,"replay_verified":true,"schema_version":1}'
EOF
chmod 755 "${temporary_directory}/qualification-status"

world_id="00000000-0000-4000-8000-000000000018"
source_commit="0123456789abcdef0123456789abcdef01234567"
output_directory="${temporary_directory}/evidence"
env DATABASE_URL=postgres://unused \
  ATINY_SOURCE_COMMIT="$source_commit" \
  ATINY_QUALIFICATION_STATUS_EXECUTABLE="${temporary_directory}/qualification-status" \
  "${project_root}/scripts/create-qualification-evidence.sh" \
  "$world_id" "$genesis_directory" "$output_directory" >/dev/null

(cd "$output_directory" && sha256sum --check --strict SHA256SUMS >/dev/null)
grep -q '"contains_canonical_event_payloads":false' "${output_directory}/evidence.json"
grep -q "\"source_commit\":\"${source_commit}\"" "${output_directory}/evidence.json"
cmp "${genesis_directory}/origin-selection.json" \
  "${output_directory}/genesis/origin-selection.json"

if env DATABASE_URL=postgres://unused \
  ATINY_SOURCE_COMMIT="$source_commit" \
  ATINY_QUALIFICATION_STATUS_EXECUTABLE="${temporary_directory}/qualification-status" \
  "${project_root}/scripts/create-qualification-evidence.sh" \
  "$world_id" "$genesis_directory" "$output_directory" \
  >"${temporary_directory}/replace.out" 2>"${temporary_directory}/replace.err"; then
  echo "qualification evidence replaced an existing bundle" >&2
  exit 1
fi
grep -q 'refusing to replace qualification evidence' "${temporary_directory}/replace.err"

echo "Qualification evidence bundles are immutable and self-verifying."
