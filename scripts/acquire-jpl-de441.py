#!/usr/bin/env python3
"""Acquire JPL DE441 long-range Sun, Moon, and planetary evidence.

DE441 spans years -13,200 through +17,191. The two SPK parts are large, so this
tool is resumable, refuses replacement, and hashes every retained byte.
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


RELEASE = "de441"
PLANET_BASE = "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets"
PCK_BASE = "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck"
ARTIFACTS = (
    {
        "artifact_path": "jpl-de441/de441_part-1.bsp",
        "download_url": f"{PLANET_BASE}/de441_part-1.bsp",
    },
    {
        "artifact_path": "jpl-de441/de441_part-2.bsp",
        "download_url": f"{PLANET_BASE}/de441_part-2.bsp",
    },
    {
        "artifact_path": "jpl-de441/provenance/de441_tech-comments.txt",
        "download_url": f"{PLANET_BASE}/de441_tech-comments.txt",
    },
    {
        "artifact_path": "jpl-de441/provenance/planetary-checksums.txt",
        "download_url": f"{PLANET_BASE}/aa_checksums.txt",
    },
    {
        "artifact_path": "jpl-de441/provenance/pck00011.tpc",
        "download_url": f"{PCK_BASE}/pck00011.tpc",
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
        with urllib.request.urlopen(request, timeout=180) as response, partial.open("wb") as out:
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
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    if args.workers < 1 or args.workers > 4:
        parser.error("--workers must be between 1 and 4")

    if not args.download:
        print(json.dumps({"release": RELEASE, "artifacts": ARTIFACTS}))
        return 0

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        results = list(executor.map(lambda item: download_one(args.output_directory, item), ARTIFACTS))
    results.sort(key=lambda item: str(item["artifact_path"]))
    print(
        json.dumps(
            {
                "release": RELEASE,
                "artifact_count": len(results),
                "byte_length": sum(int(item["byte_length"]) for item in results),
                "artifacts": results,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
