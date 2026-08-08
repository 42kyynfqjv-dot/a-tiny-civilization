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
python3 ./scripts/verify-commercial-occurrence-filter.py
python3 ./scripts/verify-inaturalist-range-map-tools.py
python3 -m py_compile ./scripts/query-inaturalist-range-candidates.py
python3 ./scripts/acquire-jpl-de441.py >/dev/null
python3 ./scripts/verify-jpl-de441-tools.py
./scripts/check-boundaries.sh
./scripts/verify-supporter-production-policy.sh
./scripts/verify-production-env-file.sh
./scripts/verify-backend-status.sh
./scripts/verify-hindsight-runtime.sh
./scripts/verify-qualification-status.sh
./scripts/verify-create-qualification-evidence.sh
./scripts/verify-provisional-genesis-pins.sh
python3 scripts/verify-runtime-artifact-listing.py
./scripts/verify-demo.sh

cd web
npm run lint
npm test
