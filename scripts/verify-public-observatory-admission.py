#!/usr/bin/env python3
"""Verify the exact reviewed observer surface without authorizing deployment."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

PROJECT_ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_QUALITY_ADMISSION = PROJECT_ROOT / "docs/operations/QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json"
QUALIFIED_PATHS = ["docs/policies", "web"]
DIMENSIONS = {
    "edge_security",
    "observer_experience",
    "provenance_and_presentation",
    "public_policies",
    "supporter_isolation",
}
ROUTES = ["/", "/lives", "/presentation-policy", "/privacy", "/supporter-policy", "/terms", "/wiki"]
SHA256 = re.compile(r"^[0-9a-f]{64}$")
GIT_COMMIT = re.compile(r"^[0-9a-f]{40}$")


class AdmissionError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise AdmissionError(message)


def load_canonical(path: pathlib.Path) -> dict:
    raw = path.read_bytes()
    if not raw.endswith(b"\n") or raw.count(b"\n") != 1:
        fail("admission must be one newline-terminated canonical JSON line")
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"admission is invalid JSON: {error}")
    if not isinstance(value, dict):
        fail("admission root must be an object")
    canonical = json.dumps(value, sort_keys=True, separators=(",", ":")).encode() + b"\n"
    if raw != canonical:
        fail("admission JSON is not canonical")
    return value


def git(*arguments: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *arguments],
        cwd=PROJECT_ROOT,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def verify(args: argparse.Namespace) -> dict:
    admission = load_canonical(pathlib.Path(args.admission).resolve())
    quality_admission_path = pathlib.Path(args.quality_admission).resolve()
    quality_admission = load_canonical(quality_admission_path)
    expected_fields = {
        "dimensions",
        "public_deployment_authorized",
        "qualified_paths",
        "qualified_source_commit",
        "quality_world_admission_sha256",
        "routes",
        "schema_version",
        "status",
        "web_test_command",
        "world_id",
    }
    if set(admission) != expected_fields:
        fail("admission fields differ from schema version 1")
    if admission["schema_version"] != 1:
        fail("unsupported admission schema")
    if admission["status"] != "accepted_as_public_observatory":
        fail("observer admission status is not accepted")
    if admission["public_deployment_authorized"] is not False:
        fail("observer review cannot authorize deployment")
    expected_world_id = args.world_id or quality_admission.get("world_id")
    if admission["world_id"] != expected_world_id:
        fail("observer admission is for a different world")
    if admission["qualified_paths"] != QUALIFIED_PATHS:
        fail("observer admission qualified paths differ from the enforced boundary")
    if admission["routes"] != ROUTES:
        fail("observer admission route set is incomplete or reordered")
    if admission["web_test_command"] != "cd web && npm test":
        fail("observer admission test command differs from the enforced command")

    source = admission["qualified_source_commit"]
    if not isinstance(source, str) or not GIT_COMMIT.fullmatch(source):
        fail("qualified source commit must be a full lowercase Git commit")
    if git("cat-file", "-e", f"{source}^{{commit}}", check=False).returncode != 0:
        fail("qualified source commit is not available")
    if git("merge-base", "--is-ancestor", source, "HEAD", check=False).returncode != 0:
        fail("qualified source commit is not an ancestor of HEAD")

    quality_digest = hashlib.sha256(quality_admission_path.read_bytes()).hexdigest()
    if not SHA256.fullmatch(admission["quality_world_admission_sha256"]):
        fail("quality-world admission digest is malformed")
    if admission["quality_world_admission_sha256"] != quality_digest:
        fail("observer admission is not bound to the current quality-world admission")

    dimensions = admission["dimensions"]
    if not isinstance(dimensions, list) or len(dimensions) != len(DIMENSIONS):
        fail("observer admission has an incomplete dimension set")
    identities: set[str] = set()
    for dimension in dimensions:
        if not isinstance(dimension, dict) or set(dimension) != {"evidence", "id", "status"}:
            fail("observer admission dimension has an invalid shape")
        if dimension["status"] != "passed":
            fail("every observer admission dimension must pass")
        identity = dimension["id"]
        evidence = dimension["evidence"]
        if not isinstance(identity, str) or identity in identities:
            fail("observer admission dimension identities must be unique strings")
        identities.add(identity)
        if not isinstance(evidence, list) or not evidence or evidence != sorted(set(evidence)):
            fail("observer admission evidence must be a nonempty sorted unique list")
        for path in evidence:
            if not isinstance(path, str) or not any(
                path == root or path.startswith(f"{root}/") for root in QUALIFIED_PATHS
            ):
                fail("observer evidence escapes the qualified boundary")
            if git("cat-file", "-e", f"{source}:{path}", check=False).returncode != 0:
                fail(f"observer evidence is absent from the qualified commit: {path}")
    if identities != DIMENSIONS:
        fail("observer admission dimension identities differ from the review contract")

    if git("diff", "--quiet", source, "--", *QUALIFIED_PATHS, check=False).returncode != 0:
        fail("reviewed observer paths differ from the qualified source commit")
    untracked = git("ls-files", "--others", "--exclude-standard", "--", *QUALIFIED_PATHS).stdout.strip()
    if untracked:
        fail("reviewed observer paths contain untracked files")

    return {
        "status": "public-observatory-admission-passed",
        "world_id": admission["world_id"],
        "qualified_source_commit": source,
        "routes": admission["routes"],
        "public_deployment_authorized": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--admission", required=True)
    parser.add_argument("--quality-admission", default=DEFAULT_QUALITY_ADMISSION)
    parser.add_argument("--world-id")
    args = parser.parse_args()
    try:
        result = verify(args)
    except (AdmissionError, OSError, subprocess.SubprocessError) as error:
        print(f"public-observatory admission rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
