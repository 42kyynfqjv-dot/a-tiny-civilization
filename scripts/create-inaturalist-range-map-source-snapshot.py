#!/usr/bin/env python3
"""Create a canonical source snapshot for retained iNaturalist range-map bytes."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from datetime import date
from pathlib import Path


VERSION = "2.20"
PREFIX = f"inaturalist-open-range-maps-{VERSION}"
REMOTE_PREFIX = f"https://inaturalist-open-data.s3.us-east-1.amazonaws.com/geomodel/geopackages/{VERSION}"
SNAPSHOT_ID = f"inaturalist-open-range-maps-v{VERSION.replace('.', '-')}-animalia"
PACKAGES = (
    "Actinopterygii", "Amphibia", "Arachnida", "Aves_1", "Aves_2", "Insecta_1",
    "Insecta_2", "Insecta_3", "Insecta_4", "Insecta_5", "Insecta_6", "Insecta_7",
    "Mammalia", "Mollusca", "OtherAnimalia", "Protozoa", "Reptilia",
)


def observed(root: Path, relative: str, role: str, url: str, media_type: str) -> dict[str, str]:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"required retained artifact is not a regular file: {relative}")
    if path.with_suffix(path.suffix + ".partial").exists():
        raise ValueError(f"incomplete artifact remains beside: {relative}")
    digest = hashlib.sha256()
    byte_length = 0
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
            byte_length += len(chunk)
    if byte_length == 0:
        raise ValueError(f"source artifact is empty: {relative}")
    return {
        "role": role, "artifact_path": relative, "download_url": url,
        "media_type": media_type, "content_hash": digest.hexdigest(), "byte_length": str(byte_length),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--retrieved-on", required=True)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    try:
        date.fromisoformat(args.retrieved_on)
    except ValueError:
        parser.error("--retrieved-on must be an ISO YYYY-MM-DD date")
    root = args.artifact_root.resolve(strict=True)
    if not root.is_dir() or root.is_symlink():
        parser.error("--artifact-root must be a real directory")
    if (args.output.exists() or args.output.is_symlink()) and not args.dry_run:
        parser.error(f"refusing to replace existing manifest: {args.output}")
    artifacts = [
        observed(root, f"{PREFIX}/metadata.json", "version_evidence", f"{REMOTE_PREFIX}/metadata.json", "application/json"),
        observed(root, f"{PREFIX}/taxonomy.csv", "documentation", f"{REMOTE_PREFIX}/taxonomy.csv", "text/csv"),
        observed(root, f"{PREFIX}/provenance/license-cc-by-4.0.html", "license_evidence", "https://spdx.org/licenses/CC-BY-4.0.html", "text/html"),
    ]
    artifacts.extend(
        observed(root, f"{PREFIX}/inaturalist_geomodel_{name.lower()}.gpkg", "data", f"{REMOTE_PREFIX}/iNaturalist_geomodel_{name}.gpkg", "application/geopackage+sqlite3")
        for name in PACKAGES
    )
    artifacts.sort(key=lambda item: item["artifact_path"])
    manifest = {
        "source_snapshot_schema_version": 1,
        "snapshot_id": SNAPSHOT_ID,
        "title": "iNaturalist Open Range Map Dataset v2.20, Animalia packages",
        "publisher": "iNaturalist",
        "documentation_url": "https://www.inaturalist.org/pages/range_maps",
        "upstream_release": VERSION,
        "upstream_revision": VERSION,
        "artifact_locator_policy": "evidence_bound_release",
        "dataset_version": VERSION,
        "retrieved_on": args.retrieved_on,
        "license_expression": "CC-BY-4.0",
        "license_url": "https://www.inaturalist.org/pages/range_maps",
        "scope": "All 17 v2.20 Animalia GeoPackages plus release metadata, taxonomy crosswalk, and CC BY terms. Maps are modeled expected-presence polygons trained from iNaturalist observations and elevation data.",
        "limitations": [
            "The maps are modeled ranges, not direct occurrence observations, habitat suitability measurements, or abundance estimates.",
            "Range accuracy varies with observation density and taxon coverage.",
            "The iNaturalist taxonomy has no direct GBIF-key crosswalk; any matching remains an explicit, exact-name or separately sourced operation.",
            "No artifact in this snapshot is exposed to agents or admitted directly as a canonical world-data bundle.",
        ],
        "artifacts": artifacts,
    }
    encoded = json.dumps(manifest, separators=(",", ":"), ensure_ascii=True).encode("utf-8") + b"\n"
    if args.dry_run:
        sys.stdout.buffer.write(encoded)
        return 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    with os.fdopen(descriptor, "wb") as output:
        output.write(encoded)
        output.flush()
        os.fsync(output.fileno())
    directory = os.open(args.output.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
