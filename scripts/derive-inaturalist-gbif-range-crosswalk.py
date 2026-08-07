#!/usr/bin/env python3
"""Derive a conservative bridge from pinned iNaturalist ranges to GBIF species.

The two sources have distinct identifiers. This bridge accepts only a unique,
byte-for-byte scientific-name match for an iNaturalist *species* range feature and an
accepted GBIF Animalia species. It never follows synonyms, ranks, fuzzy matches, or
one-to-many names. The output locates a range feature inside a content-addressed
GeoPackage; it does not turn that modelled polygon into abundance or habitat truth.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import struct
import sys
import tempfile


GBIF_MAGIC = b"ATCGBF01"
GBIF_SCHEMA = 1
VERSION = "2.20"
PREFIX = f"inaturalist-open-range-maps-{VERSION}"
PACKAGES = (
    "actinopterygii", "amphibia", "arachnida", "aves_1", "aves_2", "insecta_1",
    "insecta_2", "insecta_3", "insecta_4", "insecta_5", "insecta_6", "insecta_7",
    "mammalia", "mollusca", "otheranimalia", "protozoa", "reptilia",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def read_exact(stream, length: int) -> bytes:
    value = stream.read(length)
    if len(value) != length:
        raise ValueError("truncated GBIF Animalia catalog")
    return value


def read_string(stream) -> str:
    length = struct.unpack("<I", read_exact(stream, 4))[0]
    if length > 1024 * 1024:
        raise ValueError("GBIF Animalia catalog has an oversized string")
    return read_exact(stream, length).decode("utf-8")


def load_gbif_names(path: Path) -> dict[str, list[int]]:
    names: dict[str, list[int]] = {}
    with path.open("rb") as stream:
        if read_exact(stream, 8) != GBIF_MAGIC:
            raise ValueError("GBIF Animalia catalog magic changed")
        if struct.unpack("<H", read_exact(stream, 2))[0] != GBIF_SCHEMA:
            raise ValueError("GBIF Animalia catalog schema changed")
        read_exact(stream, 32)  # Source-snapshot digest is bound by the caller's artifact hash.
        count = struct.unpack("<Q", read_exact(stream, 8))[0]
        if count == 0:
            raise ValueError("GBIF Animalia catalog is empty")
        for _ in range(count):
            key = struct.unpack("<Q", read_exact(stream, 8))[0]
            name = read_string(stream)
            for _ in range(6):
                read_string(stream)
            if key == 0 or not name:
                raise ValueError("GBIF Animalia catalog contains an invalid accepted species")
            names.setdefault(name, []).append(key)
        if stream.read(1):
            raise ValueError("GBIF Animalia catalog contains trailing bytes")
    return names


def load_inaturalist_taxonomy(path: Path) -> dict[int, tuple[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as stream:
        rows = csv.DictReader(stream)
        required = {"taxon_id", "name", "rank"}
        if rows.fieldnames is None or not required.issubset(rows.fieldnames):
            raise ValueError("iNaturalist taxonomy.csv columns changed")
        result: dict[int, tuple[str, str]] = {}
        for row in rows:
            identifier = int(row["taxon_id"])
            name, rank = row["name"], row["rank"]
            if identifier <= 0 or not name or not rank or identifier in result:
                raise ValueError("iNaturalist taxonomy.csv contains an invalid taxon")
            result[identifier] = (name, rank)
    return result


def package_ranges(path: Path) -> list[tuple[int, int]]:
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        contents = connection.execute("SELECT table_name FROM gpkg_contents WHERE data_type = 'features'").fetchall()
        if len(contents) != 1:
            raise ValueError(f"{path.name} does not contain exactly one feature table")
        table = contents[0][0]
        rows = connection.execute(f'SELECT fid, taxon_id FROM "{table.replace(chr(34), chr(34) * 2)}" ORDER BY taxon_id')
        result: list[tuple[int, int]] = []
        previous = 0
        for fid, taxon_id in rows:
            if not isinstance(fid, int) or not isinstance(taxon_id, int) or taxon_id <= previous:
                raise ValueError(f"{path.name} has unordered or duplicate taxon IDs")
            previous = taxon_id
            result.append((fid, taxon_id))
        return result
    finally:
        connection.close()


def atomic_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() or path.is_symlink():
        raise ValueError(f"refusing to replace existing crosswalk: {path}")
    descriptor, partial_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".partial", dir=path.parent)
    partial = Path(partial_name)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.link(partial, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        partial.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--gbif-catalog", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    root = args.artifact_root.resolve(strict=True)
    source = root / PREFIX
    gbif_names = load_gbif_names(args.gbif_catalog)
    taxonomy = load_inaturalist_taxonomy(source / "taxonomy.csv")
    records = []
    unresolved_non_species = unresolved_ambiguous = unresolved_absent = 0
    for package in PACKAGES:
        for fid, inaturalist_id in package_ranges(source / f"inaturalist_geomodel_{package}.gpkg"):
            name, rank = taxonomy[inaturalist_id]
            if rank != "species":
                unresolved_non_species += 1
                continue
            keys = gbif_names.get(name, [])
            if len(keys) == 1:
                records.append({
                    "gbif_taxon_key": str(keys[0]), "inaturalist_taxon_id": str(inaturalist_id),
                    "scientific_name": name, "range_package": package, "range_feature_fid": str(fid),
                })
            elif len(keys) == 0:
                unresolved_absent += 1
            else:
                unresolved_ambiguous += 1
    records.sort(key=lambda item: (int(item["gbif_taxon_key"]), int(item["inaturalist_taxon_id"])))
    if any(left["gbif_taxon_key"] == right["gbif_taxon_key"] for left, right in zip(records, records[1:])):
        raise ValueError("unique GBIF name matching produced duplicate accepted taxa")
    document = {
        "crosswalk_schema_version": 1,
        "method": "unique-byte-exact-scientific-name-match-at-species-rank",
        "gbif_catalog_sha256": sha256(args.gbif_catalog),
        "inaturalist_release": VERSION,
        "inaturalist_taxonomy_sha256": sha256(source / "taxonomy.csv"),
        "mapped_range_count": len(records),
        "unresolved_non_species_range_count": unresolved_non_species,
        "unresolved_absent_name_count": unresolved_absent,
        "unresolved_ambiguous_name_count": unresolved_ambiguous,
        "records": records,
    }
    payload = json.dumps(document, separators=(",", ":"), ensure_ascii=True).encode("utf-8") + b"\n"
    if args.dry_run:
        sys.stdout.buffer.write(payload)
    else:
        atomic_write(args.output, payload)
        print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
