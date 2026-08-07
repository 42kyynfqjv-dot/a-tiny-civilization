#!/usr/bin/env python3
"""Compile exact AnimalTraits body-mass observations into canonical profiles.

The output retains every usable exact-name observation. It never selects a mean,
imputes a missing value, or maps a synonym to a GBIF identity.
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

def digest(data): return hashlib.sha256(data).hexdigest()
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
            for _ in range(5): read_string(handle)
            result.setdefault(canonical, []).append((key, scientific))
    return result

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
            value, unit = row["body mass"].strip(), row["body mass - units"].strip()
            if len(candidates) != 1 or not value or unit not in {"kg", "g"}:
                continue
            try:
                scaled_decimal = Decimal(value) * (1000 if unit == "kg" else 1)
                if scaled_decimal != scaled_decimal.to_integral_value(): raise ValueError
                scaled = int(scaled_decimal)
            except (InvalidOperation, ValueError):
                continue
            if scaled <= 0: continue
            key, scientific = candidates[0]
            record = {"line": line_number, "species": name, "body mass": value, "body mass - units": unit}
            profiles.append({"species": {"catalog":"gbif","identifier":str(key),"scientific_name":scientific,"source_url":f"https://www.gbif.org/species/{key}"}, "trait_id":"adult-body-mass", "value":{"value":scaled,"decimal_places":0,"unit":"g"}, "source":"animal-traits-1.0.7", "source_field":"body_mass", "source_record_id":f"animaltraits-observations-line-{line_number}", "source_record_digest":digest(json.dumps(record, sort_keys=True, separators=(",", ":")).encode()), "evidence_basis":"empirical_observation"})
    profiles.sort(key=lambda value: (value["species"]["catalog"], int(value["species"]["identifier"]), value["trait_id"], value["source_record_id"]))
    if not profiles: raise RuntimeError("no exact, positive AnimalTraits body-mass observations")
    payload = {"profile_set_schema_version":1, "source_artifact_digest":digest(source), "profiles":profiles}
    data = json.dumps(payload, separators=(",", ":")).encode()
    if args.output.exists(): raise RuntimeError(f"refusing to replace {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(dir=args.output.parent, prefix=".body-mass-")
    try:
        with os.fdopen(fd, "wb") as handle: handle.write(data)
        os.replace(temporary, args.output)
    finally:
        if os.path.exists(temporary): os.unlink(temporary)
    print(json.dumps({"output_path":str(args.output), "content_hash":digest(data), "profile_count":len(profiles)}))
if __name__ == "__main__": main()
