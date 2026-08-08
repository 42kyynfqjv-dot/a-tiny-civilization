#!/usr/bin/env python3
"""Produce a deterministic, fail-closed science audit for a genesis directory.

This tool does not grant scientific admission. It verifies the genesis checksum
manifest, checks the cross-file species contracts used by the causal runtime, and
reports how many commitments are source-informed versus engineering assumptions.
"""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import sys


REQUIRED_FILES = (
    "fauna-ecology-plan.json",
    "fauna-population-plan.json",
    "material-resource-plan.json",
    "organism-body-profile-plan.json",
)
EVIDENCE_BASES = (
    "direct_measurement",
    "documented_transformation",
    "literature_approximation",
    "engineering_assumption",
)
SHA256_RE = re.compile(r"^([0-9a-f]{64})  (.+)$")
PORTABLE_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def fail(message: str) -> None:
    raise ValueError(message)


def load_json(root: Path, name: str) -> dict:
    path = root / name
    if not path.is_file() or path.is_symlink():
        fail(f"required genesis artifact is absent or unsafe: {name}")
    try:
        value = json.loads(path.read_bytes())
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot decode {name}: {error}")
    if not isinstance(value, dict):
        fail(f"genesis artifact must contain one JSON object: {name}")
    return value


def verify_checksums(root: Path) -> str:
    manifest_path = root / "SHA256SUMS"
    if not manifest_path.is_file() or manifest_path.is_symlink():
        fail("genesis SHA256SUMS is absent or unsafe")
    entries: dict[str, str] = {}
    for line_number, line in enumerate(
        manifest_path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        match = SHA256_RE.fullmatch(line)
        if match is None:
            fail(f"invalid SHA256SUMS line {line_number}")
        digest, raw_path = match.groups()
        covered_path = Path(raw_path)
        if covered_path.is_absolute():
            if covered_path.parent.resolve() != root.resolve():
                fail(f"SHA256SUMS line {line_number} escapes the genesis directory")
            name = covered_path.name
        else:
            if covered_path.parent != Path("."):
                fail(f"SHA256SUMS line {line_number} contains a nested path")
            name = covered_path.name
        if PORTABLE_NAME_RE.fullmatch(name) is None:
            fail(f"SHA256SUMS line {line_number} has an unsafe artifact name")
        if name in entries:
            fail(f"duplicate SHA256SUMS entry: {name}")
        entries[name] = digest
    for required in REQUIRED_FILES:
        if required not in entries:
            fail(f"SHA256SUMS does not cover required artifact: {required}")
    for name, expected in entries.items():
        path = root / name
        if not path.is_file() or path.is_symlink():
            fail(f"checksummed genesis artifact is absent or unsafe: {name}")
        observed = hashlib.sha256(path.read_bytes()).hexdigest()
        if observed != expected:
            fail(f"genesis artifact digest mismatch: {name}")
    return hashlib.sha256(manifest_path.read_bytes()).hexdigest()


def require_list(document: dict, field: str, source: str) -> list:
    value = document.get(field)
    if not isinstance(value, list):
        fail(f"{source}.{field} must be a list")
    return value


def species_key(value: object, source: str) -> tuple[str, str, str]:
    if not isinstance(value, dict):
        fail(f"{source} species must be an object")
    fields = tuple(value.get(field) for field in ("catalog", "identifier", "scientific_name"))
    if not all(isinstance(field, str) and field for field in fields):
        fail(f"{source} species identity is incomplete")
    return fields  # type: ignore[return-value]


def unique_species(entries: list, source: str) -> dict[tuple[str, str, str], dict]:
    result: dict[tuple[str, str, str], dict] = {}
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            fail(f"{source}[{index}] must be an object")
        key = species_key(entry.get("species"), f"{source}[{index}]")
        if key in result:
            fail(f"duplicate species in {source}: {key[2]}")
        result[key] = entry
    return result


def evidence_counts(entries: list, field: str, source: str) -> dict[str, int]:
    counts: Counter[str] = Counter()
    for index, entry in enumerate(entries):
        commitment = entry.get(field) if isinstance(entry, dict) else None
        if not isinstance(commitment, dict):
            fail(f"{source}[{index}].{field} must be an object")
        basis = commitment.get("evidence_basis")
        if basis not in EVIDENCE_BASES:
            fail(f"{source}[{index}].{field} has unsupported evidence basis")
        counts[basis] += 1
    return {basis: counts[basis] for basis in EVIDENCE_BASES if counts[basis]}


def audit(root: Path) -> dict:
    manifest_digest = verify_checksums(root)
    population = load_json(root, "fauna-population-plan.json")
    body = load_json(root, "organism-body-profile-plan.json")
    ecology = load_json(root, "fauna-ecology-plan.json")
    materials = load_json(root, "material-resource-plan.json")

    population_entries = require_list(population, "entries", "fauna-population-plan")
    body_entries = require_list(body, "entries", "organism-body-profile-plan")
    ecology_entries = require_list(ecology, "entries", "fauna-ecology-plan")
    material_sources = require_list(materials, "sources", "material-resource-plan")
    population_species = unique_species(population_entries, "fauna-population-plan.entries")
    body_species = unique_species(body_entries, "organism-body-profile-plan.entries")
    ecology_species = unique_species(ecology_entries, "fauna-ecology-plan.entries")

    human = ("gbif", "2436436", "Homo sapiens")
    if set(body_species) != set(population_species) | {human}:
        fail("body-profile species must equal every fauna species plus Homo sapiens")
    if not set(ecology_species).issubset(population_species):
        fail("fauna ecology plan contains a species outside the population plan")

    causal_fields = (
        "metabolic_rate",
        "physiological_regulation",
        "reproductive_physiology",
        "adult_body_mass",
        "heritable_disposition_profile",
    )
    commitments = {
        field: evidence_counts(body_entries, field, "organism-body-profile-plan.entries")
        for field in causal_fields
    }

    maturity_counts: Counter[str] = Counter()
    for index, entry in enumerate(body_entries):
        reproductive = entry["reproductive_physiology"]
        categories = require_list(
            reproductive,
            "category_maturity",
            f"organism-body-profile-plan.entries[{index}].reproductive_physiology",
        )
        for category in categories:
            if not isinstance(category, dict) or category.get("evidence_basis") not in EVIDENCE_BASES:
                fail("category maturity commitment has unsupported evidence basis")
            maturity_counts[category["evidence_basis"]] += 1

    reservoir_counts: Counter[str] = Counter()
    oral_counts: Counter[str] = Counter()
    material_summary = []
    for index, source in enumerate(material_sources):
        if not isinstance(source, dict):
            fail(f"material-resource-plan.sources[{index}] must be an object")
        material = source.get("material")
        if not isinstance(material, dict) or not isinstance(material.get("canonical_name"), str):
            fail(f"material-resource-plan.sources[{index}] has no material identity")
        reservoir = source.get("reservoir")
        reservoir_basis = None
        if reservoir is not None:
            if not isinstance(reservoir, dict) or reservoir.get("evidence_basis") not in EVIDENCE_BASES:
                fail("material reservoir has unsupported evidence basis")
            reservoir_basis = reservoir["evidence_basis"]
            reservoir_counts[reservoir_basis] += 1
        oral_profiles = require_list(
            source,
            "oral_transfer_profiles",
            f"material-resource-plan.sources[{index}]",
        )
        oral_species = set()
        for profile in oral_profiles:
            if not isinstance(profile, dict) or profile.get("evidence_basis") not in EVIDENCE_BASES:
                fail("oral-transfer profile has unsupported evidence basis")
            oral_counts[profile["evidence_basis"]] += 1
            key = species_key(profile.get("species"), "oral-transfer profile")
            if key in oral_species:
                fail("material has duplicate oral-transfer profile for one species")
            oral_species.add(key)
        if oral_species and oral_species != set(body_species):
            fail("material oral-transfer coverage must include every body-profile species")
        material_summary.append(
            {
                "material": material["canonical_name"],
                "oral_transfer_profile_count": len(oral_profiles),
                "reservoir_evidence_basis": reservoir_basis,
            }
        )

    return {
        "audit_schema_version": 1,
        "scientific_admission": False,
        "genesis_sha256s_sha256": manifest_digest,
        "species": {
            "body_profile_count": len(body_species),
            "fauna_ecology_covered_count": len(ecology_species),
            "fauna_ecology_uncovered": sorted(key[2] for key in set(population_species) - set(ecology_species)),
            "fauna_population_species_count": len(population_species),
        },
        "causal_commitments": commitments,
        "category_maturity": {
            basis: maturity_counts[basis] for basis in EVIDENCE_BASES if maturity_counts[basis]
        },
        "material_resources": {
            "materials": sorted(material_summary, key=lambda value: value["material"]),
            "oral_transfer_profiles": {
                basis: oral_counts[basis] for basis in EVIDENCE_BASES if oral_counts[basis]
            },
            "reservoirs": {
                basis: reservoir_counts[basis] for basis in EVIDENCE_BASES if reservoir_counts[basis]
            },
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("genesis_directory", type=Path)
    arguments = parser.parse_args()
    try:
        report = audit(arguments.genesis_directory.resolve(strict=True))
    except (OSError, ValueError) as error:
        print(f"canonical science audit failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
