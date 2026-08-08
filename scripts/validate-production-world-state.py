#!/usr/bin/env python3
"""Validate the complete production worlds-table result for one launch phase."""

from __future__ import annotations

import argparse
import json
import re
import sys

UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")


class StateError(RuntimeError):
    pass


def positive_integer(raw: str, field: str, *, allow_zero: bool) -> int:
    if not raw.isascii() or not raw.isdigit():
        raise StateError(f"{field} is not a canonical unsigned integer")
    value = int(raw)
    if (not allow_zero and value == 0) or str(value) != raw:
        raise StateError(f"{field} is outside its canonical range")
    return value


def validate(mode: str, expected_world_id: str, expected_ruleset: int, raw: str) -> dict:
    if not UUID.fullmatch(expected_world_id):
        raise StateError("expected world identity is not a lowercase UUID")
    if expected_ruleset <= 0:
        raise StateError("expected ruleset must be positive")
    if raw and not raw.endswith("\n"):
        raise StateError("world rows must be newline terminated")
    lines = raw.splitlines()
    if not lines:
        if mode != "allow-empty":
            raise StateError("production database has no activated world")
        return {
            "expected_ruleset": expected_ruleset,
            "expected_world_id": expected_world_id,
            "status": "empty-ready-for-qualified-activation",
        }
    if len(lines) != 1:
        raise StateError(f"production database contains {len(lines)} worlds")
    fields = lines[0].split("|")
    if len(fields) != 5:
        raise StateError("production world row has an unexpected shape")
    world_id, ruleset_raw, tick_raw, sequence_raw, status = fields
    ruleset = positive_integer(ruleset_raw, "ruleset", allow_zero=False)
    tick = positive_integer(tick_raw, "tick", allow_zero=True)
    sequence = positive_integer(sequence_raw, "sequence", allow_zero=False)
    if world_id != expected_world_id:
        raise StateError("production database contains a different world")
    if ruleset != expected_ruleset:
        raise StateError("production database contains a different ruleset")
    if status != "running":
        raise StateError("production world is not running")
    return {
        "current_sequence": sequence,
        "current_tick": tick,
        "ruleset_version": ruleset,
        "status": "qualified-running-world",
        "world_id": world_id,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("allow-empty", "require-running"), required=True)
    parser.add_argument("--expected-world-id", required=True)
    parser.add_argument("--expected-ruleset", required=True, type=int)
    args = parser.parse_args()
    try:
        result = validate(
            args.mode,
            args.expected_world_id,
            args.expected_ruleset,
            sys.stdin.read(),
        )
    except StateError as error:
        print(f"production world state rejected: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
