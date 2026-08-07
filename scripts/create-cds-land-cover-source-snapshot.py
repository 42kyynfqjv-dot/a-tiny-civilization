#!/usr/bin/env python3
"""Create a canonical source snapshot for the acquired 2022 land-cover evidence."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import sys
from datetime import date
from pathlib import Path


DATASET_SLUG = "satellite-land-cover"
DATASET_URL = f"https://cds.climate.copernicus.eu/datasets/{DATASET_SLUG}"
DOCUMENTATION_URL = f"{DATASET_URL}?tab=documentation"
DOWNLOAD_URL = f"{DATASET_URL}?tab=download"
VERSION_EVIDENCE_URL = "https://doi.org/10.24381/cds.006f2c9a"
SNAPSHOT_ID = "copernicus-satellite-land-cover-v2-1-1-2022"
PREFIX = "copernicus-land-cover-2022"
DATA_NAME = "copernicus-satellite-land-cover-v2.1.1-2022.zip"
DATASET_VERSION = "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1"
LICENSE_EXPRESSION = (
    "CC-BY-4.0 AND LicenseRef-ESA-CCI-Land-Cover-Rev-1 "
    "AND LicenseRef-VITO-PROBA-V-Rev-1"
)


def acquisition_module():
    path = Path(__file__).with_name("acquire-cds-land-cover.py")
    spec = importlib.util.spec_from_file_location("cds_land_cover_acquisition", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load land-cover acquisition contract")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def observed_artifact(
    root: Path,
    relative: str,
    role: str,
    url: str,
    media_type: str,
) -> dict[str, object]:
    path = root / relative
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"required retained source artifact is not a regular file: {relative}")
    partial = path.with_suffix(path.suffix + ".partial")
    if partial.exists() or partial.is_symlink():
        raise ValueError(f"refusing source tree with an incomplete artifact: {partial.relative_to(root)}")
    digest = hashlib.sha256()
    byte_length = 0
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
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


def observed_data_artifact(root: Path) -> dict[str, object]:
    relative = f"{PREFIX}/{DATA_NAME}"
    acquisition_module().validate_response(root / relative)
    return observed_artifact(root, relative, "data", DATASET_URL, "application/zip")


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(description=__doc__)
    command.add_argument(
        "--artifact-root",
        type=Path,
        required=True,
        help="root containing the land-cover evidence directory, normally data/source-cache",
    )
    command.add_argument(
        "--output",
        type=Path,
        required=True,
        help="new canonical JSON manifest path; it must not already exist",
    )
    command.add_argument("--retrieved-on", required=True, help="UTC acquisition date in YYYY-MM-DD form")
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
        parser().error("--retrieved-on must be an ISO YYYY-MM-DD date")
    supplied_root = arguments.artifact_root
    if supplied_root.is_symlink():
        parser().error("artifact root must be a real existing directory")
    root = supplied_root.resolve(strict=True)
    if not root.is_dir():
        parser().error("artifact root must be a real existing directory")
    output = arguments.output
    if (output.exists() or output.is_symlink()) and not arguments.dry_run:
        parser().error(f"refusing to replace existing manifest: {output}")

    artifacts = [observed_data_artifact(root)]
    for relative, role, url, media_type in (
        (
            f"{PREFIX}/provenance/catalogue-metadata.json",
            "documentation",
            "https://cds.climate.copernicus.eu/api/catalogue/v1/collections/satellite-land-cover",
            "application/json",
        ),
        (
            f"{PREFIX}/provenance/dataset-documentation.html",
            "documentation",
            DOCUMENTATION_URL,
            "text/html",
        ),
        (
            f"{PREFIX}/provenance/dataset-download-and-licence.html",
            "license_evidence",
            DOWNLOAD_URL,
            "text/html",
        ),
        (
            f"{PREFIX}/provenance/license-cc-by.html",
            "license_evidence",
            "https://spdx.org/licenses/CC-BY-4.0.html",
            "text/html",
        ),
        (
            f"{PREFIX}/provenance/license-esa-cci-land-cover.pdf",
            "license_evidence",
            "https://object-store.os-api.cci2.ecmwf.int:443/cci2-prod-catalogue/licences/"
            "satellite-land-cover/"
            "satellite-land-cover_8423d13d3dfd95bbeca92d9355516f21de90d9b40083a915ead15a189d6120fa.pdf",
            "application/pdf",
        ),
        (
            f"{PREFIX}/provenance/license-vito-proba-v.pdf",
            "license_evidence",
            "https://object-store.os-api.cci2.ecmwf.int:443/cci2-prod-catalogue/licences/"
            "vito-proba-v/"
            "vito-proba-v_d729c524b2b07d74c7af460e9caf574ccdbcb4cd08406c886737551099a4bb07.pdf",
            "application/pdf",
        ),
        (
            f"{PREFIX}/provenance/version-evidence.html",
            "version_evidence",
            VERSION_EVIDENCE_URL,
            "text/html",
        ),
    ):
        artifacts.append(observed_artifact(root, relative, role, url, media_type))
    artifacts.sort(key=lambda artifact: str(artifact["artifact_path"]))

    manifest = {
        "source_snapshot_schema_version": 1,
        "snapshot_id": SNAPSHOT_ID,
        "title": "Copernicus C3S global land-cover map for 2022",
        "publisher": "Copernicus Climate Change Service",
        "documentation_url": DOCUMENTATION_URL,
        "upstream_release": DATASET_SLUG,
        "upstream_revision": "10.24381/cds.006f2c9a",
        "artifact_locator_policy": "evidence_bound_release",
        "dataset_version": DATASET_VERSION,
        "retrieved_on": arguments.retrieved_on,
        "license_expression": LICENSE_EXPRESSION,
        "license_url": DOWNLOAD_URL,
        "scope": "One global 2022 C3S v2.1.1 ZIP response containing the 300 m annual LCCS classification, processing state, current-pixel state, observation count, and change count, plus retained catalogue, documentation, licence, and DOI evidence.",
        "limitations": [
            "The product is a satellite-derived categorical classification with quality indicators, not a direct measurement of habitat suitability or species presence.",
            "The 2022 surface includes urban and cultivated land; reconstructing an agent-starting ecological baseline requires an explicit counterfactual policy and additional evidence.",
            "The nominal 300 m regular latitude-longitude grid is not an equal-area simulation grid and must be deterministically aggregated before use.",
            "Land-cover water and snow classes do not establish surveyed coastline, drainage topology, freshwater storage, or ocean state.",
            "The CDS collection lists ESA CCI, CC-BY, and VITO licence evidence across its multi-era lineage; downstream publication must retain the exact evidence and attribution recorded here.",
            "This source snapshot is evidence only and cannot authorize canonical genesis until normalization, independent validation, and bundle admission succeed.",
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
