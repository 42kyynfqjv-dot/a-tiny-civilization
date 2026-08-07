#!/usr/bin/env python3
"""Query exact pinned iNaturalist range polygons at one geographic point.

This is a deterministic source adapter for the GeoPackage release. It first uses the
package's R-tree only as a bounding-box accelerator, then decodes each candidate's
GeoPackage/WKB MULTIPOLYGON and performs point-in-polygon tests. Returned records are
limited to the conservative GBIF crosswalk; a result means *modelled range contains
this point*, never measured presence, abundance, or a canonical spawning decision.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import struct
import sys


VERSION = "2.20"
PREFIX = f"inaturalist-open-range-maps-{VERSION}"


def f64(data: bytes, offset: int, endian: str) -> tuple[float, int]:
    if offset + 8 > len(data):
        raise ValueError("truncated WKB coordinate")
    return struct.unpack_from(endian + "d", data, offset)[0], offset + 8


def u32(data: bytes, offset: int, endian: str) -> tuple[int, int]:
    if offset + 4 > len(data):
        raise ValueError("truncated WKB integer")
    return struct.unpack_from(endian + "I", data, offset)[0], offset + 4


def endian_at(data: bytes, offset: int) -> tuple[str, int]:
    if offset >= len(data) or data[offset] not in (0, 1):
        raise ValueError("invalid WKB byte order")
    return ("<" if data[offset] == 1 else ">"), offset + 1


def parse_polygon(data: bytes, offset: int) -> tuple[list[list[tuple[float, float]]], int]:
    endian, offset = endian_at(data, offset)
    geometry_type, offset = u32(data, offset, endian)
    if geometry_type != 3:
        raise ValueError(f"expected WKB Polygon, got type {geometry_type}")
    ring_count, offset = u32(data, offset, endian)
    rings = []
    for _ in range(ring_count):
        point_count, offset = u32(data, offset, endian)
        if point_count < 4:
            raise ValueError("polygon ring has fewer than four points")
        ring = []
        for _ in range(point_count):
            longitude, offset = f64(data, offset, endian)
            latitude, offset = f64(data, offset, endian)
            ring.append((longitude, latitude))
        if ring[0] != ring[-1]:
            raise ValueError("polygon ring is not closed")
        rings.append(ring)
    return rings, offset


def geopackage_multipolygon(blob: bytes) -> list[list[list[tuple[float, float]]]]:
    if len(blob) < 8 or blob[:2] != b"GP" or blob[2] != 0:
        raise ValueError("invalid GeoPackage geometry header")
    flags = blob[3]
    if flags & 0b1111_0000 or not flags & 1:
        raise ValueError("unsupported GeoPackage geometry flags")
    envelope = (flags >> 1) & 0b111
    envelope_doubles = {0: 0, 1: 4, 2: 6, 3: 6, 4: 8}.get(envelope)
    if envelope_doubles is None:
        raise ValueError("unsupported GeoPackage envelope")
    offset = 8 + envelope_doubles * 8
    endian, offset = endian_at(blob, offset)
    geometry_type, offset = u32(blob, offset, endian)
    if geometry_type != 6:
        raise ValueError(f"expected WKB MultiPolygon, got type {geometry_type}")
    polygon_count, offset = u32(blob, offset, endian)
    polygons = []
    for _ in range(polygon_count):
        polygon, offset = parse_polygon(blob, offset)
        polygons.append(polygon)
    if offset != len(blob):
        raise ValueError("GeoPackage geometry has trailing bytes")
    return polygons


def unwrap(longitude: float, around: float) -> float:
    while longitude - around > 180:
        longitude -= 360
    while longitude - around <= -180:
        longitude += 360
    return longitude


def inside_ring(longitude: float, latitude: float, ring: list[tuple[float, float]]) -> bool:
    inside = False
    for (left_x, left_y), (right_x, right_y) in zip(ring, ring[1:]):
        left_x = unwrap(left_x, longitude)
        right_x = unwrap(right_x, longitude)
        if (left_y > latitude) != (right_y > latitude):
            crossing = left_x + (latitude - left_y) * (right_x - left_x) / (right_y - left_y)
            if crossing == longitude:
                return True
            if crossing > longitude:
                inside = not inside
    return inside


def contains(polygons: list[list[list[tuple[float, float]]]], longitude: float, latitude: float) -> bool:
    for polygon in polygons:
        if polygon and inside_ring(longitude, latitude, polygon[0]) and not any(
            inside_ring(longitude, latitude, hole) for hole in polygon[1:]
        ):
            return True
    return False


def candidates_in_package(path: Path, longitude: float, latitude: float, wanted_taxa: set[int]) -> list[tuple[int, int, bytes]]:
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        table_rows = connection.execute("SELECT table_name FROM gpkg_contents WHERE data_type = 'features'").fetchall()
        if len(table_rows) != 1:
            raise ValueError(f"{path.name} does not have one feature table")
        table = table_rows[0][0]
        rtree = f"rtree_{table}_geom"
        quoted_table = '"' + table.replace('"', '""') + '"'
        quoted_rtree = '"' + rtree.replace('"', '""') + '"'
        connection.execute("CREATE TEMP TABLE wanted_taxa (taxon_id INTEGER PRIMARY KEY)")
        connection.executemany("INSERT INTO wanted_taxa(taxon_id) VALUES (?)", ((taxon,) for taxon in sorted(wanted_taxa)))
        return connection.execute(
            f"SELECT features.fid, features.taxon_id, features.geom "
            f"FROM {quoted_table} AS features JOIN {quoted_rtree} AS bounds ON bounds.id = features.fid "
            "JOIN wanted_taxa ON wanted_taxa.taxon_id = features.taxon_id "
            "WHERE bounds.minx <= ? AND bounds.maxx >= ? AND bounds.miny <= ? AND bounds.maxy >= ? "
            "ORDER BY features.taxon_id",
            (longitude, longitude, latitude, latitude),
        ).fetchall()
    finally:
        connection.close()


def point_identifier(latitude_e7: int, longitude_e7: int) -> str:
    latitude = f"n{latitude_e7}" if latitude_e7 >= 0 else f"s{-latitude_e7}"
    longitude = f"e{longitude_e7}" if longitude_e7 >= 0 else f"w{-longitude_e7}"
    return f"inaturalist-v2-20-point-{latitude}-{longitude}"


def candidate_record(record: dict[str, str]) -> dict[str, object]:
    key = record["gbif_taxon_key"]
    return {
        "species": {
            "catalog": "gbif",
            "identifier": key,
            "scientific_name": record["scientific_name"],
            "source_url": f"https://www.gbif.org/species/{key}",
        },
        "inaturalist_taxon_id": int(record["inaturalist_taxon_id"]),
        "range_package": record["range_package"],
        "range_feature_fid": int(record["range_feature_fid"]),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--crosswalk", type=Path, required=True)
    parser.add_argument("--latitude-e7", type=int, required=True)
    parser.add_argument("--longitude-e7", type=int, required=True)
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument(
        "--gbif-taxon-key", action="append",
        help="a crosswalked GBIF species key; repeat to test a bounded candidate set",
    )
    selection.add_argument(
        "--all-crosswalked-species", action="store_true",
        help="evaluate every crosswalked Animalia range at this one point",
    )
    args = parser.parse_args()
    if not -900_000_000 <= args.latitude_e7 <= 900_000_000 or not -1_800_000_000 <= args.longitude_e7 <= 1_800_000_000:
        parser.error("coordinates must be WGS84 E7 bounds")
    crosswalk_bytes = args.crosswalk.read_bytes()
    crosswalk = json.loads(crosswalk_bytes)
    if crosswalk.get("inaturalist_release") != VERSION or crosswalk.get("crosswalk_schema_version") != 1:
        raise ValueError("unsupported iNaturalist-to-GBIF crosswalk")
    requested_keys = None
    if args.gbif_taxon_key is not None:
        try:
            requested_keys = {str(int(value)) for value in args.gbif_taxon_key if int(value) > 0}
        except ValueError as error:
            parser.error(f"--gbif-taxon-key must contain positive integers: {error}")
        if not requested_keys:
            parser.error("--gbif-taxon-key must contain at least one positive integer")
    by_package = {}
    for record in crosswalk["records"]:
        if requested_keys is None or record["gbif_taxon_key"] in requested_keys:
            by_package.setdefault(record["range_package"], {})[int(record["inaturalist_taxon_id"])] = record
    source = args.artifact_root.resolve(strict=True) / PREFIX
    longitude = args.longitude_e7 / 10_000_000
    latitude = args.latitude_e7 / 10_000_000
    matches = []
    for package, taxa in sorted(by_package.items()):
        path = source / f"inaturalist_geomodel_{package}.gpkg"
        for fid, taxon_id, geometry in candidates_in_package(path, longitude, latitude, set(taxa)):
            record = taxa.get(taxon_id)
            if record is not None and int(record["range_feature_fid"]) == fid and contains(geopackage_multipolygon(geometry), longitude, latitude):
                matches.append(record)
    matches.sort(key=lambda record: int(record["gbif_taxon_key"]))
    sys.stdout.write(json.dumps({
        "candidate_set_schema_version": 1,
        "candidate_set_id": point_identifier(args.latitude_e7, args.longitude_e7),
        "inaturalist_release": VERSION,
        "query_point": {
            "latitude_e7": args.latitude_e7,
            "longitude_e7": args.longitude_e7,
        },
        "source_crosswalk_digest": hashlib.sha256(crosswalk_bytes).hexdigest(),
        "source_gbif_catalog_digest": crosswalk["gbif_catalog_sha256"],
        "source_inaturalist_taxonomy_digest": crosswalk["inaturalist_taxonomy_sha256"],
        "candidates": [candidate_record(record) for record in matches],
    }, separators=(",", ":"), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
