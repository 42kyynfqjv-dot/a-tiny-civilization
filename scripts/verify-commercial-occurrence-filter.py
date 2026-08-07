#!/usr/bin/env python3
"""Regression checks for fail-closed commercial occurrence filtering."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
TOOL = ROOT / "scripts/filter-commercial-occurrences.py"


def invoke(source: Path, output: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "python3", str(TOOL), "--input", str(source), "--output", str(output),
            "--source-name", "fixture", "--source-doi", "https://doi.org/10.1/fixture",
            "--source-version", "v1",
        ], text=True, capture_output=True, check=False
    )


def main() -> int:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        source = root / "occurrence.txt"
        source.write_text(
            "key\tscientificName\tlicense\n"
            "one\tBison bison\tCC0_1.0\n"
            "two\tCanis lupus\tCC-BY-4.0\n"
            "three\tFelis silvestris\tCC-BY-NC-4.0\n"
            "four\tUrsus arctos\t\n", encoding="utf-8"
        )
        output = root / "commercial.txt"
        result = invoke(source, output)
        if result.returncode != 0:
            raise RuntimeError(result.stderr)
        metadata = json.loads(result.stdout)
        assert metadata["retained_records"] == 2
        assert metadata["rejected_records"] == 2
        assert output.read_text(encoding="utf-8").splitlines() == [
            "key\tscientificName\tlicense", "one\tBison bison\tCC0_1.0",
            "two\tCanis lupus\tCC-BY-4.0",
        ]
        source.write_text("key\tlicense\none\tCC-BY-NC-4.0\n", encoding="utf-8")
        empty = invoke(source, root / "empty.txt")
        assert empty.returncode != 0
        assert "zero admissible records" in empty.stderr
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
