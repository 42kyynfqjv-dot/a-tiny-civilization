#!/usr/bin/env python3

import hashlib
import json
import pathlib
import subprocess
import tempfile
import unittest


PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[1]
VERIFIER = PROJECT_ROOT / "scripts" / "verify-launch-candidate-evidence.py"
WORLD_ID = "b3ea736d-7a5a-5161-a74b-fa8c4302d333"


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class LaunchCandidateEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.temporary.name)
        self.genesis = root / "genesis-source"
        self.evidence = root / "evidence"
        (self.evidence / "genesis").mkdir(parents=True)
        self.genesis.mkdir()
        genesis_manifest = "0" * 64 + "  /source/not-used-by-this-verifier.json\n"
        (self.genesis / "SHA256SUMS").write_text(genesis_manifest, encoding="utf-8")
        (self.evidence / "genesis" / "SHA256SUMS").write_text(
            genesis_manifest, encoding="utf-8"
        )
        source_commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=PROJECT_ROOT, text=True
        ).strip()
        self.write_json(
            "evidence.json",
            {
                "contains_canonical_event_payloads": False,
                "genesis_sha256s_sha256": sha(self.genesis / "SHA256SUMS"),
                "source_commit": source_commit,
                "world_id": WORLD_ID,
            },
        )
        self.write_json(
            "qualification-status.json",
            {
                "world_id": WORLD_ID,
                "passed": True,
                "replay_verified": True,
                "world": {
                    "status": "running",
                    "ruleset_version": 30,
                    "current_tick": 1000,
                    "current_sequence": 1018,
                },
                "checks": {"one": True, "two": True},
                "projections": {"required": 5, "current": 5},
                "memory": {"total": 1, "pending": 0, "errors": 0},
                "cognition": {"model_receipts": 1, "non_person_requests": 0},
            },
        )
        self.write_manifest()

    def tearDown(self):
        self.temporary.cleanup()

    def write_json(self, name: str, value: dict):
        (self.evidence / name).write_text(
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )

    def write_manifest(self):
        paths = [
            self.evidence / "evidence.json",
            self.evidence / "genesis" / "SHA256SUMS",
            self.evidence / "qualification-status.json",
        ]
        lines = [f"{sha(path)}  ./{path.relative_to(self.evidence).as_posix()}" for path in paths]
        (self.evidence / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")

    def run_verifier(self):
        return subprocess.run(
            [
                str(VERIFIER),
                "--world-id",
                WORLD_ID,
                "--genesis-directory",
                str(self.genesis),
                "--evidence-directory",
                str(self.evidence),
                "--expected-ruleset",
                "30",
            ],
            cwd=PROJECT_ROOT,
            text=True,
            capture_output=True,
        )

    def test_accepts_a_complete_bound_candidate(self):
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"status":"launch-candidate-evidence-passed"', result.stdout)

    def test_rejects_checksum_tampering(self):
        with (self.evidence / "qualification-status.json").open("a", encoding="utf-8") as target:
            target.write(" ")
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum mismatch", result.stderr)

    def test_rejects_a_failing_gate_even_when_rehashed(self):
        report = json.loads((self.evidence / "qualification-status.json").read_text())
        report["checks"]["two"] = False
        self.write_json("qualification-status.json", report)
        self.write_manifest()
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("failing or malformed check", result.stderr)


if __name__ == "__main__":
    unittest.main()
