#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
installer="${project_root}/scripts/install-production-backend-monitor.sh"
renderer="${project_root}/scripts/render-production-backend-monitor-override.sh"
alert_renderer="${project_root}/scripts/render-production-alert-override.sh"
deployment="${project_root}/scripts/deploy-production-app.sh"
notifier="${project_root}/scripts/send-operations-alert.py"

if [[ ! -x "$notifier" ]]; then
  echo "production operations alert notifier is absent or not executable" >&2
  exit 1
fi

required_installer_contracts=(
  'requires the literal --confirm-production-monitor-install argument'
  'verify-production-checkout\.sh'
  'production-preflight\.sh.*--env-file'
  'render-production-backend-monitor-override\.sh'
  'render-production-alert-override\.sh'
  'a-tiny-civilization-operations-alert@\.service'
  'install -m 0644.*service_name'
  'install -m 0644.*timer_name'
  'systemctl daemon-reload'
  'systemd-analyze verify'
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
alert_service_name="a-tiny-civilization-operations-alert@.service"
mkdir -p "${temporary_directory}/${service_name}.d"
mkdir -p "${temporary_directory}/${alert_service_name}.d"
cp "${project_root}/ops/systemd/${service_name}" "${temporary_directory}/${service_name}"
cp "${project_root}/ops/systemd/${timer_name}" "${temporary_directory}/${timer_name}"
cp "${project_root}/ops/systemd/${alert_service_name}" \
  "${temporary_directory}/${alert_service_name}"
touch "${temporary_directory}/production.env"
"$renderer" "$project_root" "${temporary_directory}/production.env" \
  "${temporary_directory}/${service_name}.d/10-host-paths.conf"
"$alert_renderer" "$project_root" "${temporary_directory}/production.env" \
  "${temporary_directory}/${alert_service_name}.d/10-host-paths.conf"

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
if ! grep -Fxq \
  'OnFailure=a-tiny-civilization-operations-alert@%n.service' \
  "${temporary_directory}/${service_name}"; then
  echo "backend monitor no longer routes failures to the operator alert unit" >&2
  exit 1
fi

alert_override="${temporary_directory}/${alert_service_name}.d/10-host-paths.conf"
for exact_line in \
  "WorkingDirectory=${project_root}" \
  'EnvironmentFile=' \
  "EnvironmentFile=-${temporary_directory}/production.env" \
  'ExecStart=' \
  "ExecStart=/usr/bin/env python3 ${project_root}/scripts/send-operations-alert.py --unit %i" \
  'ProtectHome=read-only'; do
  if ! grep -Fxq -- "$exact_line" "$alert_override"; then
    echo "rendered alert override lost exact line: $exact_line" >&2
    exit 1
  fi
done
SYSTEMD_UNIT_PATH="${temporary_directory}:/usr/local/lib/systemd/system:/usr/lib/systemd/system:/lib/systemd/system" \
  systemd-analyze verify "$service_name" "$timer_name" "$alert_service_name"

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

env -u ATINY_OPERATIONS_ALERT_WEBHOOK_URL \
  -u ATINY_OPERATIONS_ALERT_BEARER_TOKEN \
  "$notifier" --unit "$service_name" \
  >"${temporary_directory}/unconfigured-alert.out" \
  2>"${temporary_directory}/unconfigured-alert.err"
if ! grep -q 'remains failed in systemd' "${temporary_directory}/unconfigured-alert.err"; then
  echo "unconfigured operations alert no longer preserves a visible systemd failure" >&2
  exit 1
fi
if ATINY_OPERATIONS_ALERT_WEBHOOK_URL=http://example.test/failure \
  "$notifier" --unit "$service_name" \
  >"${temporary_directory}/plaintext-alert.out" \
  2>"${temporary_directory}/plaintext-alert.err"; then
  echo "operations alert notifier accepted a plaintext external destination" >&2
  exit 1
fi
if ! grep -q 'must use HTTPS' "${temporary_directory}/plaintext-alert.err"; then
  echo "operations alert notifier rejected plaintext for the wrong reason" >&2
  exit 1
fi

echo "Production deployment installs an active host-path-correct backend monitor with alert delivery."
