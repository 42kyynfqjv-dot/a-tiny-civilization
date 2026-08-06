"""Independently verify fixed-point geographic-to-S2 golden vectors.

Uses only Python integers and the separately maintained ECEF-to-S2 verifier. Runtime
does not call Rust, floating point, or a GIS library.
"""

from __future__ import annotations

import json
import runpy
from pathlib import Path


ANGLE_SCALE = 1 << 62
QUARTER_TURN = ANGLE_SCALE // 4
HALF_TURN = ANGLE_SCALE // 2
DEGREES_E7_PER_TURN = 3_600_000_000
ECEF_SCALE_MM = 6_400_000_000
FLATTENING_DENOMINATOR = 298_257_223_563
FLATTENING_NUMERATOR = 1_000_000_000
CORDIC_GAIN_INVERSE_Q62 = 2_800_459_870_029_452_800
CORDIC_ATAN_TURNS_Q62 = (
    576460752303423488, 340304653033718272, 179807632645220256, 91273161881380496,
    45813697873323712, 22929182573009056, 11467389120678284, 5734044481687724,
    2867065987018958, 1433538461969102, 716769914547871, 358385042719534,
    179192532040472, 89596267355325, 44798133844548, 22399066943135,
    11199533474175, 5599766737413, 2799883368747, 1399941684379,
    699970842190, 349985421095, 174992710548, 87496355274, 43748177637,
    21874088818, 10937044409, 5468522205, 2734261102, 1367130551,
    683565276, 341782638, 170891319, 85445659, 42722830, 21361415,
    10680707, 5340354, 2670177, 1335088, 667544, 333772, 166886, 83443,
    41722, 20861, 10430, 5215,
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def round_div(numerator: int, denominator: int) -> int:
    return (numerator + denominator // 2) // denominator if numerator >= 0 else (numerator - denominator // 2) // denominator


def degrees_e7_to_turns(value: int) -> int:
    return round_div(value * ANGLE_SCALE, DEGREES_E7_PER_TURN)


def cordic_sin_cos(angle: int) -> tuple[int, int]:
    x, y, remaining = CORDIC_GAIN_INVERSE_Q62, 0, angle
    for index, atan in enumerate(CORDIC_ATAN_TURNS_Q62):
        shifted_x, shifted_y = x >> index, y >> index
        if remaining > 0:
            x, y, remaining = x - shifted_y, y + shifted_x, remaining - atan
        else:
            x, y, remaining = x + shifted_y, y - shifted_x, remaining + atan
    return y, x


def sin_cos_turns(angle: int) -> tuple[int, int]:
    reduced = angle % ANGLE_SCALE
    if reduced >= HALF_TURN:
        reduced -= ANGLE_SCALE
    negate = False
    if reduced > QUARTER_TURN:
        reduced -= HALF_TURN
        negate = True
    if reduced < -QUARTER_TURN:
        reduced += HALF_TURN
        negate = True
    sine, cosine = cordic_sin_cos(reduced)
    return (-sine, -cosine) if negate else (sine, cosine)


def scale_product(left: int, right: int) -> int:
    return ((left * right) // ANGLE_SCALE * ECEF_SCALE_MM) // ANGLE_SCALE


def scale_single(value: int) -> int:
    return value * ECEF_SCALE_MM // ANGLE_SCALE


def geographic_ecef(latitude_e7: int, longitude_e7: int) -> tuple[int, int, int]:
    sine_latitude, cosine_latitude = sin_cos_turns(degrees_e7_to_turns(latitude_e7))
    sine_longitude, cosine_longitude = sin_cos_turns(degrees_e7_to_turns(longitude_e7))
    retained_axis = FLATTENING_DENOMINATOR - FLATTENING_NUMERATOR
    return (
        scale_product(cosine_latitude, cosine_longitude),
        scale_product(cosine_latitude, sine_longitude),
        scale_single(sine_latitude) * retained_axis * retained_axis // (FLATTENING_DENOMINATOR * FLATTENING_DENOMINATOR),
    )


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    routing = runpy.run_path(root / "scripts" / "verify-s2-routing.py")
    route = routing["route"]
    fixture_path = root / "crates" / "world-domain" / "testdata" / "geographic-s2-v1.json"
    suite = json.loads(fixture_path.read_text(encoding="utf-8"))
    require(suite["schema_version"] == 1, "unsupported fixture schema")
    require(suite["coordinate_frame"] == "WGS84 geodetic e7 degree", "unexpected coordinate frame")
    require(suite["address_bridge"] == "wgs84_ecef_geocentric_ray", "unexpected bridge")
    require(suite["angle_algorithm"] == "cordic_q62_turns_v1", "unexpected angle algorithm")
    checked = 0
    for vector in suite["vectors"]:
        latitude, longitude = vector["latitude_e7"], vector["longitude_e7"]
        require(type(latitude) is int and -900_000_000 <= latitude <= 900_000_000, "invalid latitude")
        require(type(longitude) is int and -1_800_000_000 <= longitude < 1_800_000_000, "invalid longitude")
        point = geographic_ecef(latitude, longitude)
        for level, expected in vector["cells"].items():
            actual = route(point, int(level))
            require(actual == int(expected, 16), f"{vector['name']} L{level}: expected {expected}, got {actual:016x}")
            checked += 1
    print(f"verified {checked} geographic-to-S2 golden addresses from {fixture_path.relative_to(root)}")


if __name__ == "__main__":
    main()
