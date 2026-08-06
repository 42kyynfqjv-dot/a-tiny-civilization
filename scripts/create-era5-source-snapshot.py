#!/usr/bin/env python3
"""Create the canonical source-snapshot manifest for acquired ERA5 evidence.

The manifest remains outside the raw source cache, while every hash and byte length
in it is observed from immutable retained source artifacts. This script only writes a
new manifest and refuses missing, partial, or pre-existing evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import sys
from datetime import date
from pathlib import Path


NORMAL_START_YEAR = 1981
NORMAL_END_YEAR = 2010
DATASET_SLUG = "reanalysis-era5-single-levels-monthly-means"
DATASET_URL = f"https://cds.climate.copernicus.eu/datasets/{DATASET_SLUG}"
DOCUMENTATION_URL = f"{DATASET_URL}?tab=documentation"
LICENCE_URL = "https://creativecommons.org/licenses/by/4.0/"
VERSION_EVIDENCE_URL = "https://doi.org/10.24381/cds.f17050d7"
SNAPSHOT_ID = "era5-single-levels-monthly-means-1981-2010"


def data_name(year: int) -> str:
    return f"era5-monthly-single-levels-{NORMAL_START_YEAR}-{NORMAL_END_YEAR}-{year}.zip"


def acquisition_module():
    path = Path(__file__).with_name("acquire-era5-monthly-climate.py")
    spec = importlib.util.spec_from_file_location("era5_acquisition", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load ERA5 acquisition contract")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def observed_artifact(root: Path, relative: str, role: str, url: str, media_type: str) -> dict[str, object]:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"required retained source artifact is not a regular file: {relative}")
    partial = path.with_suffix(path.suffix + ".partial")
    if partial.exists():
        raise ValueError(f"refusing source tree with an incomplete artifact: {partial.relative_to(root)}")
    digest = hashlib.sha256()
    byte_length = 0
    with path.open("rb") as source:
        while chunk := source.read(64 * 1024):
            digest.update(chunk)
            byte_length += len(chunk)
    if byte_length == 0:
        raise ValueError(f"source artifact is empty: {relative}")
    return {
        "role": role,
        "artifact_path": relative,
        "download_url": url,
        "media_type": media_type,
        "content_hash": digest.hexdigest(),
        "byte_length": str(byte_length),
    }


def observed_data_artifact(root: Path, relative: str) -> dict[str, object]:
    acquisition_module().validate_netcdf_archive(root / relative)
    return observed_artifact(root, relative, "data", DATASET_URL, "application/zip")


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(description=__doc__)
    command.add_argument(
        "--artifact-root",
        type=Path,
        required=True,
        help="root containing the ERA5 evidence directory, normally data/source-cache",
    )
    command.add_argument(
        "--output",
        type=Path,
        required=True,
        help="new canonical JSON manifest path; it must not already exist",
    )
    command.add_argument(
        "--retrieved-on",
        required=True,
        help="UTC acquisition date in YYYY-MM-DD form",
    )
    command.add_argument(
        "--dry-run",
        action="store_true",
        help="print canonical manifest bytes without writing them",
    )
    return command


def main() -> int:
    arguments = parser().parse_args()
    try:
        date.fromisoformat(arguments.retrieved_on)
    except ValueError:
        parser.error("--retrieved-on must be an ISO YYYY-MM-DD date")
    root = arguments.artifact_root.resolve(strict=True)
    if not root.is_dir() or root.is_symlink():
        parser.error("artifact root must be a real existing directory")
    output = arguments.output
    if output.exists() and not arguments.dry_run:
        parser.error(f"refusing to replace existing manifest: {output}")

    prefix = "era5-monthly-1981-2010"
    artifacts = [
        observed_data_artifact(root, f"{prefix}/{data_name(year)}")
        for year in range(NORMAL_START_YEAR, NORMAL_END_YEAR + 1)
    ]
    artifacts.extend(
        [
            observed_artifact(
                root,
                f"{prefix}/provenance/dataset-documentation.html",
                "documentation",
                DOCUMENTATION_URL,
                "text/html",
            ),
            observed_artifact(
                root,
                f"{prefix}/provenance/licence-evidence.html",
                "license_evidence",
                f"{DATASET_URL}?tab=download",
                "text/html",
            ),
            observed_artifact(
                root,
                f"{prefix}/provenance/version-evidence.html",
                "version_evidence",
                VERSION_EVIDENCE_URL,
                "text/html",
            ),
        ]
    )
    artifacts.sort(key=lambda artifact: str(artifact["artifact_path"]))
    manifest = {
        "source_snapshot_schema_version": 1,
        "snapshot_id": SNAPSHOT_ID,
        "title": "ERA5 monthly averaged data on single levels, 1981-2010 normal-period evidence",
        "publisher": "European Centre for Medium-Range Weather Forecasts",
        "documentation_url": DOCUMENTATION_URL,
        "upstream_release": DATASET_SLUG,
        "upstream_revision": "10.24381/cds.f17050d7",
        "artifact_locator_policy": "evidence_bound_release",
        "dataset_version": "ERA5 monthly averaged data on single levels from 1940 to present",
        "retrieved_on": arguments.retrieved_on,
        "license_expression": "CC-BY-4.0",
        "license_url": LICENCE_URL,
        "scope": "Thirty global annual ZIP responses containing NetCDF members for every month of 1981-2010 and six fixed ERA5 monthly-mean variables, plus retained official documentation, licence, and DOI version evidence.",
        "limitations": [
            "ERA5 is a reanalysis driven by observations and models; it is not a direct measurement at every simulation location or time.",
            "The retained 1981-2010 monthly means are climate evidence, not historical weather replay or a complete ecological state.",
            "No artifact in this snapshot is exposed to agents or accepted directly as a canonical world-data bundle.",
            "The source request includes sea-surface temperature and sea-ice cover but does not itself supply bathymetry, habitat, hydrography, soil, materials, or species evidence.",
        ],
        "artifacts": artifacts,
    }
    bytes_to_write = json.dumps(manifest, separators=(",", ":"), ensure_ascii=True).encode("utf-8") + b"\n"
    if arguments.dry_run:
        sys.stdout.buffer.write(bytes_to_write)
        return 0
    output.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    with os.fdopen(descriptor, "wb") as destination:
        destination.write(bytes_to_write)
        destination.flush()
        os.fsync(destination.fileno())
    directory = os.open(output.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    print(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
