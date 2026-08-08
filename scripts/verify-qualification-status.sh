#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
grep -qF "result.result_payload -> 'receipt' <> 'null'::JSONB" \
  "${project_root}/scripts/qualification-status.sh"
grep -qF "ruleset_version < 26 OR non_person_requests = 0" \
  "${project_root}/scripts/qualification-status.sh"
grep -qF "distinct_metabolic_powers > 1" \
  "${project_root}/scripts/qualification-status.sh"
grep -qF "invalid_energy_reserves = 0" \
  "${project_root}/scripts/qualification-status.sh"
grep -qF "distinct_oral_portions > 1" \
  "${project_root}/scripts/qualification-status.sh"
grep -qF "oral_transfers > 0" \
  "${project_root}/scripts/qualification-status.sh"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
mkdir -p "${temporary_directory}/bin"

cat > "${temporary_directory}/bin/civilization-runner" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ " $* " == *" verify-world "* ]]
[[ " $* " != *" postgres://"* ]]
EOF
cat > "${temporary_directory}/bin/psql" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
[[ " $* " != *" postgres://"* ]]
[[ -z "${DATABASE_URL:-}" ]]
[[ "${PGHOST:-}" == "qualification.invalid" ]]
[[ "${PGPORT:-}" == "5432" ]]
[[ "${PGUSER:-}" == "operator" ]]
[[ "${PGPASSWORD:-}" == "not-printed" ]]
[[ "${PGDATABASE:-}" == "qualification" ]]
printf '%s\n' "${FAKE_QUALIFICATION_REPORT:-{\"passed\": true, \"schema_version\": 1}}"
EOF
chmod 755 "${temporary_directory}/bin/civilization-runner" "${temporary_directory}/bin/psql"

world_id="00000000-0000-4000-8000-000000000018"
common_environment=(
  "PATH=${temporary_directory}/bin:${PATH}"
  "DATABASE_URL=postgres://operator:not-printed@qualification.invalid/qualification"
  "ATINY_CIVILIZATION_RUNNER_EXECUTABLE=${temporary_directory}/bin/civilization-runner"
)

env "${common_environment[@]}" \
  "${project_root}/scripts/qualification-status.sh" "$world_id" \
  >"${temporary_directory}/passed.json"
grep -q '"passed": true' "${temporary_directory}/passed.json"

if env "${common_environment[@]}" FAKE_QUALIFICATION_REPORT='{"passed": false}' \
  "${project_root}/scripts/qualification-status.sh" "$world_id" \
  >"${temporary_directory}/failed.json" 2>"${temporary_directory}/failed.err"; then
  echo "qualification status accepted a failing report" >&2
  exit 1
fi
grep -q '"passed": false' "${temporary_directory}/failed.json"

if env "${common_environment[@]}" \
  "${project_root}/scripts/qualification-status.sh" not-a-world \
  >"${temporary_directory}/invalid.out" 2>"${temporary_directory}/invalid.err"; then
  echo "qualification status accepted an invalid world ID" >&2
  exit 1
fi
grep -q 'WORLD_ID must be a UUID' "${temporary_directory}/invalid.err"

echo "Machine-readable qualification status fails closed."
