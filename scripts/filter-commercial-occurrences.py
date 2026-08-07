#!/usr/bin/env python3
"""Make a commercially reusable occurrence evidence subset.

The input is a tab-separated Darwin Core occurrence export (for example, a
DOI-issued GBIF download or an OBIS export). The script retains only records whose
record-level licence is CC0 or CC BY. It deliberately fails closed on missing,
ambiguous, share-alike, no-derivatives, or non-commercial licences. The output is an
immutable evidence candidate, not a species range or abundance estimate.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import tempfile


ALLOWED = {
    "cc0",
    "cc0-1.0",
    "cc0_1.0",
    "cc by",
    "cc-by",
    "cc-by-4.0",
    "https://creativecommons.org/publicdomain/zero/1.0",
    "https://creativecommons.org/licenses/by/4.0",
}


def is_allowed(value: str | None) -> bool:
    return (value or "").strip().lower().rstrip("/") in ALLOWED


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def filter_records(source: Path, destination: Path, license_column: str) -> dict[str, int | str]:
    with source.open("r", encoding="utf-8", newline="") as input_stream:
        reader = csv.DictReader(input_stream, delimiter="\t")
        if reader.fieldnames is None or license_column not in reader.fieldnames:
            raise RuntimeError(f"input must contain a {license_column!r} column")
        descriptor, partial_name = tempfile.mkstemp(
            prefix=f".{destination.name}.", suffix=".partial", dir=destination.parent
        )
        os.close(descriptor)
        partial = Path(partial_name)
        retained = rejected = 0
        try:
            with partial.open("w", encoding="utf-8", newline="") as output_stream:
                writer = csv.DictWriter(
                    output_stream, fieldnames=reader.fieldnames, delimiter="\t", lineterminator="\n"
                )
                writer.writeheader()
                for row in reader:
                    if is_allowed(row.get(license_column)):
                        writer.writerow(row)
                        retained += 1
                    else:
                        rejected += 1
                output_stream.flush()
                os.fsync(output_stream.fileno())
            if retained == 0:
                raise RuntimeError("refusing to create an occurrence evidence set with zero admissible records")
            try:
                os.link(partial, destination)
            except FileExistsError:
                if sha256(partial) != sha256(destination):
                    raise RuntimeError(f"refusing to overwrite different existing evidence: {destination}")
            return {
                "input_records": retained + rejected,
                "retained_records": retained,
                "rejected_records": rejected,
                "content_hash": sha256(destination),
                "byte_length": destination.stat().st_size,
            }
        finally:
            partial.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--license-column", default="license")
    parser.add_argument("--source-name", required=True)
    parser.add_argument("--source-doi", required=True)
    parser.add_argument("--source-version", required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    result = filter_records(args.input, args.output, args.license_column)
    print(json.dumps({
        "schema_version": 1,
        "source_name": args.source_name,
        "source_doi": args.source_doi,
        "source_version": args.source_version,
        "admission_policy": "record-license-is-CC0-or-CC-BY-only",
        "output": str(args.output),
        **result,
    }, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
