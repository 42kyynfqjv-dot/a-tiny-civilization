#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
environment_file="${temporary_directory}/production.env"

touch "$environment_file"
chmod 600 "$environment_file"
{
  echo 'APP_ENV=production'
  echo 'POSTGRES_DB=civilization'
  echo 'POSTGRES_USER=civilization'
  echo 'POSTGRES_PASSWORD=not-the-development-password'
  echo 'LOCAL_COGNITION_BASE_URL=http://local-cognition:11434/v1'
  echo 'COGNITION_PAID_ENABLED=false'
} > "$environment_file"

"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" >/dev/null

deadline_file="${temporary_directory}/deadline.env"
cp "$environment_file" "$deadline_file"
{
  echo 'RUNNER_TICK_MILLISECONDS=1000'
  echo 'COGNITION_REQUEST_TIMEOUT_SECONDS=60'
} >> "$deadline_file"
if "${project_root}/scripts/production-preflight.sh" --env-file "$deadline_file" \
  >"${temporary_directory}/deadline.txt" 2>&1; then
  echo "production preflight accepted a wall timeout at the cognition deadline" >&2
  exit 1
fi
if ! grep -q 'must keep the bounded ladder before the 60-tick cognition deadline' \
  "${temporary_directory}/deadline.txt"; then
  echo "production preflight rejected the cognition deadline for the wrong reason" >&2
  exit 1
fi

short_lease_file="${temporary_directory}/short-lease.env"
cp "$environment_file" "$short_lease_file"
echo 'COGNITION_CLAIM_LEASE_SECONDS=240' >> "$short_lease_file"
if "${project_root}/scripts/production-preflight.sh" --env-file "$short_lease_file" \
  >"${temporary_directory}/short-lease.txt" 2>&1; then
  echo "production preflight accepted a lease without cognition renewal slack" >&2
  exit 1
fi
if ! grep -q 'must outlive one bounded operation plus renewal slack' \
  "${temporary_directory}/short-lease.txt"; then
  echo "production preflight rejected the short cognition lease for the wrong reason" >&2
  exit 1
fi

short_research_lease_file="${temporary_directory}/short-research-lease.env"
cp "$environment_file" "$short_research_lease_file"
echo 'CANCER_RESEARCH_CLAIM_LEASE_SECONDS=570' >> "$short_research_lease_file"
if "${project_root}/scripts/production-preflight.sh" --env-file "$short_research_lease_file" \
  >"${temporary_directory}/short-research-lease.txt" 2>&1; then
  echo "production preflight accepted a lease shorter than one bounded research job" >&2
  exit 1
fi
if ! grep -q 'must outlive every bounded route attempt' \
  "${temporary_directory}/short-research-lease.txt"; then
  echo "production preflight rejected the short research lease for the wrong reason" >&2
  exit 1
fi

{
  echo 'OPENROUTER_API_KEY=test-only-provider-key'
} >> "$environment_file"

if "${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" \
  >"${temporary_directory}/missing-export-approval.txt" 2>&1; then
  echo "production preflight accepted a provider key without external-export approval" >&2
  exit 1
fi
if ! grep -q 'remote cognition providers require COGNITION_EXTERNAL_EXPORT_APPROVED=true' \
  "${temporary_directory}/missing-export-approval.txt"; then
  echo "production preflight rejected missing export approval for the wrong reason" >&2
  exit 1
fi
echo 'COGNITION_EXTERNAL_EXPORT_APPROVED=true' >> "$environment_file"
"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" >/dev/null

plaintext_alert_file="${temporary_directory}/plaintext-alert.env"
cp "$environment_file" "$plaintext_alert_file"
echo 'ATINY_OPERATIONS_ALERT_WEBHOOK_URL=http://alerts.example.test/failure' \
  >> "$plaintext_alert_file"
if "${project_root}/scripts/production-preflight.sh" --env-file "$plaintext_alert_file" \
  >"${temporary_directory}/plaintext-alert.txt" 2>&1; then
  echo "production preflight accepted a plaintext operations alert destination" >&2
  exit 1
fi
if ! grep -q 'operations alert webhook URL must use HTTPS' \
  "${temporary_directory}/plaintext-alert.txt"; then
  echo "production preflight rejected a plaintext alert for the wrong reason" >&2
  exit 1
fi

unpaired_alert_file="${temporary_directory}/unpaired-alert.env"
cp "$environment_file" "$unpaired_alert_file"
echo 'ATINY_OPERATIONS_ALERT_BEARER_TOKEN=test-only-alert-token' >> "$unpaired_alert_file"
if "${project_root}/scripts/production-preflight.sh" --env-file "$unpaired_alert_file" \
  >"${temporary_directory}/unpaired-alert.txt" 2>&1; then
  echo "production preflight accepted an alert token without a destination" >&2
  exit 1
fi
if ! grep -q 'operations alert bearer token requires a webhook URL' \
  "${temporary_directory}/unpaired-alert.txt"; then
  echo "production preflight rejected an unpaired alert token for the wrong reason" >&2
  exit 1
fi

configured_alert_file="${temporary_directory}/configured-alert.env"
cp "$environment_file" "$configured_alert_file"
{
  echo 'ATINY_OPERATIONS_ALERT_WEBHOOK_URL=https://alerts.example.test/failure'
  echo 'ATINY_OPERATIONS_ALERT_BEARER_TOKEN=test-only-alert-token'
} >> "$configured_alert_file"
"${project_root}/scripts/production-preflight.sh" --env-file "$configured_alert_file" >/dev/null

canonical_environment='/etc/a-tiny-civilization-production.env'
for unit in \
  ops/systemd/a-tiny-civilization-backend-status.service \
  ops/systemd/a-tiny-civilization-backup.service \
  ops/systemd/a-tiny-civilization-backup-status.service \
  ops/systemd/a-tiny-civilization-moderation-status.service; do
  if ! grep -Fxq "EnvironmentFile=${canonical_environment}" "$unit"; then
    echo "$unit does not use the one canonical production environment file" >&2
    exit 1
  fi
done
if ! grep -Fxq "EnvironmentFile=-${canonical_environment}" \
  ops/systemd/a-tiny-civilization-operations-alert@.service; then
  echo "operations alert unit does not use the canonical optional production environment file" >&2
  exit 1
fi
for monitored_unit in \
  ops/systemd/a-tiny-civilization-backend-status.service \
  ops/systemd/a-tiny-civilization-backup.service \
  ops/systemd/a-tiny-civilization-backup-status.service \
  ops/systemd/a-tiny-civilization-moderation-status.service; do
  if ! grep -Fxq 'OnFailure=a-tiny-civilization-operations-alert@%n.service' \
    "$monitored_unit"; then
    echo "$monitored_unit does not route failures to the operator alert unit" >&2
    exit 1
  fi
done
if rg -q '/etc/a-tiny-civilization/production\.env' ops docs scripts; then
  echo "legacy split production environment path remains in the repository" >&2
  exit 1
fi

chmod 640 "$environment_file"
if "${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" \
  >"${temporary_directory}/unsafe.txt" 2>&1; then
  echo "production preflight accepted a group-readable secret file" >&2
  exit 1
fi
if ! grep -q 'must not be accessible by group or other users' "${temporary_directory}/unsafe.txt"; then
  echo "production preflight rejected unsafe permissions for the wrong reason" >&2
  exit 1
fi

echo "Production env-file parsing and permission checks are enforced."
