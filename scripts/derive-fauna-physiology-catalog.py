#!/usr/bin/env python3
"""Build a canonical catalog of independently compiled fauna profile artifacts."""
import argparse
import hashlib
import json
import os
import tempfile
from pathlib import Path


def digest(value):
    return hashlib.sha256(value).hexdigest()


def entry(path):
    value = json.loads(path.read_text())
    required = {"artifact_id", "content_hash", "profile_count"}
    missing = required.difference(value)
    if missing:
        raise RuntimeError(f"{path} lacks {sorted(missing)}")
    source_digest = value.get(
        "source_artifact_digest",
        value.get("source_artifact_set_digest", value.get("source_artifact_hash")),
    )
    if not source_digest:
        raise RuntimeError(f"{path} lacks a source artifact digest")
    return {
        "profile_set_id": value["artifact_id"].removesuffix("-profiles"),
        "profile_set_digest": value["content_hash"],
        "source_artifact_digest": source_digest,
        "profile_count": int(value["profile_count"]),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--inspection", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    entries = sorted((entry(path) for path in args.inspection), key=lambda value: value["profile_set_id"])
    if not entries or any(left["profile_set_id"] == right["profile_set_id"] for left, right in zip(entries, entries[1:])):
        raise RuntimeError("profile catalog needs unique source inspections")
    data = json.dumps({"profile_catalog_schema_version": 1, "profile_sets": entries}, separators=(",", ":")).encode()
    if args.output.exists():
        raise RuntimeError(f"refusing to replace {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(dir=args.output.parent, prefix=".fauna-physiology-catalog-")
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
        os.replace(temporary, args.output)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    print(json.dumps({"output_path": str(args.output), "content_hash": digest(data), "profile_set_count": len(entries), "profile_count": sum(item["profile_count"] for item in entries)}))


if __name__ == "__main__":
    main()
