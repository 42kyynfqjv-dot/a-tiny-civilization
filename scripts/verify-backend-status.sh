#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
mkdir -p "${temporary_directory}/bin"

cat > "${temporary_directory}/bin/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "compose" && "${2:-}" == "version" ]]; then
  exit 0
fi
if [[ " $* " == *" exec -T db "* ]]; then
  echo "${FAKE_HEARTBEAT_COUNT:-4}"
fi
exit 0
EOF
chmod 755 "${temporary_directory}/bin/docker"

environment_file="${temporary_directory}/production.env"
touch "$environment_file"
chmod 600 "$environment_file"
{
  echo 'APP_ENV=production'
  echo 'POSTGRES_DB=civilization'
  echo 'POSTGRES_USER=civilization'
  echo 'POSTGRES_PASSWORD=not-the-development-password'
  echo 'OPENROUTER_API_KEY=test-only-provider-key'
  echo 'COGNITION_EXTERNAL_EXPORT_APPROVED=true'
  echo 'COGNITION_PAID_ENABLED=false'
} > "$environment_file"

PATH="${temporary_directory}/bin:${PATH}" \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" >/dev/null

if PATH="${temporary_directory}/bin:${PATH}" FAKE_HEARTBEAT_COUNT=3 \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" \
  >"${temporary_directory}/stale.txt" 2>&1; then
  echo "backend status accepted a missing service heartbeat" >&2
  exit 1
fi
if ! grep -q 'backend is not ready' "${temporary_directory}/stale.txt"; then
  echo "backend status rejected a missing heartbeat for the wrong reason" >&2
  exit 1
fi

echo "Complete backend status monitoring fails closed."
