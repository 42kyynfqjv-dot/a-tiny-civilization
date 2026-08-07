#!/usr/bin/env python3
"""Compile exact AnimalTraits metabolic-rate observations into canonical profiles.

Only already-standardised observations whose upstream unit is watts and whose exact
decimal representation fits the public profile contract are retained.  The compiler
never converts units, rounds values, fills gaps, averages observations, or turns a
measurement into an energetic policy.
"""
import argparse
import csv
from decimal import Decimal, InvalidOperation
import hashlib
import json
import os
import struct
import tempfile
from pathlib import Path


MAGIC = b"ATCGBF01"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read_string(handle):
    size = struct.unpack("<I", handle.read(4))[0]
    return handle.read(size).decode("utf-8")


def catalog_names(path):
    result = {}
    with open(path, "rb") as handle:
        if handle.read(8) != MAGIC or struct.unpack("<H", handle.read(2))[0] != 1:
            raise RuntimeError("unsupported GBIF Animalia catalog")
        handle.read(32)
        for _ in range(struct.unpack("<Q", handle.read(8))[0]):
            key = struct.unpack("<Q", handle.read(8))[0]
            scientific, canonical = read_string(handle), read_string(handle)
            for _ in range(5):
                read_string(handle)
            result.setdefault(canonical, []).append((key, scientific))
    return result


def exact_scaled(value):
    """Return the profile integer and decimal places without numeric conversion."""
    normalized = format(value, "f")
    whole, _, fraction = normalized.partition(".")
    fraction = fraction.rstrip("0")
    if len(fraction) > 9:
        return None
    digits = f"{whole}{fraction}".lstrip("+")
    return int(digits), len(fraction)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--observations", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    names = catalog_names(args.catalog)
    source = args.observations.read_bytes()
    profiles = []
    with args.observations.open(newline="", encoding="utf-8") as handle:
        for line_number, row in enumerate(csv.DictReader(handle), 2):
            name = row["species"].strip()
            candidates = names.get(name, [])
            raw = row["metabolic rate"].strip()
            unit = row["metabolic rate - units"].strip()
            if len(candidates) != 1 or unit != "W" or not raw:
                continue
            try:
                value = Decimal(raw)
                if not value.is_finite() or value <= 0:
                    continue
                scaled = exact_scaled(value)
            except (InvalidOperation, ValueError):
                continue
            if scaled is None:
                continue
            magnitude, decimal_places = scaled
            key, scientific = candidates[0]
            record = {
                "line": line_number,
                "species": name,
                "metabolic rate": raw,
                "metabolic rate - units": unit,
                "metabolic rate - method": row["metabolic rate - method"].strip(),
                "original temperature": row["original temperature"].strip(),
            }
            profiles.append({
                "species": {"catalog": "gbif", "identifier": str(key), "scientific_name": scientific, "source_url": f"https://www.gbif.org/species/{key}"},
                "trait_id": "standardized-metabolic-rate",
                "value": {"value": magnitude, "decimal_places": decimal_places, "unit": "W"},
                "source": "animal-traits-1.0.7",
                "source_field": "metabolic_rate",
                "source_record_id": f"animaltraits-observations-line-{line_number}",
                "source_record_digest": digest(json.dumps(record, sort_keys=True, separators=(",", ":")).encode()),
                "evidence_basis": "empirical_observation",
            })
    profiles.sort(key=lambda value: (value["species"]["catalog"], value["species"]["identifier"], value["trait_id"], value["source_record_id"]))
    if not profiles:
        raise RuntimeError("no exact, positive watt AnimalTraits metabolic-rate observations")
    payload = {"profile_set_schema_version": 1, "source_artifact_digest": digest(source), "profiles": profiles}
    data = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode()
    if args.output.exists():
        raise RuntimeError(f"refusing to replace {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(dir=args.output.parent, prefix=".metabolic-rate-")
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
        os.replace(temporary, args.output)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    print(json.dumps({"output_path": str(args.output), "content_hash": digest(data), "profile_count": len(profiles)}))


if __name__ == "__main__":
    main()
