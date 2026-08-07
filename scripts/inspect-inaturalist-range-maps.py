#!/usr/bin/env python3
"""Inspect the pinned iNaturalist v2.20 Animalia GeoPackage source release.

This is an input validator, not a range normalizer. It proves that the retained
packages match the release metadata, use WGS84 multipolygons, carry only their pinned
geomodel version, and do not drift from the accompanying taxonomy crosswalk.
"""

from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
import sqlite3


VERSION = "2.20"
PREFIX = f"inaturalist-open-range-maps-{VERSION}"
COLLECTIONS = {
    "Actinopterygii": ("actinopterygii",),
    "Amphibia": ("amphibia",),
    "Arachnida": ("arachnida",),
    "Aves": ("aves_1", "aves_2"),
    "Insecta": ("insecta_1", "insecta_2", "insecta_3", "insecta_4", "insecta_5", "insecta_6", "insecta_7"),
    "Mammalia": ("mammalia",),
    "Mollusca": ("mollusca",),
    "OtherAnimalia": ("otheranimalia",),
    "Protozoa": ("protozoa",),
    "Reptilia": ("reptilia",),
}


def quoted(identifier: str) -> str:
    return '"' + identifier.replace('"', '""') + '"'


def load_taxonomy(path: Path) -> dict[int, str]:
    with path.open("r", encoding="utf-8", newline="") as stream:
        rows = csv.DictReader(stream)
        if rows.fieldnames != ["taxon_id", "parent_taxon_id", "name", "rank_level", "rank", "iconic_taxon_id", "is_leaf"]:
            raise ValueError("iNaturalist taxonomy.csv columns changed")
        taxonomy: dict[int, str] = {}
        for row in rows:
            taxon_id = int(row["taxon_id"])
            name = row["name"]
            if taxon_id <= 0 or not name or taxon_id in taxonomy:
                raise ValueError("iNaturalist taxonomy.csv contains an invalid taxon")
            taxonomy[taxon_id] = name
    if not taxonomy:
        raise ValueError("iNaturalist taxonomy.csv is empty")
    return taxonomy


def inspect_package(path: Path, taxonomy: dict[int, str]) -> dict[str, object]:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"range package is not a regular file: {path}")
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        contents = connection.execute(
            "SELECT table_name, data_type, srs_id FROM gpkg_contents ORDER BY table_name"
        ).fetchall()
        if len(contents) != 1 or contents[0][1:] != ("features", 4326):
            raise ValueError(f"{path.name} has an unexpected GeoPackage contents declaration")
        table = contents[0][0]
        geometry = connection.execute(
            "SELECT column_name, geometry_type_name, srs_id, z, m FROM gpkg_geometry_columns WHERE table_name = ?",
            (table,),
        ).fetchall()
        if geometry != [("geom", "MULTIPOLYGON", 4326, 0, 0)]:
            raise ValueError(f"{path.name} does not declare WGS84 MULTIPOLYGON geometry")
        columns = [row[1] for row in connection.execute(f"PRAGMA table_info({quoted(table)})")]
        expected = ["fid", "geom", "taxon_id", "parent_taxon_id", "name", "rank", "iconic_taxon_id", "iconic_taxon_name", "geomodel_version"]
        if columns != expected:
            raise ValueError(f"{path.name} feature schema changed: {columns!r}")
        rows = connection.execute(
            f"SELECT taxon_id, name, geomodel_version FROM {quoted(table)} ORDER BY taxon_id"
        )
        count = 0
        previous = 0
        for taxon_id, name, version in rows:
            if not isinstance(taxon_id, int) or taxon_id <= previous:
                raise ValueError(f"{path.name} has unordered or duplicate taxon IDs")
            if version != VERSION or not isinstance(name, str) or taxonomy.get(taxon_id) != name:
                raise ValueError(f"{path.name} feature does not match pinned taxonomy/version")
            previous = taxon_id
            count += 1
        if count == 0:
            raise ValueError(f"{path.name} has no ranges")
        return {"package": path.name, "feature_table": table, "range_count": count}
    finally:
        connection.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-root", type=Path, required=True)
    args = parser.parse_args()
    root = args.artifact_root.resolve(strict=True)
    source = root / PREFIX
    metadata = json.loads((source / "metadata.json").read_text(encoding="utf-8"))
    if metadata.get("version") != VERSION or not isinstance(metadata.get("collections"), dict):
        raise ValueError("iNaturalist release metadata changed")
    taxonomy = load_taxonomy(source / "taxonomy.csv")
    groups = []
    for collection, packages in COLLECTIONS.items():
        expected = metadata["collections"].get(collection, {}).get("ranges")
        if not isinstance(expected, int) or expected <= 0:
            raise ValueError(f"release metadata lacks a range count for {collection}")
        inspected = [inspect_package(source / f"inaturalist_geomodel_{package}.gpkg", taxonomy) for package in packages]
        count = sum(item["range_count"] for item in inspected)
        if count != expected:
            raise ValueError(f"{collection} has {count} ranges; metadata declares {expected}")
        groups.append({"collection": collection, "range_count": count, "packages": inspected})
    print(json.dumps({
        "inspection_schema_version": 1,
        "release": VERSION,
        "taxonomy_count": len(taxonomy),
        "animal_collection_count": len(groups),
        "animal_range_count": sum(group["range_count"] for group in groups),
        "collections": groups,
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
