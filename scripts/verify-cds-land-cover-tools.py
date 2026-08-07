#!/usr/bin/env python3
"""Verify the land-cover acquisition, provenance, and source-snapshot boundaries."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PROVENANCE = ROOT / "scripts" / "acquire-cds-land-cover-provenance.py"
MANIFEST = ROOT / "scripts" / "create-cds-land-cover-source-snapshot.py"
PREFIX = "copernicus-land-cover-2022"
DATA_NAME = "copernicus-satellite-land-cover-v2.1.1-2022.zip"
EXPECTED_MEMBER = "C3S-LC-L4-LCCS-Map-300m-P1Y-2022-v2.1.1.nc"


def acquisition_module():
    path = Path(__file__).with_name("acquire-cds-land-cover.py")
    spec = importlib.util.spec_from_file_location("cds_land_cover_acquisition", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load land-cover acquisition contract")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_archive(path: Path, members: dict[str, bytes]) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in members.items():
            archive.writestr(name, content)


def expect_rejected(module, path: Path) -> None:
    try:
        module.validate_response(path)
    except ValueError:
        return
    raise AssertionError(f"unsafe synthetic response was accepted: {path.name}")


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
    module = acquisition_module()
    assert module.request() == {
        "variable": "all",
        "version": ["v2_1_1"],
        "year": ["2022"],
    }
    with tempfile.TemporaryDirectory(prefix="atc-land-cover-") as directory:
        root = Path(directory)
        valid = root / "valid.zip"
        write_archive(valid, {EXPECTED_MEMBER: b"CDF\x01evidence"})
        assert module.validate_response(valid) == (EXPECTED_MEMBER,)

        changed_schema = root / "changed-schema.zip"
        write_archive(changed_schema, {"global-land-cover-2022.nc": b"CDF\x01evidence"})
        expect_rejected(module, changed_schema)

        unsafe = root / "unsafe.zip"
        write_archive(unsafe, {"../escaped.nc": b"not allowed"})
        expect_rejected(module, unsafe)

        wrong_type = root / "wrong-type.zip"
        write_archive(wrong_type, {"readme.txt": b"not NetCDF"})
        expect_rejected(module, wrong_type)

        empty = root / "empty-member.zip"
        write_archive(empty, {"empty.nc": b""})
        expect_rejected(module, empty)

        partial = root / "response.partial"
        partial.write_bytes(b"immutable evidence")
        target = root / "response.zip"
        module.publish_without_replacement(partial, target)
        assert target.read_bytes() == b"immutable evidence"
        conflicting = root / "conflicting.partial"
        conflicting.write_bytes(b"must not replace")
        try:
            module.publish_without_replacement(conflicting, target)
        except FileExistsError:
            pass
        else:
            raise AssertionError("land-cover publication replaced existing evidence")
        assert target.read_bytes() == b"immutable evidence"

        dry_run = invoke(PROVENANCE, "--output-directory", str(root), "--dry-run")
        assert dry_run.returncode == 0, dry_run.stderr
        lines = dry_run.stdout.splitlines()
        assert len(lines) == 7
        assert lines[0] == (
            "provenance/catalogue-metadata.json\t"
            "https://cds.climate.copernicus.eu/api/catalogue/v1/collections/"
            "satellite-land-cover"
        )
        assert lines[-1] == (
            "provenance/version-evidence.html\t"
            "https://doi.org/10.24381/cds.006f2c9a"
        )
        assert len(set(lines)) == len(lines)

        source_root = root / "source-cache"
        evidence = source_root / PREFIX
        evidence.mkdir(parents=True)
        data = evidence / DATA_NAME
        write_archive(data, {EXPECTED_MEMBER: b"CDF\x01synthetic land-cover evidence"})
        provenance = evidence / "provenance"
        provenance.mkdir()
        for name, contents in {
            "catalogue-metadata.json": b'{"id":"satellite-land-cover"}',
            "dataset-documentation.html": b"documentation",
            "dataset-download-and-licence.html": b"licence selector",
            "license-cc-by.html": b"cc-by",
            "license-esa-cci-land-cover.pdf": b"esa licence",
            "license-vito-proba-v.pdf": b"vito licence",
            "version-evidence.html": b"version",
        }.items():
            (provenance / name).write_bytes(contents)

        output = root / "land-cover-source-snapshot.json"
        created = invoke(
            MANIFEST,
            "--artifact-root",
            str(source_root),
            "--output",
            str(output),
            "--retrieved-on",
            "2026-08-07",
        )
        assert created.returncode == 0, created.stderr
        raw = output.read_bytes()
        assert raw.endswith(b"\n")
        snapshot = json.loads(raw)
        assert snapshot["snapshot_id"] == "copernicus-satellite-land-cover-v2-1-1-2022"
        assert snapshot["dataset_version"] == EXPECTED_MEMBER.removesuffix(".nc")
        assert snapshot["license_expression"] == (
            "CC-BY-4.0 AND LicenseRef-ESA-CCI-Land-Cover-Rev-1 "
            "AND LicenseRef-VITO-PROBA-V-Rev-1"
        )
        assert len(snapshot["artifacts"]) == 8
        paths = [artifact["artifact_path"] for artifact in snapshot["artifacts"]]
        assert paths == sorted(paths)
        data_record = next(
            artifact for artifact in snapshot["artifacts"] if artifact["role"] == "data"
        )
        assert data_record["content_hash"] == hashlib.sha256(data.read_bytes()).hexdigest()
        assert data_record["byte_length"] == str(data.stat().st_size)

        validated = subprocess.run(
            [
                "cargo",
                "run",
                "--locked",
                "-p",
                "civilization-data",
                "--",
                "source",
                "validate",
                str(output),
                "--artifact-root",
                str(source_root),
            ],
            check=False,
            capture_output=True,
            text=True,
            cwd=ROOT,
            env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"),
        )
        assert validated.returncode == 0, validated.stderr

        duplicate = invoke(
            MANIFEST,
            "--artifact-root",
            str(source_root),
            "--output",
            str(output),
            "--retrieved-on",
            "2026-08-07",
        )
        assert duplicate.returncode != 0
        assert "refusing to replace existing manifest" in duplicate.stderr
    print("CDS land-cover acquisition and provenance tools are stable.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
