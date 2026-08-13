#!/usr/bin/env bash
set -euo pipefail

# Publish the ignored, patient-derived PDC000711 matrix and its provenance
# metadata behind one deterministic, content-addressed runtime boundary. Only
# the observer-side evidence worker is allowed to consume the emitted paths.

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly proteome_name='pdc000711-gbm-proteome.tsv'
readonly metadata_name='pdc000711-gbm-proteome.metadata.json'

proteome_source="${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/${proteome_name}"
metadata_source="${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/${metadata_name}"
runtime_root="${CANCER_PDC000711_EVIDENCE_RUNTIME_ROOT:-${project_root}/runtime-qualification/pdc000711}"
verify_only=0

while (($#)); do
  case "$1" in
    --proteome)
      proteome_source="${2:-}"
      shift 2
      ;;
    --metadata)
      metadata_source="${2:-}"
      shift 2
      ;;
    --runtime-root)
      runtime_root="${2:-}"
      shift 2
      ;;
    --verify-only)
      verify_only=1
      shift
      ;;
    *)
      echo "usage: $0 [--proteome /absolute/proteome.tsv] [--metadata /absolute/metadata.json] [--runtime-root /absolute/directory] [--verify-only]" >&2
      exit 2
      ;;
  esac
done

assert_absolute_without_symlinks() {
  local path="$1"
  local allow_missing="$2"

  python3 - "$path" "$allow_missing" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
allow_missing = sys.argv[2] == "1"
if not os.path.isabs(path):
    raise SystemExit(f"path must be absolute: {path}")
parts = path.split("/")[1:]
if any(part in ("", ".", "..") for part in parts):
    raise SystemExit(f"path must be normalized and contain no dot components: {path}")
current = "/"
missing = False
for part in parts:
    current = os.path.join(current, part)
    if missing:
        continue
    try:
        mode = os.lstat(current).st_mode
    except FileNotFoundError:
        if not allow_missing:
            raise SystemExit(f"path component does not exist: {current}")
        missing = True
        continue
    if stat.S_ISLNK(mode):
        raise SystemExit(f"path must not contain symlinks: {current}")
PY
}

for input in "$proteome_source" "$metadata_source"; do
  assert_absolute_without_symlinks "$input" 0
  if [[ ! -f "$input" || -L "$input" ]]; then
    echo "PDC000711 evidence input must be a regular, non-symlink file: $input" >&2
    exit 1
  fi
done
assert_absolute_without_symlinks "$runtime_root" 1
if [[ -e "$runtime_root" && ( ! -d "$runtime_root" || -L "$runtime_root" ) ]]; then
  echo "PDC000711 evidence runtime root must be a non-symlink directory: $runtime_root" >&2
  exit 1
fi

proteome_sha256="$(sha256sum -- "$proteome_source" | awk '{print $1}')"
metadata_sha256="$(sha256sum -- "$metadata_source" | awk '{print $1}')"
proteome_byte_length="$(stat -c '%s' -- "$proteome_source")"

# The metadata is the content-addressed semantic envelope for the derived bytes. Reject
# duplicate JSON keys and require its self-declared content address, byte count,
# source identity, and no-imputation contract to agree with the exact matrix.
python3 - "$metadata_source" "$proteome_sha256" "$proteome_byte_length" <<'PY'
import json
import re
import sys

metadata_path, actual_sha256, actual_length = sys.argv[1:]

def object_without_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result

try:
    with open(metadata_path, "r", encoding="utf-8") as handle:
        document = json.load(handle, object_pairs_hook=object_without_duplicates)
except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
    raise SystemExit(f"invalid PDC000711 derived metadata: {error}")

if not isinstance(document, dict):
    raise SystemExit("invalid PDC000711 derived metadata: top level is not an object")

failures = []
expected_name = "pdc000711-gbm-proteome.tsv"
if type(document.get("schema_version")) is not int or document.get("schema_version") != 1:
    failures.append("schema_version is not 1")
if document.get("artifact_file_name") != expected_name:
    failures.append("artifact_file_name is not the pinned derived filename")
if document.get("media_type") != "text/tab-separated-values; charset=utf-8":
    failures.append("media_type is not the pinned UTF-8 TSV type")
if document.get("artifact_sha256") != actual_sha256:
    failures.append("artifact_sha256 does not match the exact TSV bytes")
if document.get("artifact_content_address") != f"sha256:{actual_sha256}":
    failures.append("artifact_content_address does not match the exact TSV bytes")
if document.get("artifact_id") != f"pdc000711-hcmi-gbm-proteome:{actual_sha256}":
    failures.append("artifact_id does not include the exact TSV digest")
if (
    type(document.get("artifact_byte_length")) is not int
    or document.get("artifact_byte_length") != int(actual_length)
):
    failures.append("artifact_byte_length does not match the exact TSV bytes")

source = document.get("source")
if not isinstance(source, dict):
    failures.append("source provenance is absent")
else:
    expected_source = {
        "manifest_id": "pdc000711-hcmi-gbm-proteome-source-v1",
        "pdc_study_id": "PDC000711",
        "study_version_uuid": "ec0e442b-a0b8-4dc7-a4ba-6b5409fc68de",
        "file_id": "86e9b7f6-0776-4cb7-b761-dee14321b318",
    }
    for key, expected in expected_source.items():
        if source.get(key) != expected:
            failures.append(f"source.{key} does not match the pinned source")
    for key in (
        "manifest_sha256",
        "source_set_sha256",
        "source_file_sha256",
        "biospecimen_metadata_sha256",
    ):
        value = source.get(key)
        if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
            failures.append(f"source.{key} is not a lowercase SHA-256 digest")

transformation = document.get("transformation")
if not isinstance(transformation, dict):
    failures.append("transformation provenance is absent")
else:
    if transformation.get("annotation_columns_preserved") != [
        "T: Index",
        "T: NumberPSM",
        "T: ProteinID",
        "T: MaxPepProb",
    ]:
        failures.append("annotation column contract changed")
    if transformation.get("numeric_values_reparsed") is not False:
        failures.append("numeric values were reparsed")
    if transformation.get("imputation_applied") is not False:
        failures.append("imputation was applied")

dimensions = document.get("dimensions")
expected_dimensions = {
    "data_rows": 12342,
    "model_columns": 30,
    "annotation_columns": 4,
    "total_columns": 34,
}
if not isinstance(dimensions, dict):
    failures.append("dimensions are absent")
else:
    for key, expected in expected_dimensions.items():
        if dimensions.get(key) != expected:
            failures.append(f"dimensions.{key} is not {expected}")
    observed = dimensions.get("observed_model_cells")
    missing = dimensions.get("missing_model_cells")
    if (
        isinstance(observed, bool)
        or not isinstance(observed, int)
        or isinstance(missing, bool)
        or not isinstance(missing, int)
        or observed < 0
        or missing < 0
        or observed + missing != 12342 * 30
    ):
        failures.append("observed and missing model-cell counts are inconsistent")

joins = document.get("join_provenance")
if not isinstance(joins, list) or len(joins) != 30:
    failures.append("join_provenance does not contain the exact 30-model cohort")
else:
    headers = set()
    for expected_index, join in enumerate(joins):
        if not isinstance(join, dict):
            failures.append(f"join_provenance[{expected_index}] is not an object")
            continue
        header = join.get("matrix_header")
        if (
            join.get("derived_column_index") != expected_index
            or join.get("join_field") != "case_submitter_id"
            or header != join.get("case_submitter_id")
            or join.get("disease_type") != "Glioblastoma"
            or join.get("primary_site") != "Brain"
        ):
            failures.append(f"join_provenance[{expected_index}] violates the GBM join contract")
        if not isinstance(header, str) or not header or header in headers:
            failures.append(f"join_provenance[{expected_index}] has an invalid or duplicate header")
        else:
            headers.add(header)

if failures:
    raise SystemExit(
        "PDC000711 evidence metadata verification failed:\n  "
        + "\n  ".join(failures)
    )
PY

target_directory="${runtime_root}/sha256/${proteome_sha256}/${metadata_sha256}"
proteome_target="${target_directory}/${proteome_name}"
metadata_target="${target_directory}/${metadata_name}"

verify_staged_file() {
  local path="$1"
  local expected_sha256="$2"

  assert_absolute_without_symlinks "$path" 0
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "PDC000711 staged evidence is not a regular, non-symlink file: $path" >&2
    exit 1
  fi
  if [[ "$(sha256sum -- "$path" | awk '{print $1}')" != "$expected_sha256" ]]; then
    echo "PDC000711 staged evidence digest mismatch: $path" >&2
    exit 1
  fi
  if [[ "$(stat -c '%a' -- "$path")" != '444' ]]; then
    echo "PDC000711 staged evidence is not mode 0444: $path" >&2
    exit 1
  fi
}

if ((verify_only == 0)); then
  umask 022
  mkdir -p -- "$target_directory"
  assert_absolute_without_symlinks "$target_directory" 0
  chmod 0755 -- \
    "$runtime_root" \
    "${runtime_root}/sha256" \
    "${runtime_root}/sha256/${proteome_sha256}" \
    "$target_directory"

  stage_file() {
    local source="$1"
    local target="$2"
    local expected_sha256="$3"

    if [[ -e "$target" ]]; then
      verify_staged_file "$target" "$expected_sha256"
      return
    fi

    local temporary_path
    temporary_path="$(mktemp "${target_directory}/.evidence.XXXXXX")"
    cleanup_temporary() {
      rm -f -- "$temporary_path"
    }
    trap cleanup_temporary RETURN
    install -m 0444 -- "$source" "$temporary_path"
    if [[ "$(sha256sum -- "$temporary_path" | awk '{print $1}')" != "$expected_sha256" ]]; then
      echo "PDC000711 staged evidence changed during copy: $source" >&2
      exit 1
    fi
    mv --no-clobber -- "$temporary_path" "$target"
    # GNU mv --no-clobber succeeds without moving when another invocation wins
    # the publication race. Remove only this exact temporary file, then verify
    # the winner by content before accepting it.
    if [[ -e "$temporary_path" ]]; then
      rm -f -- "$temporary_path"
    fi
    temporary_path=''
    trap - RETURN
    verify_staged_file "$target" "$expected_sha256"
  }

  stage_file "$proteome_source" "$proteome_target" "$proteome_sha256"
  stage_file "$metadata_source" "$metadata_target" "$metadata_sha256"
else
  verify_staged_file "$proteome_target" "$proteome_sha256"
  verify_staged_file "$metadata_target" "$metadata_sha256"
fi

if [[ "$(stat -c '%s' -- "$proteome_target")" != "$proteome_byte_length" ]]; then
  echo "PDC000711 staged proteome byte length changed after publication" >&2
  exit 1
fi

# Stable two-line interface: matrix first, metadata second. Callers use mapfile
# instead of evaluating shell text.
printf '%s\n%s\n' "$proteome_target" "$metadata_target"
