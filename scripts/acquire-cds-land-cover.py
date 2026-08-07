#!/usr/bin/env python3
"""Acquire the pinned global 2022 Copernicus satellite land-cover response."""

from __future__ import annotations

import argparse
import json
import os
import sys
import zipfile
from pathlib import Path, PurePosixPath


DATASET = "satellite-land-cover"
VERSION = "v2_1_1"
YEAR = "2022"
TARGET_NAME = "copernicus-satellite-land-cover-v2.1.1-2022.zip"


def request() -> dict[str, object]:
    return {"variable": "all", "version": [VERSION], "year": [YEAR]}


def validate_response(path: Path) -> tuple[str, ...]:
    try:
        with zipfile.ZipFile(path) as archive:
            members = archive.infolist()
            if not members:
                raise ValueError("land-cover response ZIP is empty")
            names = tuple(member.filename for member in members)
            for member in members:
                relative = PurePosixPath(member.filename)
                if (
                    member.is_dir()
                    or member.file_size == 0
                    or relative.is_absolute()
                    or ".." in relative.parts
                    or relative.suffix.lower() != ".nc"
                ):
                    raise ValueError(
                        f"unsafe or unexpected land-cover response member: {member.filename!r}"
                    )
            invalid = archive.testzip()
            if invalid is not None:
                raise ValueError(f"land-cover response member failed CRC: {invalid}")
            return names
    except zipfile.BadZipFile as error:
        raise ValueError("land-cover response is not a ZIP container") from error


def publish_without_replacement(partial: Path, target: Path) -> None:
    os.link(partial, target)
    partial.unlink()
    directory = os.open(target.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(description=__doc__)
    command.add_argument("--output-directory", type=Path, required=True)
    command.add_argument("--dry-run", action="store_true")
    return command


def main() -> int:
    arguments = parser().parse_args()
    record = {"dataset": DATASET, "target": TARGET_NAME, "request": request()}
    if arguments.dry_run:
        print(json.dumps(record, indent=2, sort_keys=True))
        return 0

    root = arguments.output_directory
    root.mkdir(parents=True, exist_ok=True)
    if root.is_symlink():
        parser().error("output directory must be a real directory")
    root = root.resolve(strict=True)
    if not root.is_dir():
        parser().error("output directory must be a real directory")
    target = root / TARGET_NAME
    partial = target.with_suffix(target.suffix + ".partial")
    if target.exists() or partial.exists():
        parser().error("refusing to replace existing land-cover evidence")

    try:
        import cdsapi
    except ImportError as error:
        parser().error(f"cdsapi>=0.7.7 is required: {error}")

    cdsapi.Client().retrieve(DATASET, request(), str(partial))
    members = validate_response(partial)
    with partial.open("rb") as downloaded:
        os.fsync(downloaded.fileno())
    publish_without_replacement(partial, target)
    print(json.dumps({"path": str(target), "members": members}, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
