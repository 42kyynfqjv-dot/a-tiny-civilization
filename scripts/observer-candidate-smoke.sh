#!/usr/bin/env bash
set -euo pipefail

# Fail-closed read-side smoke test for a projected launch candidate. This reads
# only public endpoints and never needs database or observer credentials.

if (( $# < 2 || $# > 3 )); then
  echo "usage: $0 BASE_URL WORLD_ID [EXPECTED_SEQUENCE]" >&2
  exit 2
fi

base_url="${1%/}"
world_id="$2"
expected_sequence="${3:-}"

if [[ ! "$base_url" =~ ^https?://[^/@?#]+(:[0-9]{1,5})?$ ]]; then
  echo "BASE_URL must be one uncredentialed HTTP(S) origin" >&2
  exit 2
fi
if [[ ! "$world_id" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]]; then
  echo "WORLD_ID must be a UUID" >&2
  exit 2
fi
if [[ -n "$expected_sequence" && ! "$expected_sequence" =~ ^[1-9][0-9]*$ ]]; then
  echo "EXPECTED_SEQUENCE must be a positive integer" >&2
  exit 2
fi
if ! command -v curl >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
  echo "curl and python3 are required" >&2
  exit 2
fi

temporary_directory="$(mktemp -d)"
cleanup() {
  rm -rf -- "$temporary_directory"
}
trap cleanup EXIT

fetch() {
  local name="$1"
  local path="$2"
  curl --fail --silent --show-error --max-time 20 \
    --output "${temporary_directory}/${name}.json" "${base_url}${path}"
}

fetch worlds /api/v1/worlds
fetch telemetry "/api/v1/worlds/${world_id}/telemetry"
fetch timeline "/api/v1/worlds/${world_id}/timeline?limit=500"
fetch findings "/api/v1/worlds/${world_id}/findings?limit=500"
fetch organisms "/api/v1/worlds/${world_id}/organisms?limit=500"
fetch artifacts "/api/v1/worlds/${world_id}/artifacts?limit=500"
fetch wiki "/api/v1/worlds/${world_id}/wiki"
fetch commitments "/api/v1/worlds/${world_id}/history-commitments?after_sequence=0&limit=2"

python3 - "$temporary_directory" "$world_id" "$expected_sequence" <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
world_id = sys.argv[2]
expected_sequence = sys.argv[3]


def load(name: str):
    with (root / f"{name}.json").open("r", encoding="utf-8") as source:
        return json.load(source)


worlds = load("worlds")
matches = [world for world in worlds.get("worlds", []) if world.get("world_id") == world_id]
if len(matches) != 1:
    raise SystemExit("candidate world must appear exactly once in the public world index")
world = matches[0]
if world.get("status") != "running":
    raise SystemExit("candidate world is not publicly reported as running")
if world.get("input_status") != "provisional-not-scientifically-admitted":
    raise SystemExit("candidate world lost its provisional scientific disclosure")
through_sequence = str(world.get("through_sequence", ""))
if not through_sequence.isdigit() or int(through_sequence) < 1:
    raise SystemExit("candidate world has no valid projected sequence")
if expected_sequence and through_sequence != expected_sequence:
    raise SystemExit(
        f"candidate sequence mismatch: expected {expected_sequence}, found {through_sequence}"
    )

telemetry = load("telemetry")
if telemetry.get("world_id") != world_id or str(telemetry.get("through_sequence")) != through_sequence:
    raise SystemExit("telemetry does not describe the selected projected cursor")
for projection in ("timeline", "organism_index", "findings", "telemetry", "artifacts"):
    if str(telemetry.get(f"{projection}_through_sequence")) != through_sequence:
        raise SystemExit(f"{projection} projection is behind the candidate cursor")
    if telemetry.get(f"{projection}_lag_batches") != 0:
        raise SystemExit(f"{projection} projection reports nonzero lag")
if telemetry.get("living_people", 0) < 1 or telemetry.get("living_fauna", 0) < 1:
    raise SystemExit("candidate telemetry has no living people or no living fauna")

timeline = load("timeline")
findings = load("findings")
organisms = load("organisms")
artifacts = load("artifacts")
wiki = load("wiki")
collections = {
    "timeline": timeline.get("items", []),
    "findings": findings.get("findings", []),
    "organisms": organisms.get("organisms", []),
    "artifacts": artifacts.get("artifacts", []),
    "wiki": wiki.get("entries", []),
}
for name, values in collections.items():
    if not isinstance(values, list) or not values:
        raise SystemExit(f"public {name} projection is empty")

private_keys = {
    "action_values",
    "bodily_regulation",
    "cognition",
    "developing_parent",
    "fatigue_load_second_squared",
    "recalled_memories",
    "reproductive_development",
    "reproductive_physiology",
}
explicit_copy = re.compile(
    r"\b(sex|sexual|intercourse|rape|genital|violence|violent|murder|injury)\b",
    re.IGNORECASE,
)


def inspect_public(value):
    if isinstance(value, dict):
        overlap = private_keys.intersection(value)
        if overlap:
            raise SystemExit(f"private mechanism keys reached a public projection: {sorted(overlap)}")
        for key, child in value.items():
            if key in {"title", "summary"} and isinstance(child, str) and explicit_copy.search(child):
                raise SystemExit("explicit sexual or violent copy reached a public projection")
            inspect_public(child)
    elif isinstance(value, list):
        for child in value:
            inspect_public(child)


for document in (timeline, findings, organisms, artifacts, wiki):
    inspect_public(document)

commitments = load("commitments")
if commitments.get("world_id") != world_id or not commitments.get("commitments"):
    raise SystemExit("public audit commitments are absent")
for commitment in commitments["commitments"]:
    if {"payload", "events", "event"}.intersection(commitment):
        raise SystemExit("canonical event payload reached the public commitment endpoint")
    for required in ("sequence", "batch_hash", "post_state_hash", "previous_event_hash"):
        if not commitment.get(required):
            raise SystemExit(f"public audit commitment is missing {required}")

print(
    json.dumps(
        {
            "status": "observer-candidate-smoke-passed",
            "world_id": world_id,
            "through_sequence": through_sequence,
            "timeline_items": len(collections["timeline"]),
            "findings": len(collections["findings"]),
            "organisms": len(collections["organisms"]),
            "artifacts": len(collections["artifacts"]),
            "wiki_entries": len(collections["wiki"]),
        },
        sort_keys=True,
        separators=(",", ":"),
    )
)
PY
