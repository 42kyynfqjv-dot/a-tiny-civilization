#!/usr/bin/env python3
"""Create an immutable next provisional composition from a prior canonical manifest.

This deliberately changes only the version and the fauna-physiology evidence release.
It never replaces the source manifest and refuses to overwrite the output.
"""
import argparse
import json
import os
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--physiology-inspection", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        raise RuntimeError(f"refusing to replace {args.output}")
    composition = json.loads(args.input.read_text())
    inspection = json.loads(args.physiology_inspection.read_text())
    if composition["composition_version"] >= args.version:
        raise RuntimeError("new composition version must advance")
    required = {"artifact_id", "artifact_path", "media_type", "content_hash", "byte_length"}
    missing = required.difference(inspection)
    if missing:
        raise RuntimeError(f"physiology inspection lacks {sorted(missing)}")
    component = next(
        (entry for entry in composition["world_components"] if entry["kind"] == "fauna_physiology_evidence"),
        None,
    )
    if component is None:
        raise RuntimeError("source composition has no fauna physiology component")
    release = component["release"]
    for field in required:
        release[field] = inspection[field]
    release["scientific_scope"] = (
        "Canonical catalog of independently compiled source-pinned fauna physiology "
        "profiles, including measured metabolic-rate observations."
    )
    release["limitations"] = [
        "It retains normalized evidence references rather than an ecological model.",
        "Profile coverage is incomplete and does not establish abundance, habitat suitability, or behavior.",
        "Metabolic observations are source measurements, not environment-corrected energetic policies.",
    ]
    release["limitations"].sort()
    composition["composition_version"] = args.version
    data = json.dumps(composition, separators=(",", ":"), ensure_ascii=False).encode() + b"\n"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(dir=args.output.parent, prefix=".provisional-composition-")
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
        os.replace(temporary, args.output)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


if __name__ == "__main__":
    main()
