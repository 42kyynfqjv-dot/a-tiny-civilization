#!/usr/bin/env python3
"""Compile exact EltonTraits diet, activity, and mass values without inference.

The two upstream tables use distinct schemas.  This compiler preserves each usable
source-compiled species aggregate as an independent canonical profile and does not
turn a diet share or activity flag into an engine behaviour.
"""
import argparse
import csv
import hashlib
import json
import os
import re
import struct
import tempfile
from pathlib import Path

MAGIC = b"ATCGBF01"
FIXED_DECIMAL = re.compile(r"^[0-9]+(?:\.[0-9]+)?$")


def digest(value):
    return hashlib.sha256(value).hexdigest()


def read_string(handle):
    return handle.read(struct.unpack("<I", handle.read(4))[0]).decode("utf-8")


def parse_fixed_decimal(value, *, source):
    """Return an exact integer mantissa and scale without binary float conversion."""
    if not value:
        return None
    if FIXED_DECIMAL.fullmatch(value) is None:
        raise RuntimeError(f"invalid fixed decimal {value!r} in {source}")
    whole, separator, fractional = value.partition(".")
    decimal_places = len(fractional) if separator else 0
    if decimal_places > 9:
        raise RuntimeError(f"excessive decimal precision in {source}: {value!r}")
    scaled = int(whole + fractional)
    if scaled > 2**63 - 1:
        raise RuntimeError(f"fixed decimal overflows signed 64-bit value in {source}")
    return scaled, decimal_places


def catalog_names(path):
    result = {}
    with path.open("rb") as handle:
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


def fields(table):
    diet = (
        ("Diet-Inv", "diet-invertebrate-share-percent"),
        ("Diet-Vend", "diet-terrestrial-vertebrate-share-percent"),
        ("Diet-Vect", "diet-ectotherm-vertebrate-share-percent"),
        ("Diet-Vfish", "diet-fish-share-percent"),
        ("Diet-Vunk", "diet-unknown-vertebrate-share-percent"),
        ("Diet-Scav", "diet-scavenged-animal-share-percent"),
        ("Diet-Fruit", "diet-fruit-share-percent"),
        ("Diet-Nect", "diet-nectar-share-percent"),
        ("Diet-Seed", "diet-seed-share-percent"),
        ("Diet-PlantO", "diet-other-plant-share-percent"),
    )
    if table == "bird":
        return diet + (("Nocturnal", "activity-nocturnal-flag", "flag"), ("BodyMass-Value", "adult-body-mass", "g"))
    return diet + (
        ("Activity-Nocturnal", "activity-nocturnal-flag", "flag"),
        ("Activity-Crepuscular", "activity-crepuscular-flag", "flag"),
        ("Activity-Diurnal", "activity-diurnal-flag", "flag"),
        ("BodyMass-Value", "adult-body-mass", "g"),
    )


def normalized_fields(table):
    result = []
    for item in fields(table):
        if len(item) == 2:
            column, trait = item
            result.append((column, trait, "percent"))
        else:
            result.append(item)
    return tuple(result)


def profiles_for_table(path, table, names):
    raw = path.read_bytes()
    text = raw.decode("cp1252")
    rows = csv.DictReader(text.splitlines(), delimiter="\t")
    if not rows.fieldnames or "Scientific" not in rows.fieldnames:
        raise RuntimeError(f"{path} has no Scientific column")
    required = {field for field, _, _ in normalized_fields(table)}
    missing = required.difference(rows.fieldnames)
    if missing:
        raise RuntimeError(f"{path} is missing {sorted(missing)}")
    profiles = []
    for line, row in enumerate(rows, 2):
        name = row["Scientific"].strip()
        candidates = names.get(name, [])
        if len(candidates) != 1:
            continue
        key, scientific = candidates[0]
        for column, trait, unit in normalized_fields(table):
            value = row[column].strip()
            parsed = parse_fixed_decimal(value, source=f"{path}:{line}:{column}")
            if parsed is None:
                continue
            scaled, decimal_places = parsed
            if unit == "g" and scaled == 0:
                continue
            record = {"line": line, "table": table, "scientific_name": name, "column": column, "value": value}
            profiles.append({
                "species": {"catalog": "gbif", "identifier": str(key), "scientific_name": scientific, "source_url": f"https://www.gbif.org/species/{key}"},
                "trait_id": trait,
                "value": {"value": scaled, "decimal_places": decimal_places, "unit": unit},
                "source": "elton-traits-1.0",
                "source_field": column,
                "source_record_id": f"elton-{table}-line-{line}",
                "source_record_digest": digest(json.dumps(record, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()),
                "evidence_basis": "source_compiled_species_aggregate",
            })
    return raw, profiles


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", required=True, type=Path)
    parser.add_argument("--birds", required=True, type=Path)
    parser.add_argument("--mammals", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    names = catalog_names(args.catalog)
    birds, bird_profiles = profiles_for_table(args.birds, "bird", names)
    mammals, mammal_profiles = profiles_for_table(args.mammals, "mammal", names)
    profiles = bird_profiles + mammal_profiles
    profiles.sort(key=lambda value: (value["species"]["catalog"], value["species"]["identifier"], value["trait_id"], value["source_record_id"]))
    if not profiles:
        raise RuntimeError("no exact EltonTraits profiles")
    source_set = json.dumps(
        {"bird_func_dat": digest(birds), "mammal_func_dat": digest(mammals)},
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    payload = {"profile_set_schema_version": 1, "source_artifact_digest": digest(source_set), "profiles": profiles}
    data = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode()
    if args.output.exists():
        raise RuntimeError(f"refusing to replace {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(dir=args.output.parent, prefix=".eltontraits-")
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
