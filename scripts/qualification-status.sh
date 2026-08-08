#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 WORLD_ID" >&2
  exit 2
fi
if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required" >&2
  exit 2
fi

world_id="$1"
if [[ ! "$world_id" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; then
  echo "WORLD_ID must be a UUID" >&2
  exit 2
fi

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner_executable="${ATINY_CIVILIZATION_RUNNER_EXECUTABLE:-${project_root}/target/release/civilization-runner}"
minimum_tick="${ATINY_QUALIFICATION_MINIMUM_TICK:-1000}"
expected_ruleset="${ATINY_QUALIFICATION_RULESET_VERSION:-29}"

if [[ ! "$minimum_tick" =~ ^[1-9][0-9]*$ ]]; then
  echo "ATINY_QUALIFICATION_MINIMUM_TICK must be a positive integer" >&2
  exit 2
fi
if [[ ! "$expected_ruleset" =~ ^[1-9][0-9]*$ ]]; then
  echo "ATINY_QUALIFICATION_RULESET_VERSION must be a positive integer" >&2
  exit 2
fi
if [[ ! -x "$runner_executable" ]]; then
  echo "missing executable civilization-runner binary: $runner_executable" >&2
  exit 2
fi
if ! command -v psql >/dev/null 2>&1; then
  echo "psql is required" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to pass DATABASE_URL to libpq without exposing it in process arguments" >&2
  exit 2
fi

mapfile -d '' -t connection_fields < <(python3 <<'PY'
import os
import sys
from urllib.parse import parse_qs, unquote, urlsplit

url = urlsplit(os.environ["DATABASE_URL"])
if url.scheme not in {"postgres", "postgresql"}:
    sys.exit("DATABASE_URL must use postgres:// or postgresql://")
if not url.hostname or url.username is None or not url.path.startswith("/") or len(url.path) == 1:
    sys.exit("DATABASE_URL must include host, user, and database")
parameters = (
    parse_qs(url.query, keep_blank_values=True, strict_parsing=True)
    if url.query
    else {}
)
if set(parameters) - {"sslmode"} or any(len(values) != 1 for values in parameters.values()):
    sys.exit("qualification DATABASE_URL supports only one optional sslmode parameter")
fields = (
    url.hostname,
    str(url.port or 5432),
    unquote(url.username),
    unquote(url.password or ""),
    unquote(url.path[1:]),
    parameters.get("sslmode", [""])[0],
)
for field in fields:
    sys.stdout.buffer.write(field.encode("utf-8") + b"\0")
PY
)
if [[ "${#connection_fields[@]}" -ne 6 ]]; then
  echo "DATABASE_URL could not be converted to protected libpq settings" >&2
  exit 2
fi

verification_log="$(mktemp)"
trap 'rm -f "$verification_log"' EXIT
if ! "$runner_executable" verify-world --world-id "$world_id" \
  >"$verification_log" 2>&1; then
  echo "canonical replay verification failed" >&2
  sed -n '1,80p' "$verification_log" >&2
  exit 1
fi

libpq_environment=(
  "PGHOST=${connection_fields[0]}"
  "PGPORT=${connection_fields[1]}"
  "PGUSER=${connection_fields[2]}"
  "PGPASSWORD=${connection_fields[3]}"
  "PGDATABASE=${connection_fields[4]}"
)
if [[ -n "${connection_fields[5]}" ]]; then
  libpq_environment+=("PGSSLMODE=${connection_fields[5]}")
fi

report="$(env -u DATABASE_URL "${libpq_environment[@]}" psql -X -v ON_ERROR_STOP=1 -At \
  -v world_id="$world_id" \
  -v minimum_tick="$minimum_tick" \
  -v expected_ruleset="$expected_ruleset" <<'SQL'
WITH selected_world AS (
    SELECT * FROM worlds WHERE id = :'world_id'::UUID
), event_state AS (
    SELECT COUNT(*)::BIGINT AS batch_count,
           COALESCE(MIN(sequence), 0)::BIGINT AS minimum_sequence,
           COALESCE(MAX(sequence), 0)::BIGINT AS maximum_sequence,
           COALESCE(MAX(tick), 0)::BIGINT AS maximum_tick,
           COALESCE(MIN(event_schema_version), 0)::BIGINT AS minimum_event_schema,
           COALESCE(MAX(event_schema_version), 0)::BIGINT AS maximum_event_schema
    FROM event_batches WHERE world_id = :'world_id'::UUID
), snapshot_state AS (
    SELECT COUNT(*)::BIGINT AS snapshot_count,
           COALESCE(MAX(through_sequence), 0)::BIGINT AS newest_sequence,
           COALESCE(MAX(tick), 0)::BIGINT AS newest_tick
    FROM snapshots WHERE world_id = :'world_id'::UUID
), canonical_feature_state AS (
    SELECT COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_signal_action_association_changed'
           )::BIGINT AS signal_action_associations,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_signal_action_association_changed'
                 AND event -> 'event' -> 'data' -> 'to' -> 'movement_direction' IS NOT NULL
           )::BIGINT AS signal_motor_associations,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_movement_direction_value_changed'
           )::BIGINT AS movement_direction_values,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_acted'
                 AND event -> 'event' -> 'data' -> 'action' ->> 'kind' = 'emit_signal'
                 AND (event -> 'event' -> 'data' -> 'action' ->> 'intensity')::INTEGER > 1
           )::BIGINT AS varied_signals,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_acted'
                 AND event -> 'event' -> 'data' -> 'action' ->> 'kind' = 'move'
                 AND event -> 'event' -> 'data' -> 'action' -> 'movement_direction'
                       IS NOT NULL
           )::BIGINT AS directed_moves,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'world_configured'
                 AND (event -> 'event' -> 'data' -> 'configuration'
                      ->> 'configuration_schema_version')::INTEGER IN (5, 6)
                 AND event -> 'event' -> 'data' -> 'configuration'
                      -> 'local_weather_baseline' IS NOT NULL
           )::BIGINT AS weather_configurations,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'world_configured'
                 AND (event -> 'event' -> 'data' -> 'configuration'
                      ->> 'configuration_schema_version')::INTEGER = 6
                 AND event -> 'event' -> 'data' -> 'configuration'
                      -> 'local_surface_baseline' IS NOT NULL
           )::BIGINT AS surface_configurations,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_perceived'
                 AND jsonb_path_exists(
                       event -> 'event' -> 'data' -> 'perception' -> 'readings',
                       '$[*] ? (@.property_code == "water_flux")'
                     )
           )::BIGINT AS water_flux_perceptions,
           COUNT(*) FILTER (
               WHERE event -> 'event' ->> 'type' = 'organism_perceived'
                 AND jsonb_path_exists(
                       event -> 'event' -> 'data' -> 'perception' -> 'readings',
                       '$[*] ? (@.property_code == "air_motion")'
                     )
           )::BIGINT AS air_motion_perceptions
    FROM event_batches batch
    CROSS JOIN LATERAL jsonb_array_elements(batch.payload -> 'events') AS event
    WHERE batch.world_id = :'world_id'::UUID
), projection_state AS (
    SELECT COUNT(*) FILTER (
               WHERE projection_name IN (
                   'public-timeline-v1', 'public-organism-v1',
                   'public-finding-v1', 'public-world-telemetry-v1',
                   'public-artifact-v1'
               )
           )::BIGINT AS required_count,
           COUNT(*) FILTER (
               WHERE projection_name IN (
                   'public-timeline-v1', 'public-organism-v1',
                   'public-finding-v1', 'public-world-telemetry-v1',
                   'public-artifact-v1'
               ) AND through_sequence = (SELECT current_sequence FROM selected_world)
           )::BIGINT AS current_count
    FROM projection_offsets WHERE world_id = :'world_id'::UUID
), memory_state AS (
    SELECT COUNT(*)::BIGINT AS total,
           COUNT(*) FILTER (WHERE completed_at IS NULL)::BIGINT AS pending,
           COUNT(*) FILTER (WHERE last_error IS NOT NULL)::BIGINT AS errors
    FROM memory_outbox WHERE world_id = :'world_id'::UUID
), cognition_state AS (
    SELECT COUNT(*)::BIGINT AS requests,
           COUNT(*) FILTER (WHERE request.deadline_tick <= world.current_tick)::BIGINT AS due,
           COUNT(*) FILTER (
               WHERE request.deadline_tick <= world.current_tick AND latch.request_id IS NULL
           )::BIGINT AS due_without_latch,
           COUNT(*) FILTER (
               WHERE request.deadline_tick <= world.current_tick AND consumption.request_id IS NULL
           )::BIGINT AS due_without_consumption,
           COUNT(*) FILTER (WHERE request.deadline_tick > world.current_tick)::BIGINT AS future,
           COUNT(recall.request_id)::BIGINT AS recalled,
           COUNT(result.request_id)::BIGINT AS completed_results,
           COUNT(result.request_id) FILTER (
               WHERE result.result_payload -> 'receipt' IS NOT NULL
                 AND result.result_payload -> 'receipt' <> 'null'::JSONB
           )::BIGINT AS model_receipts,
           COUNT(*) FILTER (WHERE organism.role IS DISTINCT FROM 'person')::BIGINT
             AS non_person_requests
    FROM cognition_requests request
    JOIN selected_world world ON world.id = request.world_id
    LEFT JOIN cognition_deadline_latches latch USING (request_id)
    LEFT JOIN cognition_latch_consumptions consumption USING (request_id)
    LEFT JOIN cognition_recall_outcomes recall USING (request_id)
    LEFT JOIN cognition_results result USING (request_id)
    LEFT JOIN observer_organisms organism
      ON organism.world_id = request.world_id AND organism.organism_id = request.agent_id
), observer_state AS (
    SELECT
      (SELECT COUNT(*) FROM observer_organisms WHERE world_id = :'world_id'::UUID)::BIGINT AS organisms,
      (SELECT COUNT(*) FROM observer_timeline_items WHERE world_id = :'world_id'::UUID)::BIGINT AS timeline_items,
      (SELECT COUNT(*) FROM observer_findings WHERE world_id = :'world_id'::UUID)::BIGINT AS findings,
      (SELECT COUNT(*) FROM observer_artifact_traces WHERE world_id = :'world_id'::UUID)::BIGINT AS artifact_traces,
      (SELECT COUNT(*) FROM observer_artifact_traces WHERE world_id = :'world_id'::UUID
         AND contact_region IS NOT NULL)::BIGINT AS regional_artifact_traces
), facts AS (
    SELECT world.*, event_state.*, snapshot_state.*, canonical_feature_state.*,
           projection_state.required_count AS projection_required_count,
           projection_state.current_count AS projection_current_count,
           memory_state.total AS memory_total,
           memory_state.pending AS memory_pending,
           memory_state.errors AS memory_errors,
           cognition_state.*, observer_state.*
    FROM selected_world world
    CROSS JOIN event_state
    CROSS JOIN snapshot_state
    CROSS JOIN canonical_feature_state
    CROSS JOIN projection_state
    CROSS JOIN memory_state
    CROSS JOIN cognition_state
    CROSS JOIN observer_state
), checks AS (
    SELECT facts.*,
           status = 'running' AS running,
           ruleset_version = :'expected_ruleset'::INTEGER AS expected_ruleset,
           current_tick >= :'minimum_tick'::BIGINT AS sufficient_history,
           batch_count = current_sequence
             AND minimum_sequence = 1
             AND maximum_sequence = current_sequence
             AND maximum_tick = current_tick AS contiguous_history,
           snapshot_count > 0 AND newest_sequence <= current_sequence
             AND newest_tick <= current_tick AS snapshots_present,
           projection_required_count = 5 AND projection_current_count = 5 AS projections_current,
           memory_total > 0 AND memory_pending = 0 AND memory_errors = 0 AS memory_delivered,
           requests > 0 AND due > 0 AND due_without_latch = 0
             AND due_without_consumption = 0 AS cognition_deadlines_complete,
           recalled > 0 AND model_receipts > 0 AS hindsight_cognition_exercised,
           organisms > 0 AND timeline_items > 0 AND findings > 0 AS observer_content_present,
           (ruleset_version < 19 OR artifact_traces > 0) AS material_transformation_exercised,
           (ruleset_version < 20 OR regional_artifact_traces > 0) AS surface_arrangement_exercised
           , (ruleset_version < 21 OR varied_signals > 0) AS acoustic_variation_exercised
           , (ruleset_version < 22 OR signal_action_associations > 0)
               AS signal_action_association_exercised
           , (ruleset_version < 23 OR directed_moves > 0) AS selectable_movement_exercised
           , (ruleset_version < 24 OR movement_direction_values > 0)
               AS movement_direction_learning_exercised
           , (ruleset_version < 25 OR signal_motor_associations > 0)
               AS signal_motor_association_exercised
           , (ruleset_version < 26 OR non_person_requests = 0)
               AS person_only_cognition
           , (ruleset_version < 27 OR weather_configurations = 1) AS local_weather_bound
           , (ruleset_version < 28 OR (
                 minimum_event_schema >= 28 AND maximum_event_schema >= 28
                 AND water_flux_perceptions > 0 AND air_motion_perceptions > 0
             )) AS local_atmospheric_flux_exercised
           , (ruleset_version < 29 OR (
                 minimum_event_schema = 29 AND maximum_event_schema = 29
                 AND surface_configurations = 1
             )) AS terrain_movement_bound
    FROM facts
)
SELECT jsonb_build_object(
    'schema_version', 1,
    'world_id', id,
    'passed', running AND expected_ruleset AND sufficient_history
      AND contiguous_history AND snapshots_present AND projections_current
      AND memory_delivered AND cognition_deadlines_complete
      AND hindsight_cognition_exercised AND observer_content_present
      AND material_transformation_exercised AND surface_arrangement_exercised
      AND acoustic_variation_exercised AND signal_action_association_exercised
      AND selectable_movement_exercised AND movement_direction_learning_exercised
      AND signal_motor_association_exercised AND person_only_cognition
      AND local_weather_bound AND local_atmospheric_flux_exercised
      AND terrain_movement_bound,
    'replay_verified', true,
    'world', jsonb_build_object(
      'status', status, 'ruleset_version', ruleset_version,
      'current_tick', current_tick, 'current_sequence', current_sequence
    ),
    'history', jsonb_build_object(
      'event_batches', batch_count, 'latest_tick', maximum_tick,
      'snapshots', snapshot_count, 'newest_snapshot_sequence', newest_sequence,
      'newest_snapshot_tick', newest_tick
    ),
    'projections', jsonb_build_object(
      'required', projection_required_count, 'current', projection_current_count
    ),
    'memory', jsonb_build_object(
      'total', memory_total, 'pending', memory_pending, 'errors', memory_errors
    ),
    'cognition', jsonb_build_object(
      'requests', requests, 'due', due, 'future', future,
      'due_without_latch', due_without_latch,
      'due_without_consumption', due_without_consumption,
      'recalled', recalled, 'completed_results', completed_results,
      'model_receipts', model_receipts, 'non_person_requests', non_person_requests
    ),
    'observer', jsonb_build_object(
      'organisms', organisms, 'timeline_items', timeline_items, 'findings', findings,
      'artifact_traces', artifact_traces,
      'regional_artifact_traces', regional_artifact_traces
    ),
    'canonical_features', jsonb_build_object(
      'varied_signals', varied_signals,
      'signal_action_associations', signal_action_associations
      , 'signal_motor_associations', signal_motor_associations
      , 'directed_moves', directed_moves
      , 'movement_direction_values', movement_direction_values
      , 'weather_configurations', weather_configurations
      , 'surface_configurations', surface_configurations
      , 'water_flux_perceptions', water_flux_perceptions
      , 'air_motion_perceptions', air_motion_perceptions
    ),
    'checks', jsonb_build_object(
      'running', running, 'expected_ruleset', expected_ruleset,
      'sufficient_history', sufficient_history,
      'contiguous_history', contiguous_history,
      'snapshots_present', snapshots_present,
      'projections_current', projections_current,
      'memory_delivered', memory_delivered,
      'cognition_deadlines_complete', cognition_deadlines_complete,
      'hindsight_cognition_exercised', hindsight_cognition_exercised,
      'observer_content_present', observer_content_present,
      'material_transformation_exercised', material_transformation_exercised,
      'surface_arrangement_exercised', surface_arrangement_exercised
      , 'acoustic_variation_exercised', acoustic_variation_exercised
      , 'signal_action_association_exercised', signal_action_association_exercised
      , 'selectable_movement_exercised', selectable_movement_exercised
      , 'movement_direction_learning_exercised', movement_direction_learning_exercised
      , 'signal_motor_association_exercised', signal_motor_association_exercised
      , 'person_only_cognition', person_only_cognition
      , 'local_weather_bound', local_weather_bound
      , 'local_atmospheric_flux_exercised', local_atmospheric_flux_exercised
      , 'terrain_movement_bound', terrain_movement_bound
    )
)::TEXT
FROM checks;
SQL
)"

if [[ -z "$report" ]]; then
  echo "qualification world was not found" >&2
  exit 1
fi
printf '%s\n' "$report"
if [[ "$report" != *'"passed": true'* ]]; then
  exit 1
fi
