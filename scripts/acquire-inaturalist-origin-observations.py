#!/usr/bin/env python3
"""Retain a bounded, commercially reusable iNaturalist occurrence query.

The output is source evidence, not a population estimate. It requests research-grade,
wild Animalia observations whose observation records are CC0 or CC BY within a fixed
radius of one WGS84 point. Every raw response page is retained byte-for-byte and a
canonical manifest pins the exact query, page order, lengths, and SHA-256 digests.
The destination is published once and is never replaced.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import tempfile
import time
import urllib.parse
import urllib.request


ENDPOINT = "https://api.inaturalist.org/v1/observations"
PER_PAGE = 200
MAX_RESULTS = 10_000
REQUEST_DELAY_SECONDS = 1.1


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def exact_coordinate(value: int) -> str:
    sign = "-" if value < 0 else ""
    absolute = abs(value)
    whole, fractional = divmod(absolute, 10_000_000)
    return f"{sign}{whole}.{fractional:07d}"


def canonical_bytes(value: object) -> bytes:
    return json.dumps(value, separators=(",", ":"), sort_keys=True).encode()


def fetch(url: str) -> bytes:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "a-tiny-civilization/0.1 (https://atinycivilization.com)",
        },
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        if response.status != 200 or response.headers.get_content_type() != "application/json":
            raise RuntimeError(
                f"unexpected iNaturalist response: {response.status} "
                f"{response.headers.get_content_type()}"
            )
        data = response.read()
    if not data:
        raise RuntimeError("iNaturalist returned an empty response")
    return data


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--latitude-e7", type=int, required=True)
    parser.add_argument("--longitude-e7", type=int, required=True)
    parser.add_argument("--radius-kilometers", type=int, default=75)
    parser.add_argument("--output-directory", type=Path, required=True)
    args = parser.parse_args()

    if not -900_000_000 <= args.latitude_e7 <= 900_000_000:
        parser.error("--latitude-e7 is outside WGS84 bounds")
    if not -1_800_000_000 <= args.longitude_e7 <= 1_800_000_000:
        parser.error("--longitude-e7 is outside WGS84 bounds")
    if not 1 <= args.radius_kilometers <= 100:
        parser.error("--radius-kilometers must be between 1 and 100")
    destination = args.output_directory.resolve()
    if destination.exists():
        raise RuntimeError(f"refusing to replace observation evidence: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{destination.name}.", dir=destination.parent))

    base_parameters = {
        "captive": "false",
        "lat": exact_coordinate(args.latitude_e7),
        "license": "cc0,cc-by",
        "lng": exact_coordinate(args.longitude_e7),
        "order": "asc",
        "order_by": "id",
        "per_page": str(PER_PAGE),
        "quality_grade": "research",
        "radius": str(args.radius_kilometers),
        "taxon_id": "1",
    }
    pages: list[dict[str, object]] = []
    expected_total: int | None = None
    try:
        page = 1
        while True:
            parameters = {**base_parameters, "page": str(page)}
            url = f"{ENDPOINT}?{urllib.parse.urlencode(sorted(parameters.items()))}"
            data = fetch(url)
            try:
                payload = json.loads(data)
            except json.JSONDecodeError as error:
                raise RuntimeError(f"page {page} is not JSON: {error}") from error
            total = payload.get("total_results")
            results = payload.get("results")
            if not isinstance(total, int) or total < 0 or not isinstance(results, list):
                raise RuntimeError(f"page {page} has an invalid observation envelope")
            if total > MAX_RESULTS:
                raise RuntimeError(
                    f"query returned {total} records; bounded acquisition limit is {MAX_RESULTS}"
                )
            if len(results) > PER_PAGE:
                raise RuntimeError(f"page {page} exceeds its requested size")
            if expected_total is None:
                expected_total = total
            elif expected_total != total:
                raise RuntimeError("iNaturalist result total changed during pagination")
            filename = f"page-{page:05d}.json"
            path = staging / filename
            with path.open("xb") as output:
                output.write(data)
                output.flush()
                os.fsync(output.fileno())
            pages.append(
                {
                    "byte_length": len(data),
                    "content_hash": digest(data),
                    "page": page,
                    "path": filename,
                    "result_count": len(results),
                }
            )
            required_pages = max(1, math.ceil(total / PER_PAGE))
            if page >= required_pages:
                break
            page += 1
            time.sleep(REQUEST_DELAY_SECONDS)

        manifest = {
            "manifest_schema_version": 1,
            "endpoint": ENDPOINT,
            "query": base_parameters,
            "semantics": {
                "candidate_use": "corroborated-local-presence-not-abundance-or-native-status",
                "commercial_observation_licenses": ["cc0", "cc-by"],
                "wild_filter": "captive=false",
            },
            "total_results": expected_total,
            "pages": pages,
        }
        manifest_data = canonical_bytes(manifest)
        with (staging / "manifest.json").open("xb") as output:
            output.write(manifest_data)
            output.flush()
            os.fsync(output.fileno())
        os.rename(staging, destination)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise

    print(
        json.dumps(
            {
                "manifest_content_hash": digest(manifest_data),
                "page_count": len(pages),
                "status": "retained-source-observations-not-population",
                "total_results": expected_total,
            },
            separators=(",", ":"),
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
