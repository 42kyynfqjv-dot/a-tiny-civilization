#!/usr/bin/env python3
"""Check that the pinned iNaturalist range-map acquisition contract is complete."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    result = subprocess.run(
        ["python3", str(ROOT / "scripts/acquire-inaturalist-open-range-maps.py")],
        text=True, capture_output=True, check=True,
    )
    contract = json.loads(result.stdout)
    assert contract["release"] == "2.20"
    assert contract["license_expression"] == "CC-BY-4.0"
    assert contract["range_kind"] == "modeled_presence_candidate"
    packages = contract["animal_packages"]
    assert len(packages) == 17
    paths = [item["artifact_path"] for item in packages]
    assert paths == sorted(paths)
    assert any(path.endswith("mammalia.gpkg") for path in paths)
    assert any(path.endswith("aves_2.gpkg") for path in paths)
    assert len(contract["metadata_artifacts"]) == 3
    assert any(item["artifact_path"].endswith("license-cc-by-4.0.html") for item in contract["metadata_artifacts"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
