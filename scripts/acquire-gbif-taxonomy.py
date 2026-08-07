#!/usr/bin/env python3
"""Acquire the frozen CC BY 4.0 GBIF Backbone Taxonomy release.

The dated archive supplies stable real-taxon identities for the breadth-first fauna
path. It does not establish distribution, abundance, physiology, or behavior.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request


RELEASE = "2023-08-28"
DATASET_KEY = "d7dddbf4-2cf0-4f39-9b2a-bb099caae36c"
PREFIX = f"https://hosted-datasets.gbif.org/datasets/backbone/{RELEASE}"
ARTIFACTS = (
    {
        "artifact_path": f"gbif-backbone-{RELEASE}/backbone.zip",
        "download_url": f"{PREFIX}/backbone.zip",
    },
    {
        "artifact_path": f"gbif-backbone-{RELEASE}/README.html",
        "download_url": f"{PREFIX}/README.html",
    },
)


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
        size = destination.stat().st_size
        if size == 0:
            raise RuntimeError(f"refusing zero-byte existing artifact: {destination}")
        return {
            **item,
            "byte_length": size,
            "content_hash": sha256(destination),
            "status": "retained",
        }

    descriptor, partial_name = tempfile.mkstemp(
        prefix=f".{destination.name}.", suffix=".partial", dir=destination.parent
    )
    os.close(descriptor)
    partial = Path(partial_name)
    digest = hashlib.sha256()
    byte_length = 0
    try:
        request = urllib.request.Request(
            item["download_url"], headers={"User-Agent": "a-tiny-civilization/0.1"}
        )
        with urllib.request.urlopen(request, timeout=120) as response, partial.open("wb") as out:
            while chunk := response.read(1024 * 1024):
                out.write(chunk)
                digest.update(chunk)
                byte_length += len(chunk)
            out.flush()
            os.fsync(out.fileno())
        if byte_length == 0:
            raise RuntimeError(f"received zero bytes for {item['download_url']}")
        try:
            os.link(partial, destination)
        except FileExistsError:
            pass
        if destination.stat().st_size != byte_length:
            raise RuntimeError(f"concurrent artifact differs in length: {destination}")
        return {
            **item,
            "byte_length": byte_length,
            "content_hash": digest.hexdigest(),
            "status": "downloaded",
        }
    finally:
        partial.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-directory", type=Path, default=Path("data/source-cache"))
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()

    if not args.download:
        print(
            json.dumps(
                {"release": RELEASE, "dataset_key": DATASET_KEY, "artifacts": ARTIFACTS}
            )
        )
        return 0

    results = [download_one(args.output_directory, item) for item in ARTIFACTS]
    print(
        json.dumps(
            {
                "release": RELEASE,
                "dataset_key": DATASET_KEY,
                "artifact_count": len(results),
                "byte_length": sum(int(item["byte_length"]) for item in results),
                "artifacts": results,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
