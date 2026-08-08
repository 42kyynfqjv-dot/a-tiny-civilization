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
        (self.genesis / "seed.json").write_text("{}\n", encoding="utf-8")
        self.write_genesis_manifest(update_evidence=False)
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

    def write_genesis_manifest(self, update_evidence: bool = True):
        paths = sorted(
            path
            for path in self.genesis.iterdir()
            if path.is_file() and path.name != "SHA256SUMS"
        )
        lines = [f"{sha(path)}  ./{path.name}" for path in paths]
        manifest = self.genesis / "SHA256SUMS"
        manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
        (self.evidence / "genesis" / "SHA256SUMS").write_bytes(manifest.read_bytes())
        if update_evidence:
            record = json.loads((self.evidence / "evidence.json").read_text())
            record["genesis_sha256s_sha256"] = sha(manifest)
            self.write_json("evidence.json", record)
            self.write_manifest()

    def run_verifier(self, expected_ruleset: int = 30):
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
                str(expected_ruleset),
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

    def test_rejects_an_absolute_genesis_manifest_even_when_bound(self):
        manifest = self.genesis / "SHA256SUMS"
        manifest.write_text(
            f"{sha(self.genesis / 'seed.json')}  {self.genesis / 'seed.json'}\n",
            encoding="utf-8",
        )
        (self.evidence / "genesis" / "SHA256SUMS").write_bytes(manifest.read_bytes())
        record = json.loads((self.evidence / "evidence.json").read_text())
        record["genesis_sha256s_sha256"] = sha(manifest)
        self.write_json("evidence.json", record)
        self.write_manifest()
        result = self.run_verifier()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("noncanonical or nonportable", result.stderr)

    def configure_ruleset_31_genesis(self):
        species = {"catalog": "gbif", "identifier": "2436436"}
        (self.genesis / "organism-body-profile-plan.json").write_text(
            json.dumps(
                {
                    "entries": [
                        {
                            "species": species,
                            "adult_body_mass": {
                                "mass_grams_value": 70_000,
                                "mass_grams_decimal_places": 0,
                            },
                            "metabolic_rate": {
                                "measured_power_value": 148_461_427,
                                "measured_power_decimal_places": 6,
                            },
                            "physiological_regulation": {
                                "usable_energy_reserve_joules": 89_789_472,
                            },
                        }
                    ]
                },
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        profiles = lambda energy, hydration: [
            {
                "species": species,
                "transfer_mass_milligrams": 700_000,
                "recoverable_energy_joules": energy,
                "hydration_recovery_seconds": hydration,
            }
        ]
        (self.genesis / "material-resource-plan.json").write_text(
            json.dumps(
                {
                    "sources": [
                        {
                            "material": {"identifier": "5793"},
                            "oral_transfer_profiles": profiles(11_200_000, 0),
                        },
                        {
                            "material": {"identifier": "962"},
                            "oral_transfer_profiles": profiles(0, 21_600),
                        },
                    ]
                },
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        report = json.loads((self.evidence / "qualification-status.json").read_text())
        report["world"]["ruleset_version"] = 31
        self.write_json("qualification-status.json", report)
        self.write_genesis_manifest()

    def test_ruleset_31_requires_mass_scaled_energy_and_oral_transfer(self):
        self.configure_ruleset_31_genesis()
        result = self.run_verifier(31)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("metabolic power is still universal", result.stderr)

        body = json.loads((self.genesis / "organism-body-profile-plan.json").read_text())
        second = json.loads(json.dumps(body["entries"][0]))
        second["species"] = {"catalog": "gbif", "identifier": "1"}
        second["adult_body_mass"]["mass_grams_value"] = 3_500
        second["adult_body_mass"]["mass_grams_decimal_places"] = 3
        second["metabolic_rate"]["measured_power_value"] = 187_378
        second["physiological_regulation"]["usable_energy_reserve_joules"] = 113_327
        body["entries"].append(second)
        (self.genesis / "organism-body-profile-plan.json").write_text(
            json.dumps(body, separators=(",", ":")), encoding="utf-8"
        )
        materials = json.loads((self.genesis / "material-resource-plan.json").read_text())
        for source in materials["sources"]:
            source["oral_transfer_profiles"].append(
                {
                    "species": second["species"],
                    "transfer_mass_milligrams": 35,
                    "recoverable_energy_joules": 560 if source["material"]["identifier"] == "5793" else 0,
                    "hydration_recovery_seconds": 0 if source["material"]["identifier"] == "5793" else 21_600,
                }
            )
        (self.genesis / "material-resource-plan.json").write_text(
            json.dumps(materials, separators=(",", ":")), encoding="utf-8"
        )
        self.write_genesis_manifest()
        result = self.run_verifier(31)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
