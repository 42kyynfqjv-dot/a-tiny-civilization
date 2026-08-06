#!/usr/bin/env python3
"""Regression checks for the immutable ERA5 provenance helpers."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PROVENANCE = ROOT / "scripts" / "acquire-era5-provenance.py"
MANIFEST = ROOT / "scripts" / "create-era5-source-snapshot.py"
MIGRATION = ROOT / "scripts" / "migrate-era5-archive-filenames.py"
PREFIX = "era5-monthly-1981-2010"


def invoke(script: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    environment = dict(os.environ)
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    return subprocess.run(
        [sys.executable, str(script), *arguments],
        check=False,
        capture_output=True,
        text=True,
        env=environment,
    )


def main() -> int:
    dry_run = invoke(PROVENANCE, "--output-directory", str(ROOT), "--dry-run")
    assert dry_run.returncode == 0, dry_run.stderr
    assert dry_run.stdout.splitlines() == [
        "provenance/dataset-documentation.html\thttps://cds.climate.copernicus.eu/datasets/reanalysis-era5-single-levels-monthly-means?tab=documentation",
        "provenance/licence-evidence.html\thttps://cds.climate.copernicus.eu/datasets/reanalysis-era5-single-levels-monthly-means?tab=download",
        "provenance/version-evidence.html\thttps://doi.org/10.24381/cds.f17050d7",
    ]

    with tempfile.TemporaryDirectory(prefix="atiny-era5-provenance-") as temporary:
        root = Path(temporary) / "source-cache"
        evidence = root / PREFIX
        evidence.mkdir(parents=True)
        for year in range(1981, 2011):
            with zipfile.ZipFile(
                evidence / f"era5-monthly-single-levels-1981-2010-{year}.nc", "w"
            ) as archive:
                archive.writestr(f"data-{year}.nc", f"netcdf-{year}".encode("ascii"))
        migrated = invoke(MIGRATION, "--output-directory", str(evidence))
        assert migrated.returncode == 0, migrated.stderr
        assert not list(evidence.glob("*.nc"))
        assert len(list(evidence.glob("*.zip"))) == 30
        provenance = evidence / "provenance"
        provenance.mkdir()
        for name, contents in {
            "dataset-documentation.html": b"documentation",
            "licence-evidence.html": b"licence",
            "version-evidence.html": b"version",
        }.items():
            (provenance / name).write_bytes(contents)

        output = Path(temporary) / "era5.json"
        created = invoke(
            MANIFEST,
            "--artifact-root",
            str(root),
            "--output",
            str(output),
            "--retrieved-on",
            "2026-08-06",
        )
        assert created.returncode == 0, created.stderr
        raw = output.read_bytes()
        assert raw.endswith(b"\n")
        manifest = json.loads(raw)
        assert manifest["snapshot_id"] == "era5-single-levels-monthly-means-1981-2010"
        assert manifest["license_expression"] == "CC-BY-4.0"
        assert len(manifest["artifacts"]) == 33
        paths = [artifact["artifact_path"] for artifact in manifest["artifacts"]]
        assert paths == sorted(paths)
        first = manifest["artifacts"][0]
        assert first["artifact_path"].endswith("1981.zip")
        assert first["media_type"] == "application/zip"
        assert first["content_hash"] == hashlib.sha256(
            (evidence / "era5-monthly-single-levels-1981-2010-1981.zip").read_bytes()
        ).hexdigest()
        assert int(first["byte_length"]) > 0

        duplicate = invoke(
            MANIFEST,
            "--artifact-root",
            str(root),
            "--output",
            str(output),
            "--retrieved-on",
            "2026-08-06",
        )
        assert duplicate.returncode != 0
        assert "refusing to replace existing manifest" in duplicate.stderr

    print("ERA5 provenance tools are deterministic and refuse replacement.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
