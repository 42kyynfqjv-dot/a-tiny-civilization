#!/usr/bin/env bash
set -euo pipefail

# Derive the complete, canonical provisional genesis input chain for one public
# seed. This does not create or advance a world and never replaces an artifact.

if (( $# < 2 || $# > 3 )); then
  echo "usage: $0 WORLD_SEED OUTPUT_DIRECTORY [SPECIES_LIMIT]" >&2
  exit 2
fi

world_seed="$1"
output_directory="$(realpath -m "$2")"
species_limit="${3:-32}"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
data_executable="${ATINY_CIVILIZATION_DATA_EXECUTABLE:-${project_root}/target/release/civilization-data}"

if [[ ! "${world_seed}" =~ ^[0-9]+$ || ! "${species_limit}" =~ ^[1-9][0-9]*$ ]]; then
  echo "WORLD_SEED and SPECIES_LIMIT must be positive decimal integers" >&2
  exit 2
fi
if [[ ! -x "${data_executable}" ]]; then
  echo "missing executable civilization-data binary: ${data_executable}" >&2
  exit 2
fi
if [[ -e "${output_directory}" ]]; then
  echo "output directory already exists: ${output_directory}" >&2
  exit 2
fi

cd "${project_root}"
install -d -m 0750 "${output_directory}"
origin_selection="${output_directory}/origin-selection.json"
origin_environment="${output_directory}/origin-environment.json"
fauna_candidates="${output_directory}/fauna-candidates.json"
fauna_modeled_candidates="${output_directory}/fauna-modeled-range-candidates.json"
fauna_occurrences="${output_directory}/local-fauna-occurrence-evidence.json"
fauna_selection="${output_directory}/fauna-selection.json"
fauna_population="${output_directory}/fauna-population-plan.json"
fauna_metabolic_rates="${output_directory}/fauna-metabolic-rate-plan.json"
body_profiles="${output_directory}/organism-body-profile-plan.json"
material_resources="${output_directory}/material-resource-plan.json"

"${data_executable}" derive provisional-land-origin-selection \
  --land-reference-root-index data/derived-cache/natural-earth-10m-land-v5.1.2-l6-l10-reference/layers/land-reference/root.index \
  --artifact-root data/derived-cache/natural-earth-10m-land-v5.1.2-l6-l10-reference \
  --world-seed "${world_seed}" \
  --output "${origin_selection}"

"${data_executable}" derive provisional-origin-environment \
  --origin-selection "${origin_selection}" \
  --composition data/provisional/full-earth-breadth-first-0.1.1.json \
  --artifact-root . \
  --output "${origin_environment}"

selected_l10_patch="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["selected_patch"])' "${origin_selection}")"
coordinate_json="$("${data_executable}" inspect s2-geographic --s2-cell-id "${selected_l10_patch}")"
latitude_e7="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["latitude_e7"])' "${coordinate_json}")"
longitude_e7="$(python3 -c 'import json,sys; print(json.loads(sys.argv[1])["longitude_e7"])' "${coordinate_json}")"

occurrence_source="${ATINY_LOCAL_OCCURRENCE_SOURCE_DIRECTORY:-}"
require_occurrences="${ATINY_REQUIRE_LOCAL_OCCURRENCE_EVIDENCE:-0}"
if [[ "${require_occurrences}" != "0" && "${require_occurrences}" != "1" ]]; then
  echo "ATINY_REQUIRE_LOCAL_OCCURRENCE_EVIDENCE must be 0 or 1" >&2
  exit 2
fi
if [[ "${require_occurrences}" == "1" && -z "${occurrence_source}" ]]; then
  echo "canonical preparation requires ATINY_LOCAL_OCCURRENCE_SOURCE_DIRECTORY" >&2
  exit 2
fi
candidate_output="${fauna_candidates}"
if [[ -n "${occurrence_source}" ]]; then
  occurrence_source="$(realpath "${occurrence_source}")"
  candidate_output="${fauna_modeled_candidates}"
fi

python3 scripts/query-inaturalist-range-candidates.py \
  --artifact-root data/source-cache \
  --crosswalk data/derived-cache/inaturalist-gbif-animalia-range-crosswalk-v2-20.json \
  --latitude-e7 "${latitude_e7}" \
  --longitude-e7 "${longitude_e7}" \
  --all-crosswalked-species \
  --output "${candidate_output}"

if [[ -n "${occurrence_source}" ]]; then
  "${data_executable}" derive local-fauna-occurrence-evidence \
    --source-directory "${occurrence_source}" \
    --output "${fauna_occurrences}"
  "${data_executable}" derive corroborated-fauna-candidates \
    --candidates "${fauna_modeled_candidates}" \
    --occurrence-evidence "${fauna_occurrences}" \
    --output "${fauna_candidates}"
fi

"${data_executable}" derive fauna-seeded-selection \
  --candidates "${fauna_candidates}" \
  --world-seed "${world_seed}" \
  --species-limit "${species_limit}" \
  --individual-fauna-only \
  --output "${fauna_selection}"

"${data_executable}" derive provisional-fauna-population-plan \
  --candidates "${fauna_candidates}" \
  --selection "${fauna_selection}" \
  --origin-environment "${origin_environment}" \
  --output "${fauna_population}"

"${data_executable}" derive fauna-metabolic-rate-plan \
  --population-plan "${fauna_population}" \
  --candidates "${fauna_candidates}" \
  --selection "${fauna_selection}" \
  --origin-environment "${origin_environment}" \
  --metabolic-profiles data/derived-cache/animaltraits-metabolic-rate-v1.json \
  --output "${fauna_metabolic_rates}"

"${data_executable}" derive provisional-organism-body-profile-plan \
  --population-plan "${fauna_population}" \
  --candidates "${fauna_candidates}" \
  --selection "${fauna_selection}" \
  --origin-environment "${origin_environment}" \
  --metabolic-profiles data/derived-cache/animaltraits-metabolic-rate-v1.json \
  --metabolic-rate-plan "${fauna_metabolic_rates}" \
  --life-history-profiles data/derived-cache/amniote-life-history-v1.json \
  --tick-duration-seconds 300 \
  --output "${body_profiles}"

"${data_executable}" derive provisional-material-resource-plan \
  --population-plan "${fauna_population}" \
  --candidates "${fauna_candidates}" \
  --selection "${fauna_selection}" \
  --origin-environment "${origin_environment}" \
  --organism-body-profile-plan "${body_profiles}" \
  --output "${material_resources}"

sha256sum "${output_directory}"/*.json >"${output_directory}/SHA256SUMS"
chmod 0640 "${output_directory}"/*.json "${output_directory}/SHA256SUMS"
echo "prepared canonical provisional genesis inputs at ${output_directory}"
