#!/usr/bin/env python3

import json
import pathlib
import subprocess
import tempfile
import unittest


PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[1]
VERIFIER = PROJECT_ROOT / "scripts" / "verify-quality-world-admission.py"
ADMISSION = PROJECT_ROOT / "docs" / "operations" / "QUALITY_WORLD_ADMISSION_RULESET30_2026-08-08.json"
WORLD_ID = "b3ea736d-7a5a-5161-a74b-fa8c4302d333"
GENESIS_DIGEST = "36f92754e0e50c7bfc018c303f57b670f0320ba01452d013a5b9820afb27d4d9"
EVIDENCE_DIGEST = "b31d82abf6fd73c646e755cdfb289130d02cf2ad6ceddbc315a38eea6d23c444"


class QualityWorldAdmissionTests(unittest.TestCase):
    def run_verifier(self, admission: pathlib.Path = ADMISSION):
        return subprocess.run(
            [
                str(VERIFIER),
                "--admission", str(admission),
                "--project-root", str(PROJECT_ROOT),
                "--world-id", WORLD_ID,
                "--expected-ruleset", "30",
                "--genesis-sha256s-sha256", GENESIS_DIGEST,
                "--evidence-sha256s-sha256", EVIDENCE_DIGEST,
            ],
            cwd=PROJECT_ROOT,
            text=True,
            capture_output=True,
        )

    def changed_admission(self, change):
        value = json.loads(ADMISSION.read_text(encoding="utf-8"))
        change(value)
        temporary = tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False)
        with temporary:
            temporary.write(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
        self.addCleanup(pathlib.Path(temporary.name).unlink)
        return pathlib.Path(temporary.name)

    def test_accepts_the_exact_experimental_quality_world(self):
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"status":"experimental-quality-world-admission-passed"', result.stdout)

    def test_rejects_deployment_authorization(self):
        admission = self.changed_admission(
            lambda value: value.__setitem__("public_deployment_authorized", True)
        )
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cannot authorize deployment", result.stderr)

    def test_rejects_an_incomplete_dimension_set(self):
        admission = self.changed_admission(lambda value: value["dimensions"].pop())
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("incomplete dimension set", result.stderr)


if __name__ == "__main__":
    unittest.main()
