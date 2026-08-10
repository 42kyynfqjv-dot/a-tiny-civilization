#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
environment_file="${ATINY_PRODUCTION_ENV_FILE:-/etc/a-tiny-civilization-production.env}"
confirmed=0

while (($#)); do
  case "$1" in
    --env-file)
      environment_file="${2:-}"
      shift 2
      ;;
    --confirm-production-monitor-install)
      confirmed=1
      shift
      ;;
    *)
      echo "usage: $0 [--env-file /absolute/path/to/production.env] --confirm-production-monitor-install" >&2
      exit 2
      ;;
  esac
done
if ((confirmed != 1)); then
  echo "backend monitor installation requires the literal --confirm-production-monitor-install argument" >&2
  exit 2
fi
if ((EUID != 0)); then
  echo "run backend monitor installation as root" >&2
  exit 2
fi
cd "$project_root"
"${project_root}/scripts/verify-production-checkout.sh"
"${project_root}/scripts/production-preflight.sh" --env-file "$environment_file" >/dev/null

unit_directory="/etc/systemd/system"
service_name="a-tiny-civilization-backend-status.service"
timer_name="a-tiny-civilization-backend-status.timer"
alert_service_name="a-tiny-civilization-operations-alert@.service"
disk_guard_service_name="a-tiny-civilization-disk-guard.service"
disk_guard_timer_name="a-tiny-civilization-disk-guard.timer"
drop_in_directory="${unit_directory}/${service_name}.d"
alert_drop_in_directory="${unit_directory}/${alert_service_name}.d"
disk_guard_drop_in_directory="${unit_directory}/${disk_guard_service_name}.d"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
override="${temporary_directory}/10-host-paths.conf"
alert_override="${temporary_directory}/10-alert-host-paths.conf"
disk_guard_override="${temporary_directory}/10-disk-guard-host-paths.conf"

"${project_root}/scripts/render-production-backend-monitor-override.sh" \
  "$project_root" "$environment_file" "$override"
"${project_root}/scripts/render-production-alert-override.sh" \
  "$project_root" "$environment_file" "$alert_override"
"${project_root}/scripts/render-production-disk-guard-override.sh" \
  "$project_root" "$environment_file" "$disk_guard_override"

install -d -m 0755 "$unit_directory" "$drop_in_directory" "$alert_drop_in_directory" \
  "$disk_guard_drop_in_directory"
install -m 0644 "${project_root}/ops/systemd/${service_name}" "${unit_directory}/${service_name}"
install -m 0644 "${project_root}/ops/systemd/${timer_name}" "${unit_directory}/${timer_name}"
install -m 0644 "${project_root}/ops/systemd/${alert_service_name}" \
  "${unit_directory}/${alert_service_name}"
install -m 0644 "${project_root}/ops/systemd/${disk_guard_service_name}" \
  "${unit_directory}/${disk_guard_service_name}"
install -m 0644 "${project_root}/ops/systemd/${disk_guard_timer_name}" \
  "${unit_directory}/${disk_guard_timer_name}"
install -m 0644 "$override" "${drop_in_directory}/10-host-paths.conf"
install -m 0644 "$alert_override" "${alert_drop_in_directory}/10-host-paths.conf"
install -m 0644 "$disk_guard_override" "${disk_guard_drop_in_directory}/10-host-paths.conf"

systemctl daemon-reload
systemd-analyze verify \
  "${unit_directory}/${service_name}" \
  "${unit_directory}/${timer_name}" \
  "${unit_directory}/${disk_guard_service_name}" \
  "${unit_directory}/${disk_guard_timer_name}" \
  "${unit_directory}/${alert_service_name}"
systemctl enable --now "$timer_name" "$disk_guard_timer_name"
systemctl is-enabled --quiet "$timer_name"
systemctl is-active --quiet "$timer_name"
systemctl is-enabled --quiet "$disk_guard_timer_name"
systemctl is-active --quiet "$disk_guard_timer_name"

echo "Backend health timer is installed for ${project_root} and active."
