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


def verify_genesis_manifest(root: pathlib.Path) -> str:
    manifest = root / "SHA256SUMS"
    if not manifest.is_file() or manifest.is_symlink():
        raise ValueError(f"unsafe or missing genesis checksum manifest: {manifest}")
    covered: set[pathlib.Path] = set()
    for line in manifest.read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  \./([^\s]+)", line)
        if not match:
            raise ValueError("genesis checksum manifest has a noncanonical or nonportable line")
        relative = pathlib.PurePosixPath(match.group(2))
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("genesis checksum manifest path escapes its bundle")
        path = root.joinpath(*relative.parts)
        if not path.is_file() or path.is_symlink() or path in covered or path == manifest:
            raise ValueError(f"unsafe, missing, or duplicate genesis file: {relative}")
        if digest(path) != match.group(1):
            raise ValueError(f"genesis checksum mismatch: {relative}")
        covered.add(path)
    genesis_files = {
        path
        for path in root.iterdir()
        if path.is_file() and not path.is_symlink() and path != manifest
    }
    if not genesis_files or covered != genesis_files:
        raise ValueError("genesis checksum manifest must cover every and only genesis artifact")
    return digest(manifest)


def require_true_checks(report: dict) -> None:
    checks = report.get("checks")
    if not isinstance(checks, dict) or not checks or any(value is not True for value in checks.values()):
        raise ValueError("qualification report contains a failing or malformed check")


def verify_mass_scaled_genesis(genesis: pathlib.Path) -> None:
    body_plan = load_object(genesis / "organism-body-profile-plan.json")
    material_plan = load_object(genesis / "material-resource-plan.json")
    entries = body_plan.get("entries")
    if not isinstance(entries, list) or not entries:
        raise ValueError("ruleset-31 body profiles are absent")
    masses: dict[tuple[str, str], tuple[int, int]] = {}
    powers: set[tuple[int, int]] = set()
    for entry in entries:
        try:
            species = entry["species"]
            key = (species["catalog"], species["identifier"])
            mass = entry["adult_body_mass"]
            metabolic = entry["metabolic_rate"]
            regulation = entry["physiological_regulation"]
            mass_value = mass["mass_grams_value"]
            mass_places = mass["mass_grams_decimal_places"]
            power_value = metabolic["measured_power_value"]
            power_places = metabolic["measured_power_decimal_places"]
            reserve = regulation["usable_energy_reserve_joules"]
        except (KeyError, TypeError) as error:
            raise ValueError("ruleset-31 body profile is malformed") from error
        values = (mass_value, mass_places, power_value, power_places, reserve)
        if not all(isinstance(value, int) and not isinstance(value, bool) for value in values):
            raise ValueError("ruleset-31 mass, power, or reserve is not integral")
        if mass_value <= 0 or not 0 <= mass_places <= 9 or power_value <= 0 or not 0 <= power_places <= 9:
            raise ValueError("ruleset-31 mass or power is outside its fixed-point domain")
        power_scale = 10 ** power_places
        expected_reserve = (power_value * 604_800 + power_scale - 1) // power_scale
        if reserve != expected_reserve:
            raise ValueError("ruleset-31 usable energy reserve is not seven days of committed power")
        if key in masses:
            raise ValueError("ruleset-31 body profile repeats a species")
        masses[key] = (mass_value, mass_places)
        powers.add((power_value, power_places))
    if len(powers) < 2:
        raise ValueError("ruleset-31 metabolic power is still universal")

    sources = material_plan.get("sources")
    if not isinstance(sources, list):
        raise ValueError("ruleset-31 material sources are absent")
    oral_by_material: dict[str, list[dict]] = {}
    for source in sources:
        if isinstance(source, dict) and isinstance(source.get("material"), dict):
            oral_by_material[source["material"].get("identifier")] = source.get(
                "oral_transfer_profiles", []
            )
    for material_id, energy_per_milligram, hydration_seconds in (
        ("5793", 16, 0),
        ("962", 0, 21_600),
    ):
        profiles = oral_by_material.get(material_id)
        if not isinstance(profiles, list) or len(profiles) != len(masses):
            raise ValueError("ruleset-31 oral transfer coverage is incomplete")
        seen: set[tuple[str, str]] = set()
        for profile in profiles:
            try:
                species = profile["species"]
                key = (species["catalog"], species["identifier"])
                transfer = profile["transfer_mass_milligrams"]
                energy = profile["recoverable_energy_joules"]
                hydration = profile["hydration_recovery_seconds"]
            except (KeyError, TypeError) as error:
                raise ValueError("ruleset-31 oral transfer profile is malformed") from error
            if key not in masses or key in seen:
                raise ValueError("ruleset-31 oral transfer species differ from body profiles")
            mass_value, mass_places = masses[key]
            denominator = (10 ** mass_places) * 1_000_000
            expected_transfer = max(1, (mass_value * 1_000 * 10_000 + denominator - 1) // denominator)
            if (
                transfer != expected_transfer
                or energy != transfer * energy_per_milligram
                or hydration != hydration_seconds
            ):
                raise ValueError("ruleset-31 oral transfer is not body-mass scaled")
            seen.add(key)


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
    genesis_manifest_digest = verify_genesis_manifest(genesis)
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
    if args.expected_ruleset >= 31:
        verify_mass_scaled_genesis(genesis)
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
