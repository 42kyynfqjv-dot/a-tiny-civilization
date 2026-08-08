#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
installer="${project_root}/scripts/install-production-backend-monitor.sh"
renderer="${project_root}/scripts/render-production-backend-monitor-override.sh"
deployment="${project_root}/scripts/deploy-production-app.sh"

required_installer_contracts=(
  'requires the literal --confirm-production-monitor-install argument'
  'verify-production-checkout\.sh'
  'production-preflight\.sh.*--env-file'
  'render-production-backend-monitor-override\.sh'
  'install -m 0644.*service_name'
  'install -m 0644.*timer_name'
  'systemctl daemon-reload'
  'systemd-analyze verify.*service_name.*timer_name'
  'systemctl enable --now.*timer_name'
  'systemctl is-enabled --quiet.*timer_name'
  'systemctl is-active --quiet.*timer_name'
)
for contract in "${required_installer_contracts[@]}"; do
  if ! rg -q -- "$contract" "$installer"; then
    echo "production monitor installer lost required contract: $contract" >&2
    exit 1
  fi
done

temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
service_name="a-tiny-civilization-backend-status.service"
timer_name="a-tiny-civilization-backend-status.timer"
mkdir -p "${temporary_directory}/${service_name}.d"
cp "${project_root}/ops/systemd/${service_name}" "${temporary_directory}/${service_name}"
cp "${project_root}/ops/systemd/${timer_name}" "${temporary_directory}/${timer_name}"
touch "${temporary_directory}/production.env"
"$renderer" "$project_root" "${temporary_directory}/production.env" \
  "${temporary_directory}/${service_name}.d/10-host-paths.conf"

override="${temporary_directory}/${service_name}.d/10-host-paths.conf"
for exact_line in \
  "WorkingDirectory=${project_root}" \
  'EnvironmentFile=' \
  "EnvironmentFile=${temporary_directory}/production.env" \
  'ExecStart=' \
  "ExecStart=/usr/bin/env bash ${project_root}/scripts/backend-status.sh --env-file ${temporary_directory}/production.env" \
  'ProtectHome=read-only'; do
  if ! grep -Fxq -- "$exact_line" "$override"; then
    echo "rendered monitor override lost exact line: $exact_line" >&2
    exit 1
  fi
done
SYSTEMD_UNIT_PATH="${temporary_directory}:/usr/local/lib/systemd/system:/usr/lib/systemd/system:/lib/systemd/system" \
  systemd-analyze verify "$service_name" "$timer_name"

edge_line="$(rg -n -m1 'verify-public-edge\.sh.*https://atinycivilization\.com' "$deployment")"
install_line="$(rg -n -m1 'install-production-backend-monitor\.sh' "$deployment")"
if ((${install_line%%:*} <= ${edge_line%%:*})); then
  echo "deployment must install monitoring only after the public edge passes" >&2
  exit 1
fi
if ! rg -q -- '--confirm-production-monitor-install' "$deployment"; then
  echo "deployment lost the explicit monitor-install confirmation" >&2
  exit 1
fi

echo "Production deployment installs an active host-path-correct backend monitor."
