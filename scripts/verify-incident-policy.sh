#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
policy="${project_root}/docs/operations/INCIDENT_RESPONSE.md"
log="${project_root}/docs/operations/INCIDENTS.md"

for path in "$policy" "$log"; do
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "public incident contract is absent or unsafe: $path" >&2
    exit 1
  fi
done

required_policy=(
  'committed boundary'
  'within 24 hours'
  'SEV-1'
  'SEV-2'
  'SEV-3'
  'canonical-history'
  'personal/payment-data'
  'private cognition or memory'
)
for contract in "${required_policy[@]}"; do
  if ! grep -Fq "$contract" "$policy"; then
    echo "incident response policy lost required contract: $contract" >&2
    exit 1
  fi
done

required_entry=(
  'Incident ID:'
  'Severity:'
  'Status:'
  'Discovered (UTC):'
  'Recovered (UTC):'
  'Committed world cursor:'
  'Canonical-history impact:'
  'Personal/payment-data impact:'
  'Verification evidence:'
  'Follow-up owner:'
)
for field in "${required_entry[@]}"; do
  if ! grep -Fq -- "- $field" "$log"; then
    echo "public incident template lost required field: $field" >&2
    exit 1
  fi
done

echo "Public incident disclosure and response fields are complete."
