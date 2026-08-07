#!/usr/bin/env python3
"""Merge deterministic package-scoped iNaturalist candidate-set fragments.

Every fragment must have the same point and pinned provenance. Output is the same
canonical wire form produced by query-inaturalist-range-candidates.py, ordered by the
numeric GBIF taxon key and without a trailing newline.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


HEADER_FIELDS = (
    "candidate_set_schema_version",
    "candidate_set_id",
    "inaturalist_release",
    "query_point",
    "source_crosswalk_digest",
    "source_gbif_catalog_digest",
    "source_inaturalist_taxonomy_digest",
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", type=Path, nargs="+", help="canonical fragment JSON files")
    args = parser.parse_args()
    fragments = [json.loads(path.read_bytes()) for path in args.inputs]
    first = fragments[0]
    for fragment in fragments:
        if any(fragment.get(field) != first.get(field) for field in HEADER_FIELDS):
            raise ValueError("candidate-set fragments disagree on point or pinned provenance")
        if not isinstance(fragment.get("candidates"), list):
            raise ValueError("candidate-set fragment has no candidates list")
    deduplicated = {}
    for candidate in (candidate for fragment in fragments for candidate in fragment["candidates"]):
        key = candidate["species"]["identifier"]
        source_order = (
            candidate["range_package"],
            int(candidate["inaturalist_taxon_id"]),
            int(candidate["range_feature_fid"]),
        )
        existing = deduplicated.get(key)
        existing_order = None if existing is None else (
            existing["range_package"],
            int(existing["inaturalist_taxon_id"]),
            int(existing["range_feature_fid"]),
        )
        if existing_order is None or source_order < existing_order:
            deduplicated[key] = candidate
    candidates = sorted(deduplicated.values(), key=lambda candidate: int(candidate["species"]["identifier"]))
    merged = {field: first[field] for field in HEADER_FIELDS}
    merged["candidates"] = candidates
    sys.stdout.write(json.dumps(merged, separators=(",", ":"), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
