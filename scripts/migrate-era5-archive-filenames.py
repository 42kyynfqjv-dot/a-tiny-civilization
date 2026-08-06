#!/usr/bin/env python3
"""Atomically correct legacy ERA5 ZIP filenames without discarding source bytes."""

from __future__ import annotations

import argparse
import importlib.util
import os
import sys
from pathlib import Path


def acquisition_module():
    path = Path(__file__).with_name("acquire-era5-monthly-climate.py")
    spec = importlib.util.spec_from_file_location("era5_acquisition", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load ERA5 acquisition contract")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(description=__doc__)
    command.add_argument("--output-directory", type=Path, required=True)
    command.add_argument("--dry-run", action="store_true")
    return command


def main() -> int:
    arguments = parser().parse_args()
    root = arguments.output_directory.resolve(strict=True)
    if not root.is_dir() or root.is_symlink():
        parser().error("output directory must be a real existing directory")
    era5 = acquisition_module()
    pairs = [
        (era5.legacy_output_path(root, year), era5.output_path(root, year))
        for year in range(era5.NORMAL_START_YEAR, era5.NORMAL_END_YEAR + 1)
    ]
    for old, new in pairs:
        if not old.is_file() or old.is_symlink():
            parser().error(f"legacy archive is not a regular file: {old.name}")
        if new.exists():
            parser().error(f"refusing to replace corrected archive: {new.name}")
        if old.with_suffix(old.suffix + ".partial").exists():
            parser().error(f"legacy archive has an incomplete sibling: {old.name}.partial")
        era5.validate_netcdf_archive(old)
    if arguments.dry_run:
        for old, new in pairs:
            print(f"{old.name}\t{new.name}")
        return 0
    for old, new in pairs:
        os.link(old, new)
        old.unlink()
    directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    print(f"migrated {len(pairs)} ERA5 archives")
    return 0


if __name__ == "__main__":
    sys.exit(main())
