#!/usr/bin/env python3
"""Acquire the fixed global ERA5 climate-normal evidence request.

This is an operator-side evidence acquisition tool, never a runner dependency. It
does not accept, print, or store a CDS token: ``cdsapi`` reads the owner's normal
CDS credential configuration. Each year is requested separately so a finished file
is small enough to audit and a failed acquisition cannot silently overwrite it.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

DATASET = "reanalysis-era5-single-levels-monthly-means"
PRODUCT_TYPE = "monthly_averaged_reanalysis"
NORMAL_START_YEAR = 1981
NORMAL_END_YEAR = 2010
VARIABLES = (
    "2m_temperature",
    "total_precipitation",
    "10m_u_component_of_wind",
    "10m_v_component_of_wind",
    "sea_surface_temperature",
    "sea_ice_cover",
)
MONTHS = tuple(f"{month:02d}" for month in range(1, 13))


def request_for(year: int) -> dict[str, object]:
    return {
        "product_type": [PRODUCT_TYPE],
        "variable": list(VARIABLES),
        "year": [str(year)],
        "month": list(MONTHS),
        "time": ["00:00"],
        "data_format": "netcdf",
    }


def output_path(root: Path, year: int) -> Path:
    return root / f"era5-monthly-single-levels-{NORMAL_START_YEAR}-{NORMAL_END_YEAR}-{year}.nc"


def publish_without_replacement(partial: Path, target: Path) -> None:
    """Publish a completed download without replacing a concurrently created file."""
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
        help="new or existing local evidence directory outside Git",
    )
    command.add_argument(
        "--year",
        type=int,
        action="append",
        help="one normal-period year; repeat to acquire a subset",
    )
    command.add_argument(
        "--dry-run",
        action="store_true",
        help="print exact request records without importing cdsapi or using the network",
    )
    return command


def main() -> int:
    arguments = parser().parse_args()
    years = sorted(set(arguments.year or range(NORMAL_START_YEAR, NORMAL_END_YEAR + 1)))
    invalid = [year for year in years if not NORMAL_START_YEAR <= year <= NORMAL_END_YEAR]
    if invalid:
        parser().error(f"years outside {NORMAL_START_YEAR}–{NORMAL_END_YEAR}: {invalid}")

    root = arguments.output_directory
    root.mkdir(parents=True, exist_ok=True)
    root = root.resolve(strict=True)
    if not root.is_dir() or root.is_symlink():
        parser().error("output directory must be a real directory")

    records = [
        {
            "dataset": DATASET,
            "target": output_path(root, year).name,
            "request": request_for(year),
        }
        for year in years
    ]
    if arguments.dry_run:
        print(json.dumps(records, indent=2, sort_keys=True))
        return 0

    occupied = [record["target"] for record in records if (root / record["target"]).exists()]
    if occupied:
        parser().error(f"refusing to replace existing evidence: {', '.join(occupied)}")

    try:
        import cdsapi
    except ImportError as error:
        parser().error(f"cdsapi>=0.7.7 is required: {error}")

    client = cdsapi.Client()
    for record in records:
        target = root / str(record["target"])
        partial = target.with_suffix(target.suffix + ".partial")
        if partial.exists():
            parser().error(f"refusing to reuse partial evidence file {partial.name}")
        client.retrieve(DATASET, record["request"], str(partial))
        with partial.open("rb") as downloaded:
            os.fsync(downloaded.fileno())
        publish_without_replacement(partial, target)
        print(target)
    return 0


if __name__ == "__main__":
    sys.exit(main())
