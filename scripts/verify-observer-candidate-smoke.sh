#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
cleanup() {
  rm -rf -- "$temporary_directory"
}
trap cleanup EXIT

mkdir -p "${temporary_directory}/bin" "${temporary_directory}/fixtures"

cat > "${temporary_directory}/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=""
while (($#)); do
  case "$1" in
    --output)
      output="${2:-}"
      shift 2
      ;;
    --max-time)
      shift 2
      ;;
    --fail|--silent|--show-error)
      shift
      ;;
    *)
      shift
      ;;
  esac
done

[[ -n "$output" ]]
name="$(basename "$output" .json)"
cp -- "${FAKE_SMOKE_FIXTURES}/${name}.json" "$output"

case "${FAKE_SMOKE_FAILURE:-}:$name" in
  lag:telemetry)
    sed -i 's/"timeline_lag_batches":0/"timeline_lag_batches":1/' "$output"
    ;;
  private:timeline)
    sed -i 's/"summary":"A quiet event"/"summary":"A quiet event","action_values":{}/' "$output"
    ;;
  explicit:wiki)
    sed -i 's/"summary":"Observed evidence"/"summary":"Observed violence"/' "$output"
    ;;
  payload:commitments)
    sed -i 's/"post_state_hash":"state"/"post_state_hash":"state","events":[]/' "$output"
    ;;
esac
EOF
chmod 755 "${temporary_directory}/bin/curl"

fixtures="${temporary_directory}/fixtures"
world_id="b3ea736d-7a5a-5161-a74b-fa8c4302d333"
printf '%s\n' \
  '{"worlds":[{"world_id":"b3ea736d-7a5a-5161-a74b-fa8c4302d333","status":"running","input_status":"provisional-not-scientifically-admitted","through_sequence":"42"}]}' \
  > "${fixtures}/worlds.json"
printf '%s\n' \
  '{"world_id":"b3ea736d-7a5a-5161-a74b-fa8c4302d333","through_sequence":"42","timeline_through_sequence":"42","timeline_lag_batches":0,"organism_index_through_sequence":"42","organism_index_lag_batches":0,"findings_through_sequence":"42","findings_lag_batches":0,"telemetry_through_sequence":"42","telemetry_lag_batches":0,"artifacts_through_sequence":"42","artifacts_lag_batches":0,"living_people":2,"living_fauna":3}' \
  > "${fixtures}/telemetry.json"
printf '%s\n' '{"items":[{"title":"First trace","summary":"A quiet event"}]}' \
  > "${fixtures}/timeline.json"
printf '%s\n' '{"findings":[{"title":"A first","summary":"Observed evidence"}]}' \
  > "${fixtures}/findings.json"
printf '%s\n' '{"organisms":[{"title":"An inhabitant","summary":"Observed evidence"}]}' \
  > "${fixtures}/organisms.json"
printf '%s\n' '{"artifacts":[{"title":"A trace","summary":"Observed evidence"}]}' \
  > "${fixtures}/artifacts.json"
printf '%s\n' '{"entries":[{"title":"A finding","summary":"Observed evidence"}]}' \
  > "${fixtures}/wiki.json"
printf '%s\n' \
  '{"world_id":"b3ea736d-7a5a-5161-a74b-fa8c4302d333","commitments":[{"sequence":"1","batch_hash":"batch","post_state_hash":"state","previous_event_hash":"previous"}]}' \
  > "${fixtures}/commitments.json"

run_smoke() {
  PATH="${temporary_directory}/bin:${PATH}" FAKE_SMOKE_FIXTURES="$fixtures" \
    FAKE_SMOKE_FAILURE="${1:-}" "${project_root}/scripts/observer-candidate-smoke.sh" \
      http://observer.invalid "$world_id" 42
}

run_smoke "" >/dev/null

for failure in lag private explicit payload; do
  if run_smoke "$failure" >"${temporary_directory}/${failure}.out" 2>&1; then
    echo "observer candidate smoke accepted the ${failure} fixture" >&2
    exit 1
  fi
done

if PATH="${temporary_directory}/bin:${PATH}" FAKE_SMOKE_FIXTURES="$fixtures" \
  "${project_root}/scripts/observer-candidate-smoke.sh" \
    http://operator:secret@observer.invalid "$world_id" 42 \
    >"${temporary_directory}/credentialed.out" 2>&1; then
  echo "observer candidate smoke accepted credentials in BASE_URL" >&2
  exit 1
fi

echo "Observer candidate smoke gate fails closed on lag and public-data leaks."
