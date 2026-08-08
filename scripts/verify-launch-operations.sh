#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

# These checks are deliberately read-only and production-host portable: they require deployment
# prerequisites, not a host Rust toolchain. Cargo dependency-tree analysis remains in CI and the
# source-bound quality admission. Together these close the container, persistence, shutdown,
# observability, privacy, and deployment-order boundaries not covered merely by candidate data.
launch_checks=(
  scripts/verify-supporter-production-policy.sh
  scripts/verify-production-env-file.sh
  scripts/verify-container-image-pins.sh
  scripts/verify-container-log-policy.sh
  scripts/verify-postgres-durability.sh
  scripts/verify-graceful-shutdown-policy.sh
  scripts/verify-container-privilege-policy.sh
  scripts/verify-runtime-volume-policy.sh
  scripts/verify-production-runtime-gate.sh
  scripts/verify-production-port-preflight.sh
  scripts/verify-incident-policy.sh
  scripts/verify-live-genesis-gate.sh
  scripts/verify-backend-status.sh
  scripts/verify-production-monitor-installer.sh
  scripts/verify-hindsight-runtime.sh
)
for check in "${launch_checks[@]}"; do
  "${project_root}/${check}"
done

echo "Every read-only production operations gate passes."
