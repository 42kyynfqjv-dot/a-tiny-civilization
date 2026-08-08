#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("audit-canonical-science.py")


def species(identifier: str, name: str) -> dict:
    return {"catalog": "gbif", "identifier": identifier, "scientific_name": name}


def commitment(basis: str) -> dict:
    return {"evidence_basis": basis}


class CanonicalScienceAuditTest(unittest.TestCase):
    def make_fixture(self, root: Path) -> None:
        human = species("2436436", "Homo sapiens")
        fauna = species("1", "Example animal")
        body_entries = []
        for identity, basis in ((human, "engineering_assumption"), (fauna, "literature_approximation")):
            body_entries.append(
                {
                    "species": identity,
                    "metabolic_rate": commitment(basis),
                    "physiological_regulation": commitment("engineering_assumption"),
                    "reproductive_physiology": {
                        "evidence_basis": "engineering_assumption",
                        "category_maturity": [commitment(basis), commitment(basis)],
                    },
                    "adult_body_mass": commitment(basis),
                    "heritable_disposition_profile": commitment("engineering_assumption"),
                }
            )
        documents = {
            "fauna-population-plan.json": {"entries": [{"species": fauna}]},
            "fauna-ecology-plan.json": {"entries": []},
            "organism-body-profile-plan.json": {"entries": body_entries},
            "material-resource-plan.json": {
                "sources": [
                    {
                        "material": {"canonical_name": "water"},
                        "reservoir": commitment("engineering_assumption"),
                        "oral_transfer_profiles": [
                            {"species": human, "evidence_basis": "engineering_assumption"},
                            {"species": fauna, "evidence_basis": "engineering_assumption"},
                        ],
                    }
                ]
            },
        }
        manifest = []
        for name, document in sorted(documents.items()):
            payload = json.dumps(document, separators=(",", ":")).encode()
            (root / name).write_bytes(payload)
            manifest.append(f"{hashlib.sha256(payload).hexdigest()}  {name}\n")
        (root / "SHA256SUMS").write_text("".join(manifest), encoding="utf-8")

    def run_audit(self, root: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(SCRIPT), str(root)], text=True, capture_output=True, check=False
        )

    def test_reports_evidence_counts_and_uncovered_ecology(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            result = self.run_audit(root)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertFalse(report["scientific_admission"])
            self.assertEqual(report["species"]["body_profile_count"], 2)
            self.assertEqual(report["species"]["fauna_ecology_uncovered"], ["Example animal"])
            self.assertEqual(
                report["causal_commitments"]["metabolic_rate"],
                {"engineering_assumption": 1, "literature_approximation": 1},
            )
            self.assertEqual(report["material_resources"]["oral_transfer_profiles"], {"engineering_assumption": 2})

    def test_rejects_tampered_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            (root / "fauna-ecology-plan.json").write_text("{}", encoding="utf-8")
            result = self.run_audit(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("digest mismatch", result.stderr)

    def test_rejects_partial_oral_coverage(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.make_fixture(root)
            path = root / "material-resource-plan.json"
            document = json.loads(path.read_text(encoding="utf-8"))
            document["sources"][0]["oral_transfer_profiles"].pop()
            payload = json.dumps(document, separators=(",", ":")).encode()
            path.write_bytes(payload)
            lines = (root / "SHA256SUMS").read_text(encoding="utf-8").splitlines()
            lines = [
                f"{hashlib.sha256(payload).hexdigest()}  material-resource-plan.json"
                if line.endswith("  material-resource-plan.json")
                else line
                for line in lines
            ]
            (root / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
            result = self.run_audit(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("coverage", result.stderr)


if __name__ == "__main__":
    unittest.main()
