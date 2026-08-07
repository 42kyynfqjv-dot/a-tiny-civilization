#!/usr/bin/env python3
"""Retain exact public provenance for the acquired Copernicus land-cover response."""

from __future__ import annotations

import argparse
import os
import sys
import urllib.request
from pathlib import Path


DATASET_URL = "https://cds.climate.copernicus.eu/datasets/satellite-land-cover"
CATALOGUE_URL = (
    "https://cds.climate.copernicus.eu/api/catalogue/v1/collections/satellite-land-cover"
)
DOCUMENTATION_URL = f"{DATASET_URL}?tab=documentation"
DOWNLOAD_URL = f"{DATASET_URL}?tab=download"
ESA_CCI_LICENCE_URL = (
    "https://object-store.os-api.cci2.ecmwf.int:443/cci2-prod-catalogue/licences/"
    "satellite-land-cover/"
    "satellite-land-cover_8423d13d3dfd95bbeca92d9355516f21de90d9b40083a915ead15a189d6120fa.pdf"
)
CC_BY_LICENCE_URL = "https://spdx.org/licenses/CC-BY-4.0.html"
VITO_LICENCE_URL = (
    "https://object-store.os-api.cci2.ecmwf.int:443/cci2-prod-catalogue/licences/"
    "vito-proba-v/"
    "vito-proba-v_d729c524b2b07d74c7af460e9caf574ccdbcb4cd08406c886737551099a4bb07.pdf"
)
VERSION_EVIDENCE_URL = "https://doi.org/10.24381/cds.006f2c9a"
ARTIFACTS = (
    ("provenance/catalogue-metadata.json", CATALOGUE_URL),
    ("provenance/dataset-documentation.html", DOCUMENTATION_URL),
    ("provenance/dataset-download-and-licence.html", DOWNLOAD_URL),
    ("provenance/license-cc-by.html", CC_BY_LICENCE_URL),
    ("provenance/license-esa-cci-land-cover.pdf", ESA_CCI_LICENCE_URL),
    ("provenance/license-vito-proba-v.pdf", VITO_LICENCE_URL),
    ("provenance/version-evidence.html", VERSION_EVIDENCE_URL),
)


def publish_without_replacement(partial: Path, target: Path) -> None:
    """Atomically publish finished evidence without replacing another observation."""
    os.link(partial, target)
    partial.unlink()
    directory = os.open(target.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(description=__doc__)
    command.add_argument(
        "--output-directory",
        type=Path,
        required=True,
        help="existing land-cover evidence directory; provenance is retained beneath it",
    )
    command.add_argument(
        "--dry-run",
        action="store_true",
        help="print the exact public artifacts without network access or writes",
    )
    return command


def main() -> int:
    arguments = parser().parse_args()
    supplied_root = arguments.output_directory
    if supplied_root.is_symlink():
        parser().error("output directory must be a real existing directory")
    root = supplied_root.resolve(strict=True)
    if not root.is_dir():
        parser().error("output directory must be a real existing directory")

    if arguments.dry_run:
        for relative, url in ARTIFACTS:
            print(f"{relative}\t{url}")
        return 0

    for relative, url in ARTIFACTS:
        target = root / relative
        if target.exists() or target.is_symlink():
            parser().error(f"refusing to replace existing provenance: {relative}")
        target.parent.mkdir(parents=True, exist_ok=True)
        partial = target.with_suffix(target.suffix + ".partial")
        if partial.exists() or partial.is_symlink():
            parser().error(f"refusing to reuse partial provenance: {partial.name}")
        request = urllib.request.Request(url, headers={"User-Agent": "A-Tiny-Civilization/0.1"})
        with urllib.request.urlopen(request, timeout=60) as response, partial.open("xb") as output:
            while chunk := response.read(64 * 1024):
                output.write(chunk)
            output.flush()
            os.fsync(output.fileno())
        if partial.stat().st_size == 0:
            raise ValueError(f"empty provenance response from {url}")
        publish_without_replacement(partial, target)
        print(target)
    return 0


if __name__ == "__main__":
    sys.exit(main())
