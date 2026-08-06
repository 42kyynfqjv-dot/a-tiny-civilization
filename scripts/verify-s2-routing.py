"""Verify the checked-in ECEF-to-S2 vectors without Rust or floating point."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


MAX_LEVEL = 30
MAX_ECEF_COMPONENT_MM = 7_000_000_000
SWAP_MASK = 0x01
IJ_TO_POSITION = (
    (0, 1, 3, 2),
    (0, 3, 1, 2),
    (2, 3, 1, 0),
    (2, 1, 3, 0),
)
POSITION_TO_ORIENTATION = (SWAP_MASK, 0, 0, 0x03)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def face_uv(point: tuple[int, int, int]) -> tuple[int, int, int, int]:
    x, y, z = point
    absolute = (abs(x), abs(y), abs(z))
    if absolute[0] > absolute[1]:
        axis = 0 if absolute[0] > absolute[2] else 2
    elif absolute[1] > absolute[2]:
        axis = 1
    else:
        axis = 2
    selected = point[axis]
    face = axis + (3 if selected < 0 else 0)
    by_face = (
        (y, z, x),
        (-x, z, y),
        (-x, -y, z),
        (-z, -y, -x),
        (-z, x, -y),
        (y, x, -z),
    )
    u_numerator, v_numerator, denominator = by_face[face]
    require(denominator > 0, "selected S2 face denominator must be positive")
    require(abs(u_numerator) <= denominator, "u lies outside the selected face")
    require(abs(v_numerator) <= denominator, "v lies outside the selected face")
    return face, u_numerator, v_numerator, denominator


def boundary_at_or_below(
    boundary_index: int,
    size: int,
    value_numerator: int,
    value_denominator: int,
) -> bool:
    size_squared = size * size
    if 2 * boundary_index >= size:
        boundary_numerator = 4 * boundary_index * boundary_index - size_squared
    else:
        complement = size - boundary_index
        boundary_numerator = size_squared - 4 * complement * complement
    boundary_denominator = 3 * size_squared
    return boundary_numerator * value_denominator <= value_numerator * boundary_denominator


def coordinate_index(value_numerator: int, value_denominator: int, level: int) -> int:
    require(value_denominator > 0, "coordinate denominator must be positive")
    require(abs(value_numerator) <= value_denominator, "coordinate lies outside a face")
    size = 1 << level
    lower = 0
    upper = size
    while lower < upper:
        midpoint = lower + (upper - lower + 1) // 2
        if boundary_at_or_below(midpoint, size, value_numerator, value_denominator):
            lower = midpoint
        else:
            upper = midpoint - 1
    return min(lower, size - 1)


def cell_id_from_face_ij(face: int, i: int, j: int, level: int) -> int:
    orientation = face & SWAP_MASK
    position = 0
    for bit in reversed(range(level)):
        ij = (((i >> bit) & 1) << 1) | ((j >> bit) & 1)
        child_position = IJ_TO_POSITION[orientation][ij]
        position = (position << 2) | child_position
        orientation ^= POSITION_TO_ORIENTATION[child_position]
    suffix_shift = 2 * (MAX_LEVEL - level)
    return (face << 61) | (position << (suffix_shift + 1)) | (1 << suffix_shift)


def route(point: tuple[int, int, int], level: int) -> int:
    require(0 <= level <= MAX_LEVEL, f"invalid S2 level {level}")
    require(any(point), "the Earth-centre vector has no direction")
    for axis, value in zip("xyz", point):
        require(
            -MAX_ECEF_COMPONENT_MM <= value <= MAX_ECEF_COMPONENT_MM,
            f"ECEF {axis} coordinate is outside the reference envelope",
        )
    face, u_numerator, v_numerator, denominator = face_uv(point)
    i = coordinate_index(u_numerator, denominator, level)
    j = coordinate_index(v_numerator, denominator, level)
    return cell_id_from_face_ij(face, i, j, level)


def checked_integer(value: Any, label: str) -> int:
    require(type(value) is int, f"{label} must be an integer")
    return value


def main() -> None:
    project_root = Path(__file__).resolve().parents[1]
    fixture_path = project_root / "crates" / "sim-engine" / "testdata" / "ecef-s2-v1.json"
    suite = json.loads(fixture_path.read_text(encoding="utf-8"))

    require(suite["schema_version"] == 1, "unsupported fixture schema")
    require(suite["coordinate_frame"] == "EPSG:4978", "unexpected coordinate frame")
    require(suite["coordinate_unit"] == "millimetre", "unexpected coordinate unit")
    require(suite["address_bridge"] == "geocentric_ecef_ray", "unexpected address bridge")
    require(suite["face_tie_precedence"] == ["z", "y", "x"], "unexpected face tie rule")
    require(
        suite["s2_reference_revision"] == "97d76747276147afb716b1c03863ae2b3e50ed65",
        "unexpected S2 reference revision",
    )

    checked = 0
    for vector in suite["vectors"]:
        name = vector["name"]
        raw_point = vector["ecef_mm"]
        require(isinstance(raw_point, list) and len(raw_point) == 3, f"{name}: invalid point")
        point = tuple(
            checked_integer(component, f"{name} ECEF component") for component in raw_point
        )
        for encoded_level, expected_hex in vector["cells"].items():
            level = int(encoded_level)
            actual = route(point, level)
            expected = int(expected_hex, 16)
            require(
                actual == expected,
                f"{name} L{level}: expected {expected:016x}, calculated {actual:016x}",
            )
            checked += 1

    print(f"verified {checked} ECEF-to-S2 golden addresses from {fixture_path.relative_to(project_root)}")


if __name__ == "__main__":
    main()
