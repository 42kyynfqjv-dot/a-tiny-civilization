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
