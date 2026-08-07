#!/usr/bin/env python3
"""Acquire a pinned CC-BY iNaturalist Open Range Map release.

This script knows the complete Animalia package inventory for geomodel v2.20. It
never consults the mutable ``latest`` prefix, never overwrites bytes, and records
hashes only after a successful atomic download. A map is modelled range evidence,
not an observation, census, or direct assertion of habitat suitability.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request


VERSION = "2.20"
PREFIX = f"https://inaturalist-open-data.s3.us-east-1.amazonaws.com/geomodel/geopackages/{VERSION}"
ANIMAL_PACKAGES = (
    "Actinopterygii",
    "Amphibia",
    "Arachnida",
    "Aves_1",
    "Aves_2",
    "Insecta_1",
    "Insecta_2",
    "Insecta_3",
    "Insecta_4",
    "Insecta_5",
    "Insecta_6",
    "Insecta_7",
    "Mammalia",
    "Mollusca",
    "OtherAnimalia",
    "Protozoa",
    "Reptilia",
)
METADATA = (
    ("metadata.json", "application/json"),
    ("taxonomy.csv", "text/csv"),
)
LICENSE_EVIDENCE = {
    "artifact_path": f"inaturalist-open-range-maps-{VERSION}/provenance/license-cc-by-4.0.html",
    "download_url": "https://spdx.org/licenses/CC-BY-4.0.html",
    "media_type": "text/html",
}


def artifact(path: str, media_type: str) -> dict[str, str]:
    return {
        # Canonical artifact paths are portable lowercase names; the URL preserves
        # iNaturalist's published mixed-case object name exactly.
        "artifact_path": f"inaturalist-open-range-maps-{VERSION}/{path.lower()}",
        "download_url": f"{PREFIX}/{path}",
        "media_type": media_type,
    }


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def download_one(root: Path, item: dict[str, str]) -> dict[str, str | int]:
    destination = root / item["artifact_path"]
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        byte_length = destination.stat().st_size
        if byte_length == 0:
            raise RuntimeError(f"refusing zero-byte existing artifact: {destination}")
        return {**item, "byte_length": byte_length, "content_hash": sha256(destination), "status": "retained"}
    descriptor, partial_name = tempfile.mkstemp(
        prefix=f".{destination.name}.", suffix=".partial", dir=destination.parent
    )
    os.close(descriptor)
    partial = Path(partial_name)
    digest = hashlib.sha256()
    byte_length = 0
    try:
        request = urllib.request.Request(item["download_url"], headers={"User-Agent": "a-tiny-civilization/0.1"})
        with urllib.request.urlopen(request, timeout=300) as response, partial.open("wb") as output:
            while chunk := response.read(1024 * 1024):
                output.write(chunk)
                digest.update(chunk)
                byte_length += len(chunk)
            output.flush()
            os.fsync(output.fileno())
        if byte_length == 0:
            raise RuntimeError(f"received zero bytes from {item['download_url']}")
        try:
            os.link(partial, destination)
        except FileExistsError:
            if sha256(destination) != digest.hexdigest():
                raise RuntimeError(f"concurrent artifact differs: {destination}")
        return {
            **item, "byte_length": destination.stat().st_size,
            "content_hash": sha256(destination), "status": "downloaded",
        }
    finally:
        partial.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-directory", type=Path, default=Path("data/source-cache"))
    parser.add_argument("--download-metadata", action="store_true", help="download pinned metadata and taxonomy crosswalk")
    parser.add_argument("--download-packages", action="store_true", help="download every pinned Animalia GeoPackage")
    parser.add_argument("--workers", type=int, default=2)
    args = parser.parse_args()
    if args.workers < 1 or args.workers > 8:
        raise ValueError("--workers must be between 1 and 8")
    metadata = [artifact(path, media_type) for path, media_type in METADATA] + [LICENSE_EVIDENCE]
    packages = [artifact(f"iNaturalist_geomodel_{name}.gpkg", "application/geopackage+sqlite3") for name in ANIMAL_PACKAGES]
    if not args.download_metadata and not args.download_packages:
        print(json.dumps({
            "release": VERSION,
            "license_expression": "CC-BY-4.0",
            "license_url": "https://www.inaturalist.org/pages/range_maps",
            "range_kind": "modeled_presence_candidate",
            "animal_package_count": len(packages),
            "metadata_artifacts": metadata,
            "animal_packages": packages,
        }, sort_keys=True))
        return 0
    requested = ([] if not args.download_metadata else metadata) + ([] if not args.download_packages else packages)
    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        results = list(executor.map(lambda item: download_one(args.output_directory, item), requested))
    print(json.dumps({
        "release": VERSION,
        "license_expression": "CC-BY-4.0",
        "range_kind": "modeled_presence_candidate",
        "artifact_count": len(results),
        "byte_length": sum(int(item["byte_length"]) for item in results),
        "artifacts": results,
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
