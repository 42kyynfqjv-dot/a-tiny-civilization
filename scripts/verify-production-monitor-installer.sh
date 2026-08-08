#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
installer="${project_root}/scripts/install-production-backend-monitor.sh"
deployment="${project_root}/scripts/deploy-production-app.sh"

required_installer_contracts=(
  'requires the literal --confirm-production-monitor-install argument'
  'verify-production-checkout\.sh'
  'production-preflight\.sh.*--env-file'
  'EnvironmentFile='
  'ExecStart='
  'ProtectHome=read-only'
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
