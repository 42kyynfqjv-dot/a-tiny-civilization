//! Private fixed-point reference routing for embodied full-Earth state.
//!
//! This module deliberately does not define a durable position, event, or snapshot
//! representation. It proves the exact ECEF-to-S2 address bridge that a later
//! embodied-state ADR can integrate without depending on floating-point behavior.

use thiserror::Error;
use world_domain::{MAX_S2_LEVEL, S2CellId, S2CellIdError};

/// The reference envelope includes every point on the WGS 84 ellipsoid with margin.
/// It is not a terrain, altitude, atmosphere, or habitat validity check.
const MAX_ECEF_COMPONENT_MM: i64 = 7_000_000_000;

// These are the S2 Hilbert orientation tables reviewed at Google S2 revision
// 97d76747276147afb716b1c03863ae2b3e50ed65. The compact iterative form avoids
// depending on a platform-specific external geometry implementation at runtime.
const SWAP_MASK: usize = 0x01;
const IJ_TO_POSITION: [[u8; 4]; 4] = [[0, 1, 3, 2], [0, 3, 1, 2], [2, 3, 1, 0], [2, 1, 3, 0]];
const POSITION_TO_ORIENTATION: [usize; 4] = [SWAP_MASK, 0, 0, 0x03];

/// One private reference position in WGS 84 ECEF (EPSG:4978) millimetres.
///
/// This is intentionally not serializable or exported from `sim-engine`. Persisting
/// it would commit event, snapshot, movement, and state-hash semantics that this
/// reference checkpoint does not yet define.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EcefPositionMm {
    x_mm: i64,
    y_mm: i64,
    z_mm: i64,
}

impl EcefPositionMm {
    pub(super) fn new(x_mm: i64, y_mm: i64, z_mm: i64) -> Result<Self, RoutingError> {
        for (axis, value) in [("x", x_mm), ("y", y_mm), ("z", z_mm)] {
            if !(-MAX_ECEF_COMPONENT_MM..=MAX_ECEF_COMPONENT_MM).contains(&value) {
                return Err(RoutingError::ComponentOutsideEnvelope {
                    axis,
                    value_mm: value,
                });
            }
        }
        if x_mm == 0 && y_mm == 0 && z_mm == 0 {
            return Err(RoutingError::EarthCenter);
        }
        Ok(Self { x_mm, y_mm, z_mm })
    }

    fn face_uv(self) -> FaceUv {
        let x = i128::from(self.x_mm);
        let y = i128::from(self.y_mm);
        let z = i128::from(self.z_mm);
        let absolute = [x.abs(), y.abs(), z.abs()];

        // This is the exact strict-comparison order used by S2's
        // Vector3::LargestAbsComponent. Therefore ties prefer Z, then Y, then X.
        let axis = if absolute[0] > absolute[1] {
            if absolute[0] > absolute[2] { 0 } else { 2 }
        } else if absolute[1] > absolute[2] {
            1
        } else {
            2
        };
        let selected = [x, y, z][axis];
        let face = axis + usize::from(selected < 0) * 3;

        let (u_numerator, v_numerator, denominator) = match face {
            0 => (y, z, x),
            1 => (-x, z, y),
            2 => (-x, -y, z),
            3 => (-z, -y, -x),
            4 => (-z, x, -y),
            5 => (y, x, -z),
            _ => unreachable!("axis and sign always select one of six S2 faces"),
        };
        debug_assert!(denominator > 0);
        debug_assert!(u_numerator.abs() <= denominator);
        debug_assert!(v_numerator.abs() <= denominator);

        FaceUv {
            face: [0_u8, 1, 2, 3, 4, 5][face],
            u_numerator,
            v_numerator,
            denominator,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FaceUv {
    face: u8,
    u_numerator: i128,
    v_numerator: i128,
    denominator: i128,
}

/// Route the geocentric ray through an ECEF point to one quadratic S2 cell.
///
/// Magnitude does not affect the result. Projection boundaries are compared as
/// exact rationals, and equality selects the higher-index half-open cell, matching
/// `floor(2^level * UVtoST(value))` without using square roots or floating point.
pub(super) fn route_ecef_to_s2(
    position: EcefPositionMm,
    level: u8,
) -> Result<S2CellId, RoutingError> {
    if level > MAX_S2_LEVEL {
        return Err(RoutingError::InvalidLevel(level));
    }

    let face_uv = position.face_uv();
    let i = quadratic_coordinate_index(face_uv.u_numerator, face_uv.denominator, level)?;
    let j = quadratic_coordinate_index(face_uv.v_numerator, face_uv.denominator, level)?;
    cell_id_from_face_ij(face_uv.face, i, j, level)
}

fn quadratic_coordinate_index(
    value_numerator: i128,
    value_denominator: i128,
    level: u8,
) -> Result<u32, RoutingError> {
    if value_denominator <= 0 || value_numerator.abs() > value_denominator {
        return Err(RoutingError::CoordinateOutsideFace);
    }

    let size = 1_u64
        .checked_shl(u32::from(level))
        .ok_or(RoutingError::ArithmeticOverflow)?;
    let mut lower = 0_u64;
    let mut upper = size;
    while lower < upper {
        let midpoint = lower + (upper - lower).div_ceil(2);
        if quadratic_boundary_is_at_or_below(midpoint, size, value_numerator, value_denominator)? {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }

    let index = lower.min(size - 1);
    u32::try_from(index).map_err(|_| RoutingError::ArithmeticOverflow)
}

fn quadratic_boundary_is_at_or_below(
    boundary_index: u64,
    size: u64,
    value_numerator: i128,
    value_denominator: i128,
) -> Result<bool, RoutingError> {
    let index = i128::from(boundary_index);
    let size = i128::from(size);
    let size_squared = size
        .checked_mul(size)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    let boundary_numerator = if index
        .checked_mul(2)
        .ok_or(RoutingError::ArithmeticOverflow)?
        >= size
    {
        index
            .checked_mul(index)
            .and_then(|value| value.checked_mul(4))
            .and_then(|value| value.checked_sub(size_squared))
            .ok_or(RoutingError::ArithmeticOverflow)?
    } else {
        let complement = size
            .checked_sub(index)
            .ok_or(RoutingError::ArithmeticOverflow)?;
        size_squared
            .checked_sub(
                complement
                    .checked_mul(complement)
                    .and_then(|value| value.checked_mul(4))
                    .ok_or(RoutingError::ArithmeticOverflow)?,
            )
            .ok_or(RoutingError::ArithmeticOverflow)?
    };
    let boundary_denominator = size_squared
        .checked_mul(3)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    let left = boundary_numerator
        .checked_mul(value_denominator)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    let right = value_numerator
        .checked_mul(boundary_denominator)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    Ok(left <= right)
}

fn cell_id_from_face_ij(face: u8, i: u32, j: u32, level: u8) -> Result<S2CellId, RoutingError> {
    let mut orientation = usize::from(face) & SWAP_MASK;
    let mut position = 0_u64;
    for bit in (0..level).rev() {
        let i_bit = usize::from(((i >> bit) & 1) != 0);
        let j_bit = usize::from(((j >> bit) & 1) != 0);
        let ij = i_bit * 2 + j_bit;
        let child_position = usize::from(IJ_TO_POSITION[orientation][ij]);
        position = position
            .checked_shl(2)
            .and_then(|value| value.checked_add(child_position as u64))
            .ok_or(RoutingError::ArithmeticOverflow)?;
        orientation ^= POSITION_TO_ORIENTATION[child_position];
    }

    let suffix_shift = 2 * u32::from(MAX_S2_LEVEL - level);
    let face_bits = u64::from(face)
        .checked_shl(61)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    let position_bits = position
        .checked_shl(suffix_shift + 1)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    let sentinel = 1_u64
        .checked_shl(suffix_shift)
        .ok_or(RoutingError::ArithmeticOverflow)?;
    S2CellId::new(face_bits | position_bits | sentinel).map_err(RoutingError::S2)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(super) enum RoutingError {
    #[error("ECEF {axis} coordinate {value_mm} mm exceeds the reference envelope")]
    ComponentOutsideEnvelope { axis: &'static str, value_mm: i64 },
    #[error("the Earth-centre ECEF vector has no routable direction")]
    EarthCenter,
    #[error("S2 level {0} is outside 0..=30")]
    InvalidLevel(u8),
    #[error("the projected coordinate lies outside its selected S2 face")]
    CoordinateOutsideFace,
    #[error("fixed-point S2 routing arithmetic overflowed")]
    ArithmeticOverflow,
    #[error(transparent)]
    S2(#[from] S2CellIdError),
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, str::FromStr};

    use serde::Deserialize;

    use super::*;

    const GOLDEN_VECTORS: &str = include_str!("../testdata/ecef-s2-v1.json");

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GoldenSuite {
        schema_version: u16,
        coordinate_frame: String,
        coordinate_unit: String,
        address_bridge: String,
        face_tie_precedence: Vec<String>,
        s2_reference_revision: String,
        vectors: Vec<GoldenVector>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GoldenVector {
        name: String,
        ecef_mm: [i64; 3],
        cells: BTreeMap<String, String>,
    }

    fn golden_suite() -> GoldenSuite {
        serde_json::from_str(GOLDEN_VECTORS).expect("valid checked-in ECEF/S2 fixture")
    }

    #[test]
    fn checked_in_golden_vectors_match_exact_cell_ids() {
        let suite = golden_suite();
        assert_eq!(suite.schema_version, 1);
        assert_eq!(suite.coordinate_frame, "EPSG:4978");
        assert_eq!(suite.coordinate_unit, "millimetre");
        assert_eq!(suite.address_bridge, "geocentric_ecef_ray");
        assert_eq!(suite.face_tie_precedence, ["z", "y", "x"]);
        assert_eq!(
            suite.s2_reference_revision,
            "97d76747276147afb716b1c03863ae2b3e50ed65"
        );

        for vector in suite.vectors {
            let [x, y, z] = vector.ecef_mm;
            let position = EcefPositionMm::new(x, y, z).expect("golden position is valid");
            for (level, expected) in vector.cells {
                let level = u8::from_str(&level).expect("golden level is an integer");
                let actual = route_ecef_to_s2(position, level)
                    .unwrap_or_else(|error| panic!("{} at level {level}: {error}", vector.name));
                assert_eq!(actual.to_string(), expected, "{} at L{level}", vector.name);
                assert_eq!(actual.level(), level, "{} at L{level}", vector.name);
            }
        }
    }

    #[test]
    fn configured_causal_levels_share_exact_ancestors() {
        let points = [
            EcefPositionMm::new(6_378_137_000, 1_111_111_111, 222_222_222).expect("valid point"),
            EcefPositionMm::new(-1_111_111_111, -222_222_222, 6_378_137_000).expect("valid point"),
            EcefPositionMm::new(1_111_111_111, 222_222_222, -6_378_137_000).expect("valid point"),
        ];
        let levels = [10, 14, 18, 23];

        for point in points {
            let finest = route_ecef_to_s2(point, 23).expect("valid finest routing");
            for level in levels {
                let direct = route_ecef_to_s2(point, level).expect("valid direct routing");
                assert_eq!(finest.ancestor(level), Ok(direct));
            }
        }
    }

    #[test]
    fn positive_vector_scaling_does_not_change_an_address() {
        let base = [123_456_789_i64, -234_567_891, 345_678_912];
        let expected = route_ecef_to_s2(
            EcefPositionMm::new(base[0], base[1], base[2]).expect("valid base point"),
            23,
        )
        .expect("valid base routing");

        for scale in [2_i64, 7, 19] {
            let scaled = EcefPositionMm::new(base[0] * scale, base[1] * scale, base[2] * scale)
                .expect("scaled point stays in the envelope");
            assert_eq!(route_ecef_to_s2(scaled, 23), Ok(expected));
        }
    }

    #[test]
    fn invalid_positions_and_levels_fail_closed() {
        assert!(matches!(
            EcefPositionMm::new(0, 0, 0),
            Err(RoutingError::EarthCenter)
        ));
        assert!(matches!(
            EcefPositionMm::new(MAX_ECEF_COMPONENT_MM + 1, 0, 0),
            Err(RoutingError::ComponentOutsideEnvelope { axis: "x", .. })
        ));

        let point =
            EcefPositionMm::new(MAX_ECEF_COMPONENT_MM, 0, 0).expect("envelope boundary is valid");
        assert!(matches!(
            route_ecef_to_s2(point, MAX_S2_LEVEL + 1),
            Err(RoutingError::InvalidLevel(31))
        ));
    }
}
