#!/usr/bin/env python3
"""Acquire the first commercially compatible, source-backed fauna trait evidence.

The retained files cover measured physiology (AnimalTraits), amniote life history,
and bird/mammal foraging ecology (EltonTraits).  Acquisition is immutable: an
existing nonempty artifact is retained and hashed, never replaced.  These bytes are
evidence only; inferred or taxonomically filled values are not admitted as observed
species facts by this script.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import csv
import hashlib
import json
import os
from pathlib import Path
import tempfile
import urllib.request


ANIMALTRAITS_PREFIX = "https://zenodo.org/api/records/6468938/files"
AMNIOTE_PREFIX = "https://www.esapubs.org/archive/ecol/E096/269"
ELTON_PREFIX = "https://esapubs.org/archive/ecol/E095/178"

ARTIFACTS = (
    {
        "dataset": "animaltraits-v1.0.7",
        "role": "data",
        "artifact_path": "animaltraits-v1.0.7/observations.csv",
        "download_url": f"{ANIMALTRAITS_PREFIX}/observations.csv/content",
        "upstream_md5": "79ccdf4e022800e76e5444f9fbfecdc1",
    },
    {
        "dataset": "animaltraits-v1.0.7",
        "role": "schema",
        "artifact_path": "animaltraits-v1.0.7/column-documentation.csv",
        "download_url": f"{ANIMALTRAITS_PREFIX}/column-documentation.csv/content",
        "upstream_md5": "0bb24bd2c9bebf77119af385bf602a46",
    },
    {
        "dataset": "animaltraits-v1.0.7",
        "role": "license_evidence",
        "artifact_path": "animaltraits-v1.0.7/LICENSE",
        "download_url": f"{ANIMALTRAITS_PREFIX}/LICENSE/content",
        "upstream_md5": "65d3616852dbf7b1a6d4b53b00626032",
    },
    {
        "dataset": "amniote-life-history-2015-08",
        "role": "data",
        "artifact_path": "amniote-life-history-2015-08/Amniote_Database_Aug_2015.csv",
        "download_url": f"{AMNIOTE_PREFIX}/Data_Files/Amniote_Database_Aug_2015.csv",
        "upstream_md5": "10e7c14395d95ed7f53258f96fadb4f7",
    },
    {
        "dataset": "amniote-life-history-2015-08",
        "role": "references",
        "artifact_path": "amniote-life-history-2015-08/Amniote_Database_References_Aug_2015.csv",
        "download_url": f"{AMNIOTE_PREFIX}/Data_Files/Amniote_Database_References_Aug_2015.csv",
        "upstream_md5": "58ce0c047162154dd752cfdd16c1dd26",
    },
    {
        "dataset": "amniote-life-history-2015-08",
        "role": "raw_observations",
        "artifact_path": "amniote-life-history-2015-08/Amniote_Sparse_Table_Aug_2015.csv",
        "download_url": f"{AMNIOTE_PREFIX}/Data_Files/Amniote_Sparse_Table_Aug_2015.csv",
        "upstream_md5": "03253b795e400f629b823e730cd7f75a",
    },
    {
        "dataset": "amniote-life-history-2015-08",
        "role": "uncertainty",
        "artifact_path": "amniote-life-history-2015-08/Amniote_Range_Count_Aug_2015.csv",
        "download_url": f"{AMNIOTE_PREFIX}/Data_Files/Amniote_Range_Count_Aug_2015.csv",
        "upstream_md5": "8095602c7d10befc52feaf22db8e6c76",
    },
    {
        "dataset": "amniote-life-history-2015-08",
        "role": "schema",
        "artifact_path": "amniote-life-history-2015-08/Supplemental_Table_9_Dataset_Field_Information.csv",
        "download_url": f"{AMNIOTE_PREFIX}/Supplemental_Materials/Supplemental_Table_9_Dataset_Field_Information.csv",
    },
    {
        "dataset": "eltontraits-v1.0",
        "role": "bird_data",
        "artifact_path": "eltontraits-v1.0/BirdFuncDat.txt",
        "download_url": f"{ELTON_PREFIX}/BirdFuncDat.txt",
        "upstream_md5": "d6197b2cd90ca3ece0a7393abbf8b7fc",
    },
    {
        "dataset": "eltontraits-v1.0",
        "role": "mammal_data",
        "artifact_path": "eltontraits-v1.0/MamFuncDat.txt",
        "download_url": f"{ELTON_PREFIX}/MamFuncDat.txt",
        "upstream_md5": "59c3eee29d3ed0a33a002975a5a8cc75",
    },
    {
        "dataset": "eltontraits-v1.0",
        "role": "bird_references",
        "artifact_path": "eltontraits-v1.0/BirdFuncDatSources.txt",
        "download_url": f"{ELTON_PREFIX}/BirdFuncDatSources.txt",
        "upstream_md5": "e4793ab8baa813bba7d168a3d2d6b6a7",
    },
    {
        "dataset": "eltontraits-v1.0",
        "role": "mammal_references",
        "artifact_path": "eltontraits-v1.0/MamFuncDatSources.txt",
        "download_url": f"{ELTON_PREFIX}/MamFuncDatSources.txt",
        "upstream_md5": "0c67fa8bf6b6ccb7a38fc5229f84508b",
    },
)


def file_digests(path: Path) -> tuple[str, str, int]:
    sha256 = hashlib.sha256()
    md5 = hashlib.md5(usedforsecurity=False)
    byte_length = 0
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            sha256.update(chunk)
            md5.update(chunk)
            byte_length += len(chunk)
    return sha256.hexdigest(), md5.hexdigest(), byte_length


def verify(path: Path, item: dict[str, str]) -> dict[str, str | int]:
    sha256, md5, byte_length = file_digests(path)
    if byte_length == 0:
        raise RuntimeError(f"refusing zero-byte artifact: {path}")
    expected_md5 = item.get("upstream_md5")
    if expected_md5 is not None and md5 != expected_md5:
        raise RuntimeError(
            f"upstream MD5 mismatch for {path}: expected {expected_md5}, observed {md5}"
        )
    return {
        **item,
        "byte_length": byte_length,
        "content_hash": sha256,
        "observed_md5": md5,
    }


def acquire(root: Path, item: dict[str, str]) -> dict[str, str | int]:
    destination = root / item["artifact_path"]
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        return {**verify(destination, item), "status": "retained"}

    descriptor, partial_name = tempfile.mkstemp(
        prefix=f".{destination.name}.", suffix=".partial", dir=destination.parent
    )
    os.close(descriptor)
    partial = Path(partial_name)
    try:
        request = urllib.request.Request(
            item["download_url"], headers={"User-Agent": "a-tiny-civilization/0.1"}
        )
        with urllib.request.urlopen(request, timeout=120) as response, partial.open("wb") as out:
            while chunk := response.read(1024 * 1024):
                out.write(chunk)
            out.flush()
            os.fsync(out.fileno())
        result = verify(partial, item)
        try:
            os.link(partial, destination)
        except FileExistsError:
            retained = verify(destination, item)
            if retained["content_hash"] != result["content_hash"]:
                raise RuntimeError(f"concurrent artifact differs: {destination}")
            return {**retained, "status": "retained"}
        return {**result, "status": "downloaded"}
    finally:
        partial.unlink(missing_ok=True)


def stable_result(item: dict[str, str | int]) -> dict[str, str | int]:
    return {key: value for key, value in item.items() if key != "status"}


def csv_contract(
    path: Path,
    *,
    delimiter: str,
    expected_records: int,
    required_columns: tuple[str, ...],
    encoding: str = "utf-8",
) -> dict[str, int]:
    with path.open("r", encoding=encoding, errors="strict", newline="") as stream:
        rows = csv.reader(stream, delimiter=delimiter)
        try:
            header = next(rows)
        except StopIteration as error:
            raise RuntimeError(f"empty tabular fauna artifact: {path}") from error
        trailing_empty_columns = 0
        while header and header[-1] == "":
            header.pop()
            trailing_empty_columns += 1
        if len(header) != len(set(header)):
            raise RuntimeError(f"duplicate columns in fauna artifact: {path}")
        missing = [column for column in required_columns if column not in header]
        if missing:
            raise RuntimeError(f"missing columns in {path}: {missing}")
        record_count = 0
        for line_number, row in enumerate(rows, start=2):
            if not row or all(field == "" for field in row):
                continue
            if trailing_empty_columns:
                if len(row) < trailing_empty_columns or any(row[-trailing_empty_columns:]):
                    raise RuntimeError(
                        f"row {line_number} in {path} uses undocumented trailing columns"
                    )
                row = row[:-trailing_empty_columns]
            if len(row) != len(header):
                raise RuntimeError(
                    f"row {line_number} in {path} has {len(row)} fields; expected {len(header)}"
                )
            record_count += 1
    if record_count != expected_records:
        raise RuntimeError(
            f"{path} has {record_count} records; expected {expected_records}"
        )
    return {"records": record_count, "columns": len(header)}


def inspect_retained_content(root: Path) -> dict[str, object]:
    animal_root = root / "animaltraits-v1.0.7"
    amniote_root = root / "amniote-life-history-2015-08"
    elton_root = root / "eltontraits-v1.0"
    license_text = (animal_root / "LICENSE").read_text(encoding="utf-8")
    if "CC0 1.0 Universal" not in license_text:
        raise RuntimeError("AnimalTraits retained license is not the pinned CC0 1.0 text")

    return {
        "animaltraits": csv_contract(
            animal_root / "observations.csv",
            delimiter=",",
            expected_records=3580,
            required_columns=(
                "species",
                "fullReference",
                "body mass",
                "metabolic rate",
                "brain size",
            ),
        ),
        "animaltraits_columns": csv_contract(
            animal_root / "column-documentation.csv",
            delimiter=",",
            expected_records=43,
            required_columns=("Column", "Description", "Defined by"),
            encoding="windows-1252",
        ),
        "amniote": csv_contract(
            amniote_root / "Amniote_Database_Aug_2015.csv",
            delimiter=",",
            expected_records=21322,
            required_columns=(
                "class",
                "genus",
                "species",
                "female_maturity_d",
                "litter_or_clutch_size_n",
                "adult_body_mass_g",
                "maximum_longevity_y",
            ),
        ),
        "amniote_references": csv_contract(
            amniote_root / "Amniote_Database_References_Aug_2015.csv",
            delimiter=",",
            expected_records=21322,
            required_columns=("class", "genus", "species"),
        ),
        "amniote_sparse": csv_contract(
            amniote_root / "Amniote_Sparse_Table_Aug_2015.csv",
            delimiter=",",
            expected_records=139827,
            required_columns=("class", "genus", "species"),
        ),
        "amniote_uncertainty": csv_contract(
            amniote_root / "Amniote_Range_Count_Aug_2015.csv",
            delimiter=",",
            expected_records=21322,
            required_columns=("classx", "genus", "species"),
        ),
        "elton_birds": csv_contract(
            elton_root / "BirdFuncDat.txt",
            delimiter="\t",
            expected_records=9995,
            required_columns=(
                "Scientific",
                "Diet-Certainty",
                "ForStrat-SpecLevel",
                "BodyMass-SpecLevel",
            ),
            encoding="windows-1252",
        ),
        "elton_mammals": csv_contract(
            elton_root / "MamFuncDat.txt",
            delimiter="\t",
            expected_records=5400,
            required_columns=(
                "Scientific",
                "Diet-Certainty",
                "Activity-Certainty",
                "BodyMass-SpecLevel",
            ),
            encoding="windows-1252",
        ),
        "elton_bird_references": csv_contract(
            elton_root / "BirdFuncDatSources.txt",
            delimiter="\t",
            expected_records=58,
            required_columns=("Ref_ID", "Full Reference"),
            encoding="windows-1252",
        ),
        "elton_mammal_references": csv_contract(
            elton_root / "MamFuncDatSources.txt",
            delimiter="\t",
            expected_records=176,
            required_columns=("Ref_ID", "Full Reference"),
            encoding="windows-1252",
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-directory", type=Path, default=Path("data/source-cache"))
    parser.add_argument("--download", action="store_true")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--report-output", type=Path)
    args = parser.parse_args()

    if not 1 <= args.workers <= 16:
        raise SystemExit("--workers must be between 1 and 16")
    if not args.download:
        print(json.dumps({"release_set": "fauna-traits-v1", "artifacts": ARTIFACTS}))
        return 0

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        results = list(executor.map(lambda item: acquire(args.output_directory, item), ARTIFACTS))
    stable = [stable_result(item) for item in results]
    report = {
        "report_schema_version": 1,
        "release_set": "fauna-traits-v1",
        "artifact_count": len(stable),
        "byte_length": sum(int(item["byte_length"]) for item in stable),
        "content_summary": inspect_retained_content(args.output_directory),
        "artifacts": stable,
    }
    encoded = json.dumps(report, separators=(",", ":"), ensure_ascii=False).encode() + b"\n"
    if args.report_output is not None:
        args.report_output.parent.mkdir(parents=True, exist_ok=True)
        try:
            descriptor = os.open(args.report_output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        except FileExistsError:
            if args.report_output.read_bytes() != encoded:
                raise RuntimeError(f"refusing to replace differing report: {args.report_output}")
        else:
            with os.fdopen(descriptor, "wb") as out:
                out.write(encoded)
                out.flush()
                os.fsync(out.fileno())
        print(
            json.dumps(
                {
                    "release_set": report["release_set"],
                    "artifact_count": report["artifact_count"],
                    "byte_length": report["byte_length"],
                    "report_output": str(args.report_output),
                }
            )
        )
    else:
        print(json.dumps({**report, "artifacts": results}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
