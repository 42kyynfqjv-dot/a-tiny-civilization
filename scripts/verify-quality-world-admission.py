#!/usr/bin/env python3
"""Verify a canonical, evidence-bound experimental quality-world admission."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys


EXPECTED_DIMENSIONS = {
    "source_bound_environment",
    "movement_material_discovery",
    "survival_and_mortality",
    "real_taxa_and_fauna",
    "private_reproduction_and_heredity",
    "memory_communication_and_cognition",
    "public_observatory_and_privacy",
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
QUALIFIED_PATHS = [
    "Cargo.lock",
    "Cargo.toml",
    "Dockerfile",
    "apps",
    "crates",
    "data",
    "db",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "scripts/advance-cognition-qualified-world.sh",
    "scripts/advance-qualification-world.sh",
    "scripts/create-qualification-evidence.sh",
    "scripts/initialize-canonical-world.sh",
    "scripts/initialize-provisional-world.sh",
    "scripts/qualification-status.sh",
]


def fail(message: str) -> None:
    raise ValueError(message)


def load_canonical(path: pathlib.Path) -> dict[str, object]:
    raw = path.read_bytes()
    if not raw.endswith(b"\n") or raw.count(b"\n") != 1:
        fail("admission must be one newline-terminated canonical JSON line")
    value = json.loads(raw)
    if not isinstance(value, dict):
        fail("admission root must be an object")
    encoded = json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode()
    if raw != encoded + b"\n":
        fail("admission JSON is not canonical")
    return value


def verify(args: argparse.Namespace) -> dict[str, object]:
    project_root = pathlib.Path(args.project_root).resolve()
    admission_path = pathlib.Path(args.admission).resolve()
    admission = load_canonical(admission_path)

    expected_keys = {
        "schema_version", "world_id", "ruleset_version", "genesis_sha256s_sha256",
        "evidence_sha256s_sha256", "quality_gate_adr", "experimental_science_policy_adr",
        "status", "scientific_admission", "public_deployment_authorized", "dimensions",
        "qualified_source_commit", "qualified_paths",
    }
    if set(admission) != expected_keys:
        fail("admission fields differ from schema version 2")
    if admission["schema_version"] != 2:
        fail("unsupported admission schema")
    if not isinstance(admission["world_id"], str) or not UUID.fullmatch(admission["world_id"]):
        fail("world identity is invalid")
    if admission["world_id"] != args.world_id:
        fail("admission is for a different world")
    if admission["ruleset_version"] != args.expected_ruleset:
        fail("admission is for a different ruleset")
    for key, expected in (
        ("genesis_sha256s_sha256", args.genesis_sha256s_sha256),
        ("evidence_sha256s_sha256", args.evidence_sha256s_sha256),
    ):
        actual = admission[key]
        if not isinstance(actual, str) or not HEX64.fullmatch(actual) or actual != expected:
            fail(f"{key} does not bind the verified candidate")
    if admission["status"] != "accepted_as_experimental_quality_world":
        fail("quality-world status is not accepted")
    if admission["scientific_admission"] is not False:
        fail("experimental quality admission cannot claim scientific admission")
    if admission["public_deployment_authorized"] is not False:
        fail("quality admission cannot authorize deployment")
    qualified_source_commit = admission["qualified_source_commit"]
    if (
        not isinstance(qualified_source_commit, str)
        or not re.fullmatch(r"[0-9a-f]{40}", qualified_source_commit)
        or qualified_source_commit != args.qualified_source_commit
    ):
        fail("quality admission is not bound to the evidence source commit")
    if admission["qualified_paths"] != QUALIFIED_PATHS:
        fail("qualified source paths differ from the frozen launch boundary")

    expected_policy_paths = {
        "quality_gate_adr": "docs/adr/0042-quality-gate-before-public-genesis.md",
        "experimental_science_policy_adr": "docs/adr/0049-experimental-genesis-science-policy.md",
    }
    evidence_paths: list[str] = []
    for key, expected in expected_policy_paths.items():
        if admission[key] != expected:
            fail(f"{key} is not the accepted policy")
        evidence_paths.append(expected)

    dimensions = admission["dimensions"]
    if not isinstance(dimensions, list) or len(dimensions) != len(EXPECTED_DIMENSIONS):
        fail("admission has an incomplete dimension set")
    seen: set[str] = set()
    for dimension in dimensions:
        if not isinstance(dimension, dict) or set(dimension) != {"id", "status", "evidence"}:
            fail("quality dimension has an invalid shape")
        identifier = dimension["id"]
        if not isinstance(identifier, str) or identifier in seen:
            fail("quality dimension identifiers must be unique strings")
        seen.add(identifier)
        if dimension["status"] != "passed":
            fail(f"quality dimension {identifier} has not passed")
        evidence = dimension["evidence"]
        if not isinstance(evidence, list) or not evidence or evidence != sorted(set(evidence)):
            fail(f"quality dimension {identifier} evidence must be nonempty, unique, and sorted")
        if not all(isinstance(path, str) for path in evidence):
            fail(f"quality dimension {identifier} has a non-string evidence path")
        evidence_paths.extend(evidence)
    if seen != EXPECTED_DIMENSIONS:
        fail("admission dimension identities differ from the quality gate")

    for relative in evidence_paths:
        if relative.startswith("/") or ".." in pathlib.PurePosixPath(relative).parts:
            fail(f"unsafe evidence path: {relative}")
        resolved = (project_root / relative).resolve()
        if os.path.commonpath((project_root, resolved)) != str(project_root):
            fail(f"evidence escapes the project: {relative}")
        if not resolved.is_file() or resolved.is_symlink():
            fail(f"evidence path is absent or unsafe: {relative}")

    git_checks = (
        ["git", "diff", "--quiet", qualified_source_commit, "HEAD", "--", *QUALIFIED_PATHS],
        ["git", "diff", "--quiet", "--", *QUALIFIED_PATHS],
        ["git", "diff", "--cached", "--quiet", "--", *QUALIFIED_PATHS],
    )
    for command in git_checks:
        result = subprocess.run(command, cwd=project_root, check=False)
        if result.returncode != 0:
            fail("qualified source boundary differs from the exercised candidate")
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard", "--", *QUALIFIED_PATHS],
        cwd=project_root,
        text=True,
    ).strip()
    if untracked:
        fail("qualified source boundary contains untracked files")

    return {
        "status": "experimental-quality-world-admission-passed",
        "world_id": admission["world_id"],
        "ruleset_version": admission["ruleset_version"],
        "dimensions": len(dimensions),
        "scientific_admission": False,
        "public_deployment_authorized": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--admission", required=True)
    parser.add_argument("--project-root", default=pathlib.Path(__file__).resolve().parent.parent)
    parser.add_argument("--world-id", required=True)
    parser.add_argument("--expected-ruleset", required=True, type=int)
    parser.add_argument("--genesis-sha256s-sha256", required=True)
    parser.add_argument("--evidence-sha256s-sha256", required=True)
    parser.add_argument("--qualified-source-commit", required=True)
    args = parser.parse_args()
    try:
        result = verify(args)
    except (OSError, json.JSONDecodeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"quality-world admission rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
