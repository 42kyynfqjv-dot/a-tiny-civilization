#!/usr/bin/env python3
"""Acquire a breadth-first global SoilGrids topsoil evidence set.

SoilGrids publishes official BigTIFF overview pyramids beside each global VRT.
Both are retained: the VRT carries the global CRS, transform, and source mosaic;
the overview carries the breadth-first pixel payload.
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
            stem = f"{soil_property}_{DEPTH}_{quantile}.vrt"
            for filename, role in ((stem, "geometry"), (f"{stem}.ovr", "data")):
                items.append(
                    {
                        "artifact_path": f"soilgrids-2-0-topsoil-overviews/{soil_property}/{filename}",
                        "download_url": f"{BASE_URL}/{soil_property}/{filename}",
                        "role": role,
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


def portable_report(results: list[dict[str, str | int]]) -> dict[str, object]:
    artifacts = [
        {
            "artifact_path": str(item["artifact_path"]),
            "byte_length": int(item["byte_length"]),
            "content_hash": str(item["content_hash"]),
            "download_url": str(item["download_url"]),
            "role": str(item["role"]),
        }
        for item in results
    ]
    artifacts.sort(key=lambda item: item["artifact_path"])
    return {
        "inventory_schema_version": 1,
        "release": RELEASE,
        "depth": DEPTH,
        "artifact_count": len(artifacts),
        "byte_length": sum(int(item["byte_length"]) for item in artifacts),
        "artifacts": artifacts,
    }


def write_new_report(path: Path, report: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (
        json.dumps(report, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
        + "\n"
    ).encode("utf-8")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(encoded)
            stream.flush()
            os.fsync(stream.fileno())
    except BaseException:
        path.unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-directory", type=Path, default=Path("data/source-cache"))
    parser.add_argument("--report-output", type=Path)
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
    report = portable_report(results)
    if args.report_output is not None:
        write_new_report(args.report_output, report)
        print(
            json.dumps(
                {
                    "release": report["release"],
                    "depth": report["depth"],
                    "artifact_count": report["artifact_count"],
                    "byte_length": report["byte_length"],
                    "report_output": str(args.report_output),
                }
            )
        )
    else:
        print(json.dumps(report))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
