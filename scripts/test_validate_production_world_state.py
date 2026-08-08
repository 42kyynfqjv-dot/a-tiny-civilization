#!/usr/bin/env python3

from __future__ import annotations

import json
import pathlib
import subprocess
import unittest

SCRIPT = pathlib.Path(__file__).with_name("validate-production-world-state.py")
WORLD_ID = "b3ea736d-7a5a-5161-a74b-fa8c4302d333"


def run(mode: str, rows: str, world_id: str = WORLD_ID, ruleset: str = "30") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(SCRIPT),
            "--mode",
            mode,
            "--expected-world-id",
            world_id,
            "--expected-ruleset",
            ruleset,
        ],
        input=rows,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


class ProductionWorldStateTests(unittest.TestCase):
    def test_allows_only_an_empty_preparation_database(self) -> None:
        result = run("allow-empty", "")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "empty-ready-for-qualified-activation")
        required = run("require-running", "")
        self.assertNotEqual(required.returncode, 0)
        self.assertIn("no activated world", required.stderr)

    def test_accepts_the_exact_running_qualified_world(self) -> None:
        row = f"{WORLD_ID}|30|0|1|running\n"
        for mode in ("allow-empty", "require-running"):
            result = run(mode, row)
            self.assertEqual(result.returncode, 0, result.stderr)
            document = json.loads(result.stdout)
            self.assertEqual(document["status"], "qualified-running-world")
            self.assertEqual(document["current_sequence"], 1)

    def test_rejects_identity_ruleset_status_and_cardinality_drift(self) -> None:
        invalid = (
            f"00000000-0000-0000-0000-000000000001|30|0|1|running\n",
            f"{WORLD_ID}|29|0|1|running\n",
            f"{WORLD_ID}|30|0|1|archived\n",
            f"{WORLD_ID}|30|0|0|running\n",
            f"{WORLD_ID}|30|00|1|running\n",
            f"{WORLD_ID}|30|0|1|running\n{WORLD_ID}|30|1|2|running\n",
            f"{WORLD_ID}|30|0|1|running",
        )
        for rows in invalid:
            with self.subTest(rows=rows):
                self.assertNotEqual(run("require-running", rows).returncode, 0)


if __name__ == "__main__":
    unittest.main()
