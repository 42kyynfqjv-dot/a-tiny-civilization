#!/usr/bin/env python3
"""Fail-closed offline verification of one retained launch-candidate bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import uuid


HEX_DIGEST = re.compile(r"^[0-9a-f]{64}$")


def digest(path: pathlib.Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def load_object(path: pathlib.Path) -> dict:
    with path.open("r", encoding="utf-8") as source:
        value = json.load(source)
    if not isinstance(value, dict):
        raise ValueError(f"{path.name} must contain one JSON object")
    return value


def verify_manifest(root: pathlib.Path) -> str:
    manifest = root / "SHA256SUMS"
    if not manifest.is_file() or manifest.is_symlink():
        raise ValueError(f"unsafe or missing checksum manifest: {manifest}")
    covered: set[pathlib.Path] = set()
    for line in manifest.read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  \./([^\s]+)", line)
        if not match:
            raise ValueError("checksum manifest has a noncanonical line")
        relative = pathlib.PurePosixPath(match.group(2))
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("checksum manifest path escapes its bundle")
        path = root.joinpath(*relative.parts)
        if not path.is_file() or path.is_symlink() or path in covered:
            raise ValueError(f"unsafe, missing, or duplicate covered file: {relative}")
        if digest(path) != match.group(1):
            raise ValueError(f"checksum mismatch: {relative}")
        covered.add(path)
    required = {
        root / "evidence.json",
        root / "qualification-status.json",
        root / "genesis" / "SHA256SUMS",
    }
    if not required.issubset(covered):
        raise ValueError("checksum manifest omits required launch evidence")
    genesis_files = {
        path
        for path in (root / "genesis").iterdir()
        if path.is_file() and not path.is_symlink()
    }
    if not genesis_files.issubset(covered):
        raise ValueError("checksum manifest does not cover every genesis file")
    return digest(manifest)


def require_true_checks(report: dict) -> None:
    checks = report.get("checks")
    if not isinstance(checks, dict) or not checks or any(value is not True for value in checks.values()):
        raise ValueError("qualification report contains a failing or malformed check")


def verify(args: argparse.Namespace) -> dict:
    world_id = str(uuid.UUID(args.world_id))
    if world_id != args.world_id.lower():
        raise ValueError("world ID must use canonical lowercase UUID form")
    genesis = pathlib.Path(args.genesis_directory).resolve(strict=True)
    evidence = pathlib.Path(args.evidence_directory).resolve(strict=True)
    if genesis.is_symlink() or evidence.is_symlink() or genesis == evidence:
        raise ValueError("genesis and evidence must be distinct real directories")

    bundle_manifest_digest = verify_manifest(evidence)
    evidence_record = load_object(evidence / "evidence.json")
    report = load_object(evidence / "qualification-status.json")
    if evidence_record.get("contains_canonical_event_payloads") is not False:
        raise ValueError("evidence bundle does not explicitly exclude canonical event payloads")
    if evidence_record.get("world_id") != world_id or report.get("world_id") != world_id:
        raise ValueError("world identity differs across launch evidence")
    genesis_manifest_digest = digest(genesis / "SHA256SUMS")
    if not HEX_DIGEST.fullmatch(genesis_manifest_digest):
        raise ValueError("genesis checksum identity is malformed")
    if evidence_record.get("genesis_sha256s_sha256") != genesis_manifest_digest:
        raise ValueError("evidence bundle is not bound to the supplied genesis manifest")
    if (evidence / "genesis" / "SHA256SUMS").read_bytes() != (genesis / "SHA256SUMS").read_bytes():
        raise ValueError("retained and supplied genesis checksum manifests differ")

    if report.get("passed") is not True or report.get("replay_verified") is not True:
        raise ValueError("qualification or replay did not pass")
    world = report.get("world")
    if not isinstance(world, dict) or world.get("status") != "running":
        raise ValueError("qualification world is not running")
    if world.get("ruleset_version") != args.expected_ruleset:
        raise ValueError("qualification ruleset is not the expected launch ruleset")
    if world.get("current_tick", -1) < args.minimum_tick:
        raise ValueError("qualification history is too short")
    projections = report.get("projections", {})
    if projections.get("required") != 5 or projections.get("current") != 5:
        raise ValueError("all five observer projections must be current")
    memory = report.get("memory", {})
    if memory.get("total", 0) < 1 or memory.get("pending") != 0 or memory.get("errors") != 0:
        raise ValueError("memory qualification is incomplete")
    cognition = report.get("cognition", {})
    if cognition.get("model_receipts", 0) < 1 or cognition.get("non_person_requests") != 0:
        raise ValueError("local-model or person-only cognition qualification is incomplete")
    require_true_checks(report)

    source_commit = evidence_record.get("source_commit")
    if not isinstance(source_commit, str) or not re.fullmatch(r"[0-9a-f]{40}", source_commit):
        raise ValueError("evidence source commit is malformed")
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", source_commit, "HEAD"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return {
        "status": "launch-candidate-evidence-passed",
        "world_id": world_id,
        "ruleset_version": args.expected_ruleset,
        "through_tick": world["current_tick"],
        "through_sequence": world["current_sequence"],
        "source_commit": source_commit,
        "genesis_sha256s_sha256": genesis_manifest_digest,
        "evidence_sha256s_sha256": bundle_manifest_digest,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--world-id", required=True)
    parser.add_argument("--genesis-directory", required=True)
    parser.add_argument("--evidence-directory", required=True)
    parser.add_argument("--expected-ruleset", required=True, type=int)
    parser.add_argument("--minimum-tick", type=int, default=1000)
    args = parser.parse_args()
    if args.expected_ruleset < 1 or args.minimum_tick < 1:
        parser.error("expected ruleset and minimum tick must be positive")
    try:
        result = verify(args)
    except (OSError, ValueError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"launch candidate evidence failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
