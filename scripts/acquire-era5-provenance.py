#!/usr/bin/env python3
"""Retain exact public ERA5 provenance pages without replacing existing evidence.

This operator-side helper complements ``acquire-era5-monthly-climate.py``. It is
not a runner dependency and it never reads CDS credentials: these public pages
document the already-requested dataset, licence, and cited publication revision.
"""

from __future__ import annotations

import argparse
import os
import sys
import urllib.request
from pathlib import Path


DATASET_URL = "https://cds.climate.copernicus.eu/datasets/reanalysis-era5-single-levels-monthly-means"
DOCUMENTATION_URL = f"{DATASET_URL}?tab=documentation"
LICENCE_URL = f"{DATASET_URL}?tab=download"
VERSION_EVIDENCE_URL = "https://doi.org/10.24381/cds.f17050d7"
ARTIFACTS = (
    ("provenance/dataset-documentation.html", DOCUMENTATION_URL),
    ("provenance/licence-evidence.html", LICENCE_URL),
    ("provenance/version-evidence.html", VERSION_EVIDENCE_URL),
)


def publish_without_replacement(partial: Path, target: Path) -> None:
    """Atomically publish a finished provenance artifact without overwriting one."""
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
        help="existing ERA5 evidence directory; provenance is retained beneath it",
    )
    command.add_argument(
        "--dry-run",
        action="store_true",
        help="print the exact public artifacts without network access or writes",
    )
    return command


def main() -> int:
    arguments = parser().parse_args()
    root = arguments.output_directory.resolve(strict=True)
    if not root.is_dir() or root.is_symlink():
        parser().error("output directory must be a real existing directory")

    if arguments.dry_run:
        for relative, url in ARTIFACTS:
            print(f"{relative}\t{url}")
        return 0

    for relative, url in ARTIFACTS:
        target = root / relative
        if target.exists():
            parser().error(f"refusing to replace existing provenance: {relative}")
        target.parent.mkdir(parents=True, exist_ok=True)
        partial = target.with_suffix(target.suffix + ".partial")
        if partial.exists():
            parser().error(f"refusing to reuse partial provenance: {partial.name}")
        request = urllib.request.Request(url, headers={"User-Agent": "A-Tiny-Civilization/0.1"})
        try:
            with urllib.request.urlopen(request, timeout=60) as response, partial.open("xb") as output:
                while chunk := response.read(64 * 1024):
                    output.write(chunk)
                output.flush()
                os.fsync(output.fileno())
        except Exception:
            raise
        publish_without_replacement(partial, target)
        print(target)
    return 0


if __name__ == "__main__":
    sys.exit(main())
