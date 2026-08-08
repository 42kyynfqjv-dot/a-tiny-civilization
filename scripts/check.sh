#!/usr/bin/env bash
set -euo pipefail

export PYTHONDONTWRITEBYTECODE=1

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

export DATABASE_URL="${TEST_DATABASE_URL:-${DATABASE_URL:-postgres://civilization:local-development-only@127.0.0.1:5432/civilization}}"

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
python3 ./scripts/verify-s2-routing.py
python3 ./scripts/verify-geographic-s2-routing.py
python3 ./scripts/verify-era5-request.py
python3 ./scripts/verify-era5-provenance-tools.py
python3 ./scripts/acquire-cds-land-cover.py \
  --output-directory data/source-cache/copernicus-land-cover-2022 --dry-run >/dev/null
python3 ./scripts/verify-cds-land-cover-tools.py
python3 ./scripts/acquire-jrc-surface-water.py --layers occurrence >/dev/null
python3 ./scripts/acquire-soilgrids-topsoil.py >/dev/null
python3 ./scripts/acquire-gbif-taxonomy.py >/dev/null
python3 ./scripts/acquire-fauna-traits.py >/dev/null
python3 ./scripts/test_derive_eltontraits_ecology.py
python3 ./scripts/verify-commercial-occurrence-filter.py
python3 ./scripts/verify-inaturalist-range-map-tools.py
python3 -m py_compile ./scripts/query-inaturalist-range-candidates.py
python3 ./scripts/acquire-jpl-de441.py >/dev/null
python3 ./scripts/verify-jpl-de441-tools.py
./scripts/check-boundaries.sh
./scripts/verify-supporter-production-policy.sh
./scripts/verify-production-env-file.sh
./scripts/verify-container-image-pins.sh
./scripts/verify-container-log-policy.sh
./scripts/verify-postgres-durability.sh
./scripts/verify-graceful-shutdown-policy.sh
./scripts/verify-container-privilege-policy.sh
./scripts/verify-runtime-volume-policy.sh
./scripts/verify-ci-supply-chain.sh
./scripts/verify-production-runtime-gate.sh
./scripts/verify-production-port-preflight.sh
./scripts/verify-incident-policy.sh
./scripts/verify-public-genesis-preflight.sh
./scripts/verify-launch-operations.sh
./scripts/verify-live-genesis-gate.sh
./scripts/verify-backend-status.sh
./scripts/verify-production-monitor-installer.sh
python3 -m py_compile ./scripts/send-operations-alert.py
python3 ./scripts/test_send_operations_alert.py
./scripts/verify-hindsight-runtime.sh
./scripts/verify-qualification-status.sh
bash -n ./scripts/advance-qualification-world.sh ./scripts/advance-cognition-qualified-world.sh \
  ./scripts/observer-candidate-smoke.sh ./scripts/verify-observer-candidate-smoke.sh \
  ./scripts/deploy-production-app.sh ./scripts/activate-qualified-canonical-world.sh \
  ./scripts/prepare-production-genesis-database.sh ./scripts/activate-production-genesis.sh \
  ./scripts/verify-production-checkout.sh \
  ./scripts/install-production-backend-monitor.sh \
  ./scripts/render-production-backend-monitor-override.sh \
  ./scripts/render-production-alert-override.sh \
  ./scripts/verify-launch-operations.sh \
  ./scripts/verify-staged-runtime-artifacts.sh ./scripts/public-genesis-preflight.sh \
  ./scripts/verify-live-genesis.sh ./scripts/smoke-runtime-images.sh \
  ./scripts/verify-public-edge.sh
./scripts/verify-observer-candidate-smoke.sh
./scripts/verify-create-qualification-evidence.sh
./scripts/verify-provisional-genesis-pins.sh
python3 scripts/verify-runtime-artifact-listing.py
python3 -m py_compile scripts/verify-launch-candidate-evidence.py
python3 scripts/test_verify_launch_candidate_evidence.py
python3 -m py_compile scripts/verify-quality-world-admission.py
python3 -m py_compile scripts/verify-public-edge-headers.py
python3 scripts/test_verify_public_edge_headers.py
python3 -m py_compile scripts/verify-web-dependency-licenses.py
python3 scripts/test_verify_quality_world_admission.py
python3 scripts/test_audit_canonical_science.py
python3 scripts/test_verify_public_observatory_admission.py
python3 scripts/test_validate_production_world_state.py
./scripts/verify-demo.sh

cd web
npm run lint
npm test
