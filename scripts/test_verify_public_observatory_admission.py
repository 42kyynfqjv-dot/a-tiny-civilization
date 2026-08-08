#!/usr/bin/env python3
from __future__ import annotations

import json
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
VERIFIER = ROOT / "scripts/verify-public-observatory-admission.py"
ADMISSION = ROOT / "docs/operations/PUBLIC_OBSERVATORY_ADMISSION_2026-08-08.json"
WORLD_ID = "b3ea736d-7a5a-5161-a74b-fa8c4302d333"


class PublicObservatoryAdmissionTests(unittest.TestCase):
    def run_verifier(self, admission: pathlib.Path = ADMISSION):
        return subprocess.run(
            [str(VERIFIER), "--admission", str(admission), "--world-id", WORLD_ID],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def changed_admission(self, change):
        value = json.loads(ADMISSION.read_text())
        change(value)
        temporary = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
        with temporary:
            temporary.write(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
        self.addCleanup(pathlib.Path(temporary.name).unlink)
        return pathlib.Path(temporary.name)

    def test_exact_admission_passes_without_authorizing_deployment(self):
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"public_deployment_authorized":false', result.stdout)
        self.assertIn('"status":"public-observatory-admission-passed"', result.stdout)

    def test_changed_quality_admission_digest_fails(self):
        admission = self.changed_admission(
            lambda value: value.__setitem__("quality_world_admission_sha256", "0" * 64)
        )
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not bound to the current quality-world admission", result.stderr)

    def test_deployment_authorization_cannot_be_smuggled_into_review(self):
        admission = self.changed_admission(
            lambda value: value.__setitem__("public_deployment_authorized", True)
        )
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot authorize deployment", result.stderr)

    def test_incomplete_routes_fail(self):
        admission = self.changed_admission(lambda value: value["routes"].pop())
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("route set is incomplete", result.stderr)

    def test_evidence_outside_reviewed_tree_fails(self):
        admission = self.changed_admission(
            lambda value: value["dimensions"][0]["evidence"].append("README.md")
        )
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("evidence must be a nonempty sorted unique list", result.stderr)


if __name__ == "__main__":
    unittest.main()
