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
  echo "${FAKE_BACKEND_DATA_STATUS:-4|1|5|0|0|0}"
fi
if [[ " $* " == *" http://local-cognition:11434/api/tags "* ]]; then
  if [[ -n "${FAKE_LOCAL_MODEL_STATUS+x}" ]]; then
    echo "$FAKE_LOCAL_MODEL_STATUS"
  else
    echo '{"models":[{"name":"qwen2.5:1.5b","digest":"65ec06548149b04c096a120e4a6da9d4017ea809c91734ea5631e89f96ddc57b"}]}'
  fi
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
  echo 'LOCAL_COGNITION_BASE_URL=http://local-cognition:11434/v1'
  echo 'COGNITION_PAID_ENABLED=false'
} > "$environment_file"

PATH="${temporary_directory}/bin:${PATH}" \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" >/dev/null

if PATH="${temporary_directory}/bin:${PATH}" FAKE_BACKEND_DATA_STATUS='3|1|4|0|0|0' \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" \
  >"${temporary_directory}/stale.txt" 2>&1; then
  echo "backend status accepted a missing service heartbeat" >&2
  exit 1
fi

if PATH="${temporary_directory}/bin:${PATH}" FAKE_BACKEND_DATA_STATUS='4|1|5|101|0|0' \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" \
  >"${temporary_directory}/projection.txt" 2>&1; then
  echo "backend status accepted excessive projection lag" >&2
  exit 1
fi

if PATH="${temporary_directory}/bin:${PATH}" FAKE_BACKEND_DATA_STATUS='4|1|5|0|1|0' \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" \
  >"${temporary_directory}/memory.txt" 2>&1; then
  echo "backend status accepted stale memory delivery" >&2
  exit 1
fi

if PATH="${temporary_directory}/bin:${PATH}" FAKE_BACKEND_DATA_STATUS='4|1|5|0|0|1' \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" \
  >"${temporary_directory}/cognition.txt" 2>&1; then
  echo "backend status accepted a stuck cognition dispatch" >&2
  exit 1
fi
if ! grep -q 'backend is not ready' "${temporary_directory}/stale.txt"; then
  echo "backend status rejected a missing heartbeat for the wrong reason" >&2
  exit 1
fi

if PATH="${temporary_directory}/bin:${PATH}" FAKE_LOCAL_MODEL_STATUS='{"models":[]}' \
  "${project_root}/scripts/backend-status.sh" --env-file "$environment_file" \
  >"${temporary_directory}/model.txt" 2>&1; then
  echo "backend status accepted a missing or changed local model" >&2
  exit 1
fi

echo "Complete backend status monitoring fails closed."
