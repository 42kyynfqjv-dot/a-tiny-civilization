#!/usr/bin/env python3

from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest


PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[1]
VERIFIER = PROJECT_ROOT / "scripts" / "verify-quality-world-admission.py"
ADMISSION = PROJECT_ROOT / "docs" / "operations" / "QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json"
WORLD_ID = "b3ea736d-7a5a-5161-a74b-fa8c4302d333"
GENESIS_DIGEST = "76d54b0749bd9602c625c73d9f6eac78c21ca06865ece796976e49284e06a725"
EVIDENCE_DIGEST = "a2ff86bfdede7daa6cec451f01df558d20f54e970c2e8c618f88064e9a865990"
QUALIFIED_SOURCE_COMMIT = "30a26a062fc9b2704b30dcc11c0c50992e1d2852"


class QualityWorldAdmissionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls._worktree_parent = tempfile.TemporaryDirectory()
        cls.qualified_root = pathlib.Path(cls._worktree_parent.name) / "qualified-source"
        subprocess.run(
            [
                "git", "worktree", "add", "--detach",
                str(cls.qualified_root), QUALIFIED_SOURCE_COMMIT,
            ],
            cwd=PROJECT_ROOT,
            check=True,
            text=True,
            capture_output=True,
        )
        admission = json.loads(ADMISSION.read_text(encoding="utf-8"))
        evidence_paths = {
            admission["quality_gate_adr"],
            admission["experimental_science_policy_adr"],
        }
        for dimension in admission["dimensions"]:
            evidence_paths.update(dimension["evidence"])
        for relative in evidence_paths:
            source = PROJECT_ROOT / relative
            destination = cls.qualified_root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)

    @classmethod
    def tearDownClass(cls):
        subprocess.run(
            ["git", "worktree", "remove", "--force", str(cls.qualified_root)],
            cwd=PROJECT_ROOT,
            check=True,
            text=True,
            capture_output=True,
        )
        cls._worktree_parent.cleanup()

    def run_verifier(
        self,
        admission: pathlib.Path = ADMISSION,
        project_root: pathlib.Path | None = None,
    ):
        project_root = project_root or self.qualified_root
        return subprocess.run(
            [
                str(VERIFIER),
                "--admission", str(admission),
                "--project-root", str(project_root),
                "--world-id", WORLD_ID,
                "--expected-ruleset", "32",
                "--genesis-sha256s-sha256", GENESIS_DIGEST,
                "--evidence-sha256s-sha256", EVIDENCE_DIGEST,
                "--qualified-source-commit", QUALIFIED_SOURCE_COMMIT,
            ],
            cwd=project_root,
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

    def test_exact_experimental_quality_world_passes(self):
        result = self.run_verifier()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"public_deployment_authorized":false', result.stdout)

    def test_changed_current_source_cannot_reuse_the_archived_admission(self):
        result = self.run_verifier(project_root=PROJECT_ROOT)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("qualified source boundary differs", result.stderr)

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

    def test_rejects_a_different_qualified_source_commit(self):
        admission = self.changed_admission(
            lambda value: value.__setitem__("qualified_source_commit", "0" * 40)
        )
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not bound to the evidence source commit", result.stderr)

    def test_rejects_a_different_qualified_path_boundary(self):
        admission = self.changed_admission(
            lambda value: value["qualified_paths"].pop()
        )
        result = self.run_verifier(admission)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("differ from the frozen launch boundary", result.stderr)


if __name__ == "__main__":
    unittest.main()
