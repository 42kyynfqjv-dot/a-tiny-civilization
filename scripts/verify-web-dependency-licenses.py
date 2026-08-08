#!/usr/bin/env python3
"""Fail closed on unknown web dependency licenses or an unreviewed obligation set."""

from __future__ import annotations

import json
from pathlib import Path
import sys


PERMISSIVE = {
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CC0-1.0",
    "ISC",
    "MIT",
    "MIT OR Apache-2.0",
}
REVIEWED_COMMERCIAL = {
    "BlueOak-1.0.0",
    "CC-BY-4.0",
    "LGPL-3.0-or-later",
    "MPL-2.0",
    "Python-2.0",
}


def main() -> None:
    project_root = Path(__file__).resolve().parent.parent
    review = json.loads(
        (project_root / "docs/security/WEB_LICENSE_REVIEW.json").read_text(encoding="utf-8")
    )
    if review.get("schema_version") != 1:
        raise SystemExit("unsupported web license-review schema")
    expected = {
        (entry["package"], entry["license"])
        for entry in review.get("reviewed_commercial_licenses", [])
    }

    packages = json.load(sys.stdin)
    observed: set[tuple[str, str]] = set()
    failures: list[str] = []
    for package in packages:
        name = package.get("name")
        version = package.get("version")
        license_expression = package.get("license")
        identity = f"{name}@{version}"
        if not isinstance(name, str) or not isinstance(version, str):
            failures.append("dependency has no stable package identity")
            continue
        if not isinstance(license_expression, str):
            failures.append(f"{identity} has no machine-readable license")
            continue
        if license_expression in REVIEWED_COMMERCIAL:
            observed.add((identity, license_expression))
        elif license_expression not in PERMISSIVE:
            failures.append(f"{identity} uses unapproved license {license_expression!r}")
    if failures:
        raise SystemExit("web dependency license gate failed:\n  " + "\n  ".join(sorted(failures)))
    if observed != expected:
        missing = sorted(expected - observed)
        unreviewed = sorted(observed - expected)
        raise SystemExit(
            "web commercial-license review drifted: "
            f"missing={missing!r}, unreviewed={unreviewed!r}"
        )
    print(f"Web dependency licenses passed; {len(observed)} reviewed obligations remain exact.")


if __name__ == "__main__":
    main()
