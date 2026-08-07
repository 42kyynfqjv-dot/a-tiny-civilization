#!/usr/bin/env python3
"""Verify the committed DE441 inventory and account-free acquisition contract."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "acquire-jpl-de441.py"
INVENTORY = ROOT / "data" / "source-inspections" / "jpl-de441-inventory.json"
EXPECTED_INVENTORY_SHA256 = "a253715e23e547d07f2e7be066a3fa437974b54f1c8a78f876f144ff8be22742"


def main() -> int:
    encoded = INVENTORY.read_bytes()
    if hashlib.sha256(encoded).hexdigest() != EXPECTED_INVENTORY_SHA256:
        raise RuntimeError("committed DE441 inventory fingerprint changed")
    inventory = json.loads(encoded)
    canonical = (
        json.dumps(inventory, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode()
    if encoded != canonical:
        raise RuntimeError("DE441 inventory is not canonical compact JSON")
    if inventory["artifact_count"] != 6 or len(inventory["artifacts"]) != 6:
        raise RuntimeError("DE441 inventory is incomplete")
    if inventory["byte_length"] != 3_308_164_805:
        raise RuntimeError("DE441 inventory byte total changed")
    if inventory["license_expression"] != "LicenseRef-NAIF-SPICE-Rules":
        raise RuntimeError("DE441 license contract changed")
    license_items = [
        item for item in inventory["artifacts"] if item["role"] == "license_evidence"
    ]
    if len(license_items) != 1 or license_items[0]["content_hash"] != (
        "ae85f851646e7c4f0a762db852907bc090a2ab50c815eb2a6cd8639e96b7e047"
    ):
        raise RuntimeError("DE441 inventory lacks the pinned NAIF rules evidence")
    data_items = [item for item in inventory["artifacts"] if item["role"] == "data"]
    if [item["byte_length"] for item in data_items] != [1_651_119_104, 1_656_830_976]:
        raise RuntimeError("DE441 planetary kernel lengths changed")

    preview = json.loads(
        subprocess.check_output([sys.executable, str(SCRIPT)], text=True)
    )
    if preview["release"] != "de441" or len(preview["artifacts"]) != 6:
        raise RuntimeError("DE441 acquisition preview differs from its inventory contract")
    if not any(item["role"] == "license_evidence" for item in preview["artifacts"]):
        raise RuntimeError("DE441 acquisition preview omits license evidence")
    print("JPL DE441 acquisition and inventory tools are stable.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
