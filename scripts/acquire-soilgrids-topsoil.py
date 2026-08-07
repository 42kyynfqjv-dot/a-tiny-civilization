#!/usr/bin/env python3
"""Acquire a breadth-first global SoilGrids topsoil evidence set.

SoilGrids publishes official BigTIFF overview pyramids beside each global VRT.
The first overview is approximately one kilometre, which is sufficient to wire a
provisional S2 L10 soil path without downloading every native 250 m source tile.
Final scientific admission must return to all six depths and the native source.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request


RELEASE = "latest"
BASE_URL = f"https://files.isric.org/soilgrids/{RELEASE}/data"
PROPERTIES = ("bdod", "cec", "cfvo", "clay", "nitrogen", "phh2o", "sand", "silt", "soc")
QUANTILES = ("Q0.05", "Q0.5", "Q0.95")
DEPTH = "0-5cm"


def inventory() -> list[dict[str, str]]:
    items = []
    for soil_property in PROPERTIES:
        for quantile in QUANTILES:
            filename = f"{soil_property}_{DEPTH}_{quantile}.vrt.ovr"
            items.append(
                {
                    "artifact_path": f"soilgrids-2-0-topsoil-overviews/{soil_property}/{filename}",
                    "download_url": f"{BASE_URL}/{soil_property}/{filename}",
                }
            )
    return items


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
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    if args.workers < 1 or args.workers > 16:
        parser.error("--workers must be between 1 and 16")

    items = inventory()
    if not args.download:
        print(json.dumps({"release": RELEASE, "depth": DEPTH, "artifacts": items}))
        return 0

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        results = list(executor.map(lambda item: download_one(args.output_directory, item), items))
    results.sort(key=lambda item: str(item["artifact_path"]))
    print(
        json.dumps(
            {
                "release": RELEASE,
                "depth": DEPTH,
                "artifact_count": len(results),
                "byte_length": sum(int(item["byte_length"]) for item in results),
                "artifacts": results,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
