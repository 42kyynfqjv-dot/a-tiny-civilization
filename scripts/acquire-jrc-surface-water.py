#!/usr/bin/env python3
"""Acquire the public JRC Global Surface Water v1.5 evidence tiles.

The official download page publishes a deterministic 10 degree tile grid.  This
operator tool enumerates that grid without scraping HTML, supports a network-free
dry run, and downloads missing artifacts without replacing completed files.
Scientific admission remains a later step: acquisition only retains exact bytes.
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


RELEASE = "VER1-5"
VERSION = "v1_5_2024"
BASE_URL = f"https://storage.googleapis.com/water-world/download2024/{RELEASE}"
DEFAULT_LAYERS = ("occurrence", "seasonality", "transitions")
SUPPORTED_LAYERS = frozenset(
    ("occurrence", "change", "seasonality", "recurrence", "transitions", "extent")
)
LONGITUDES = tuple(range(-180, 180, 10))
LATITUDES = tuple(range(80, -60, -10))


def coordinate_code(value: int, negative_suffix: str, positive_suffix: str) -> str:
    return f"{abs(value)}{negative_suffix if value < 0 else positive_suffix}"


def artifact(layer: str, longitude: int, latitude: int) -> tuple[str, str]:
    longitude_code = coordinate_code(longitude, "W", "E")
    latitude_code = coordinate_code(latitude, "S", "N")
    filename = f"{layer}_{longitude_code}_{latitude_code}_{VERSION}.tif"
    relative_path = f"jrc-global-surface-water-v1-5-2024/{layer}/{filename}"
    return relative_path, f"{BASE_URL}/{layer}/{filename}"


def inventory(layers: tuple[str, ...]) -> list[dict[str, str]]:
    return [
        {"artifact_path": path, "download_url": url}
        for layer in layers
        for latitude in LATITUDES
        for longitude in LONGITUDES
        for path, url in (artifact(layer, longitude, latitude),)
    ]


def parse_layers(raw: str) -> tuple[str, ...]:
    layers = tuple(sorted(set(part.strip() for part in raw.split(",") if part.strip())))
    unsupported = sorted(set(layers) - SUPPORTED_LAYERS)
    if not layers or unsupported:
        raise argparse.ArgumentTypeError(
            "layers must be a non-empty comma-separated subset of "
            + ",".join(sorted(SUPPORTED_LAYERS))
        )
    return layers


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


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--layers",
        type=parse_layers,
        default=DEFAULT_LAYERS,
        help="comma-separated layers (default: occurrence,seasonality,transitions)",
    )
    parser.add_argument("--output-directory", type=Path, default=Path("data/source-cache"))
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    if args.workers < 1 or args.workers > 32:
        parser.error("--workers must be between 1 and 32")

    items = inventory(args.layers)
    if not args.download:
        print(json.dumps({"release": RELEASE, "version": VERSION, "artifacts": items}))
        return 0

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        results = list(executor.map(lambda item: download_one(args.output_directory, item), items))
    results.sort(key=lambda item: str(item["artifact_path"]))
    print(
        json.dumps(
            {
                "release": RELEASE,
                "version": VERSION,
                "artifact_count": len(results),
                "byte_length": sum(int(item["byte_length"]) for item in results),
                "artifacts": results,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
