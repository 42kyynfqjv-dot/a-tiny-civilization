//! Fixed-point geographic-coordinate routing for scientific source normalization.
//!
//! This module has no platform floating-point, GIS, or wall-clock dependency. It is
//! intentionally limited to source coordinates and S2 addresses; it does not define
//! a durable organism position or movement semantics.

use thiserror::Error;

use crate::{MAX_S2_LEVEL, S2CellId, S2CellIdError};

const ANGLE_SCALE: i64 = 1_i64 << 62;
const QUARTER_TURN: i64 = ANGLE_SCALE / 4;
const HALF_TURN: i64 = ANGLE_SCALE / 2;
const DEGREES_E7_PER_TURN: i64 = 3_600_000_000;
const HALF_ARCSECONDS_PER_TURN: i64 = 2_592_000;
const ECEF_SCALE_MM: i64 = 6_400_000_000;
const WGS84_FLATTENING_DENOMINATOR: i128 = 298_257_223_563;
const WGS84_FLATTENING_NUMERATOR: i128 = 1_000_000_000;
const CORDIC_GAIN_INVERSE_Q62: i64 = 2_800_459_870_029_452_800;

// atan(2^-i) / tau, rounded to the nearest Q62 turn. Constants are generated once
// from the mathematical definition and retained as source, so runtime never calls
// host trigonometry.
const CORDIC_ATAN_TURNS_Q62: [i64; 48] = [
    576460752303423488,
    340304653033718272,
    179807632645220256,
    91273161881380496,
    45813697873323712,
    22929182573009056,
    11467389120678284,
    5734044481687724,
    2867065987018958,
    1433538461969102,
    716769914547871,
    358385042719534,
    179192532040472,
    89596267355325,
    44798133844548,
    22399066943135,
    11199533474175,
    5599766737413,
    2799883368747,
    1399941684379,
    699970842190,
    349985421095,
    174992710548,
    87496355274,
    43748177637,
    21874088818,
    10937044409,
    5468522205,
    2734261102,
    1367130551,
    683565276,
    341782638,
    170891319,
    85445659,
    42722830,
    21361415,
    10680707,
    5340354,
    2670177,
    1335088,
    667544,
    333772,
    166886,
    83443,
    41722,
    20861,
    10430,
    5215,
];

/// A WGS 84 geodetic coordinate in exact 10^-7 degree units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeographicCoordinateE7 {
    latitude_e7: i32,
    longitude_e7: i32,
}

/// Discrete S2 face coordinates decoded from a structurally valid CellId.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct S2FaceIj {
    pub face: u8,
    pub i: u32,
    pub j: u32,
    pub level: u8,
}

/// Exact rational face coordinates at the geometric centre of an S2 IJ cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct S2FaceUv {
    pub face: u8,
    pub u_numerator: i128,
    pub v_numerator: i128,
    pub denominator: i128,
}

/// An unnormalised rational 3D direction; scale is irrelevant to S2 face selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct S2FaceRay {
    pub x: i128,
    pub y: i128,
    pub z: i128,
}

/// A WGS 84 coordinate in exact half-arcsecond units.
///
/// ETOPO 2022's 60-arc-second `Area` cells have centers on this lattice. Keeping this
/// representation avoids silently rounding a source cell centre to decimal degrees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeographicCoordinateHalfArcsecond {
    latitude_half_arcseconds: i32,
    longitude_half_arcseconds: i32,
}

impl GeographicCoordinateHalfArcsecond {
    pub fn new(
        latitude_half_arcseconds: i32,
        longitude_half_arcseconds: i32,
    ) -> Result<Self, GeographicRoutingError> {
        if !(-648_000..=648_000).contains(&latitude_half_arcseconds) {
            return Err(GeographicRoutingError::LatitudeHalfArcsecondsOutOfRange(
                latitude_half_arcseconds,
            ));
        }
        if !(-1_296_000..1_296_000).contains(&longitude_half_arcseconds) {
            return Err(GeographicRoutingError::LongitudeHalfArcsecondsOutOfRange(
                longitude_half_arcseconds,
            ));
        }
        Ok(Self {
            latitude_half_arcseconds,
            longitude_half_arcseconds,
        })
    }

    #[must_use]
    pub const fn latitude_half_arcseconds(self) -> i32 {
        self.latitude_half_arcseconds
    }

    #[must_use]
    pub const fn longitude_half_arcseconds(self) -> i32 {
        self.longitude_half_arcseconds
    }
}

impl GeographicCoordinateE7 {
    /// Longitude uses the half-open interval [-180°, 180°); latitude is closed.
    pub fn new(latitude_e7: i32, longitude_e7: i32) -> Result<Self, GeographicRoutingError> {
        if !(-900_000_000..=900_000_000).contains(&latitude_e7) {
            return Err(GeographicRoutingError::LatitudeOutOfRange(latitude_e7));
        }
        if !(-1_800_000_000..1_800_000_000).contains(&longitude_e7) {
            return Err(GeographicRoutingError::LongitudeOutOfRange(longitude_e7));
        }
        Ok(Self {
            latitude_e7,
            longitude_e7,
        })
    }

    #[must_use]
    pub const fn latitude_e7(self) -> i32 {
        self.latitude_e7
    }

    #[must_use]
    pub const fn longitude_e7(self) -> i32 {
        self.longitude_e7
    }
}

/// Route a WGS 84 geodetic coordinate via its geocentric ECEF ray to an S2 CellId.
pub fn route_geographic_to_s2(
    coordinate: GeographicCoordinateE7,
    level: u8,
) -> Result<S2CellId, GeographicRoutingError> {
    route_turns_to_s2(
        degrees_e7_to_turns(coordinate.latitude_e7),
        degrees_e7_to_turns(coordinate.longitude_e7),
        level,
    )
}

/// Route an exact half-arcsecond WGS 84 coordinate via its ECEF ray to an S2 CellId.
pub fn route_half_arcsecond_to_s2(
    coordinate: GeographicCoordinateHalfArcsecond,
    level: u8,
) -> Result<S2CellId, GeographicRoutingError> {
    route_turns_to_s2(
        half_arcseconds_to_turns(coordinate.latitude_half_arcseconds),
        half_arcseconds_to_turns(coordinate.longitude_half_arcseconds),
        level,
    )
}

/// Decode the exact Hilbert-position bits of a CellId back to its face/IJ cell.
/// This is the discrete first half of inverse S2 geometry; it performs no floating
/// point coordinate conversion.
pub fn decode_s2_face_ij(cell: S2CellId) -> S2FaceIj {
    const SWAP: usize = 1;
    const IJ_TO_POS: [[u8; 4]; 4] = [[0, 1, 3, 2], [0, 3, 1, 2], [2, 3, 1, 0], [2, 1, 3, 0]];
    const POS_TO_ORIENTATION: [usize; 4] = [SWAP, 0, 0, 3];
    let level = cell.level();
    let mut orientation = usize::from(cell.face()) & SWAP;
    let (mut i, mut j) = (0_u32, 0_u32);
    for depth in 0..level {
        let shift = 2 * u32::from(MAX_S2_LEVEL - depth - 1) + 1;
        let position = usize::try_from((cell.get() >> shift) & 3).expect("two bits fit usize");
        let ij = IJ_TO_POS[orientation]
            .iter()
            .position(|candidate| usize::from(*candidate) == position)
            .expect("S2 orientation contains every position");
        i = (i << 1) | u32::try_from(ij >> 1).expect("IJ bit fits u32");
        j = (j << 1) | u32::try_from(ij & 1).expect("IJ bit fits u32");
        orientation ^= POS_TO_ORIENTATION[position];
    }
    S2FaceIj {
        face: cell.face(),
        i,
        j,
        level,
    }
}

/// Return down, right, up, and left neighbors at the cell's own S2 level.
/// Cross-face neighbors use S2's linear face-wrap projection before routing back
/// through the exact Hilbert encoder.
pub fn s2_edge_neighbors(cell: S2CellId) -> Result<[S2CellId; 4], GeographicRoutingError> {
    let ij = decode_s2_face_ij(cell);
    let leaf_size = 1_i64 << u32::from(MAX_S2_LEVEL - ij.level);
    let i = i64::from(ij.i) * leaf_size;
    let j = i64::from(ij.j) * leaf_size;
    Ok([
        face_ij_wrap(ij.face, i, j - leaf_size, ij.level)?,
        face_ij_wrap(ij.face, i + leaf_size, j, ij.level)?,
        face_ij_wrap(ij.face, i, j + leaf_size, ij.level)?,
        face_ij_wrap(ij.face, i - leaf_size, j, ij.level)?,
    ])
}

fn face_ij_wrap(face: u8, i: i64, j: i64, level: u8) -> Result<S2CellId, GeographicRoutingError> {
    const MAX_SIZE: i64 = 1_i64 << MAX_S2_LEVEL;
    let i = i.clamp(-1, MAX_SIZE);
    let j = j.clamp(-1, MAX_SIZE);
    // This is S2CellId::FromFaceIJWrap's linear face projection, expressed as
    // integer homogeneous UV coordinates rather than binary floating point.
    let ray = s2_face_uv_to_ray(S2FaceUv {
        face,
        u_numerator: i128::from(2 * (i - MAX_SIZE / 2) + 1),
        v_numerator: i128::from(2 * (j - MAX_SIZE / 2) + 1),
        denominator: i128::from(MAX_SIZE),
    })?;
    let (face, u, v, d) = face_uv_from_ray(ray)?;
    let leaf_i = linear_uv_to_leaf(u, d)?;
    let leaf_j = linear_uv_to_leaf(v, d)?;
    cell_id_from_face_ij(face, leaf_i, leaf_j, MAX_S2_LEVEL)?
        .ancestor(level)
        .map_err(GeographicRoutingError::S2)
}

fn face_uv_from_ray(ray: S2FaceRay) -> Result<(u8, i128, i128, i128), GeographicRoutingError> {
    let values = [ray.x, ray.y, ray.z];
    let absolute = [ray.x.abs(), ray.y.abs(), ray.z.abs()];
    let axis = if absolute[0] > absolute[1] {
        if absolute[0] > absolute[2] { 0 } else { 2 }
    } else if absolute[1] > absolute[2] {
        1
    } else {
        2
    };
    let face = axis + usize::from(values[axis] < 0) * 3;
    let (u, v, d) = match face {
        0 => (ray.y, ray.z, ray.x),
        1 => (-ray.x, ray.z, ray.y),
        2 => (-ray.x, -ray.y, ray.z),
        3 => (-ray.z, -ray.y, -ray.x),
        4 => (-ray.z, ray.x, -ray.y),
        5 => (ray.y, ray.x, -ray.z),
        _ => unreachable!(),
    };
    Ok((
        u8::try_from(face).map_err(|_| GeographicRoutingError::Overflow)?,
        u,
        v,
        d,
    ))
}

fn linear_uv_to_leaf(numerator: i128, denominator: i128) -> Result<u32, GeographicRoutingError> {
    const MAX_SIZE: i128 = 1_i128 << MAX_S2_LEVEL;
    let scaled = numerator
        .checked_add(denominator)
        .and_then(|value| value.checked_mul(MAX_SIZE))
        .ok_or(GeographicRoutingError::Overflow)?;
    let divisor = denominator
        .checked_mul(2)
        .ok_or(GeographicRoutingError::Overflow)?;
    u32::try_from((scaled / divisor).clamp(0, MAX_SIZE - 1))
        .map_err(|_| GeographicRoutingError::Overflow)
}

/// Apply S2's quadratic projection to the exact centre of a decoded IJ cell.
pub fn s2_face_ij_center_uv(ij: S2FaceIj) -> Result<S2FaceUv, GeographicRoutingError> {
    let size = 1_i128
        .checked_shl(u32::from(ij.level))
        .ok_or(GeographicRoutingError::Overflow)?;
    let denominator = size
        .checked_mul(size)
        .and_then(|value| value.checked_mul(3))
        .ok_or(GeographicRoutingError::Overflow)?;
    let coordinate = |index: u32| -> Result<i128, GeographicRoutingError> {
        let doubled = i128::from(index)
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(GeographicRoutingError::Overflow)?;
        let squared_size = size
            .checked_mul(size)
            .ok_or(GeographicRoutingError::Overflow)?;
        if doubled >= size {
            doubled
                .checked_mul(doubled)
                .and_then(|value| value.checked_sub(squared_size))
                .ok_or(GeographicRoutingError::Overflow)
        } else {
            let complement = size
                .checked_mul(2)
                .and_then(|value| value.checked_sub(doubled))
                .ok_or(GeographicRoutingError::Overflow)?;
            squared_size
                .checked_sub(
                    complement
                        .checked_mul(complement)
                        .ok_or(GeographicRoutingError::Overflow)?,
                )
                .ok_or(GeographicRoutingError::Overflow)
        }
    };
    Ok(S2FaceUv {
        face: ij.face,
        u_numerator: coordinate(ij.i)?,
        v_numerator: coordinate(ij.j)?,
        denominator,
    })
}

/// Apply S2's quadratic projection to an exact decoded-cell vertex. Vertex indices
/// use the closed range 0..=2^level, so callers can construct conservative cell
/// bounds without host floating point.
pub fn s2_face_ij_vertex_uv(
    ij: S2FaceIj,
    i_vertex: u32,
    j_vertex: u32,
) -> Result<S2FaceUv, GeographicRoutingError> {
    let size = 1_i128
        .checked_shl(u32::from(ij.level))
        .ok_or(GeographicRoutingError::Overflow)?;
    if i128::from(i_vertex) > size || i128::from(j_vertex) > size {
        return Err(GeographicRoutingError::Overflow);
    }
    let denominator = size
        .checked_mul(size)
        .and_then(|value| value.checked_mul(3))
        .ok_or(GeographicRoutingError::Overflow)?;
    let coordinate = |vertex: u32| -> Result<i128, GeographicRoutingError> {
        let doubled = i128::from(vertex)
            .checked_mul(2)
            .ok_or(GeographicRoutingError::Overflow)?;
        let squared_size = size
            .checked_mul(size)
            .ok_or(GeographicRoutingError::Overflow)?;
        if doubled >= size {
            doubled
                .checked_mul(doubled)
                .and_then(|value| value.checked_sub(squared_size))
                .ok_or(GeographicRoutingError::Overflow)
        } else {
            let complement = size
                .checked_mul(2)
                .and_then(|value| value.checked_sub(doubled))
                .ok_or(GeographicRoutingError::Overflow)?;
            squared_size
                .checked_sub(
                    complement
                        .checked_mul(complement)
                        .ok_or(GeographicRoutingError::Overflow)?,
                )
                .ok_or(GeographicRoutingError::Overflow)
        }
    };
    Ok(S2FaceUv {
        face: ij.face,
        u_numerator: coordinate(i_vertex)?,
        v_numerator: coordinate(j_vertex)?,
        denominator,
    })
}

/// Convert an S2 face coordinate into its exact unnormalised 3D direction.
pub fn s2_face_uv_to_ray(uv: S2FaceUv) -> Result<S2FaceRay, GeographicRoutingError> {
    let (u, v, d) = (uv.u_numerator, uv.v_numerator, uv.denominator);
    let [x, y, z] = match uv.face {
        0 => [d, u, v],
        1 => [-u, d, v],
        2 => [-u, -v, d],
        3 => [-d, -v, -u],
        4 => [v, -d, -u],
        5 => [v, u, -d],
        _ => return Err(GeographicRoutingError::InvalidFace(uv.face)),
    };
    Ok(S2FaceRay { x, y, z })
}

/// Convert a rational ray to the same exact-E7 geographic representation accepted
/// by forward routing. Values are reduced before the WGS84 correction to keep all
/// intermediate integer products bounded.
pub fn s2_ray_to_geographic_e7(
    ray: S2FaceRay,
) -> Result<GeographicCoordinateE7, GeographicRoutingError> {
    if ray.x == 0 && ray.y == 0 && ray.z == 0 {
        return Err(GeographicRoutingError::ZeroRay);
    }
    let mut scale = ray.x.abs().max(ray.y.abs()).max(ray.z.abs());
    let mut shift = 0_u32;
    while scale > 1_000_000_000_000 {
        scale >>= 1;
        shift += 1;
    }
    let x = ray.x >> shift;
    let y = ray.y >> shift;
    let z = ray.z >> shift;
    let longitude = atan2_turns_q62(y, x)?;
    let horizontal = integer_hypot(x, y)?;
    let latitude = atan2_turns_q62(
        z.checked_mul(WGS84_FLATTENING_DENOMINATOR)
            .and_then(|value| value.checked_mul(WGS84_FLATTENING_DENOMINATOR))
            .ok_or(GeographicRoutingError::Overflow)?,
        horizontal
            .checked_mul(WGS84_FLATTENING_DENOMINATOR - WGS84_FLATTENING_NUMERATOR)
            .and_then(|value| {
                value.checked_mul(WGS84_FLATTENING_DENOMINATOR - WGS84_FLATTENING_NUMERATOR)
            })
            .ok_or(GeographicRoutingError::Overflow)?,
    )?;
    GeographicCoordinateE7::new(
        turns_to_degrees_e7(latitude),
        turns_to_degrees_e7(longitude),
    )
}

fn atan2_turns_q62(y: i128, x: i128) -> Result<i64, GeographicRoutingError> {
    if x == 0 && y == 0 {
        return Err(GeographicRoutingError::ZeroRay);
    }
    // CORDIC's late iterations need fractional precision.  Rays may be tiny
    // integer ratios (for example the cardinal axes), so normalize their scale
    // before vectoring rather than letting right shifts erase the vector.
    let magnitude = x
        .checked_abs()
        .and_then(|x| y.checked_abs().map(|y| x.max(y)))
        .ok_or(GeographicRoutingError::Overflow)?;
    let bits = 128_u32 - magnitude.leading_zeros();
    const VECTORING_BITS: u32 = 100;
    let (mut x, mut y) = if bits < VECTORING_BITS {
        let shift = VECTORING_BITS - bits;
        (
            x.checked_shl(shift)
                .ok_or(GeographicRoutingError::Overflow)?,
            y.checked_shl(shift)
                .ok_or(GeographicRoutingError::Overflow)?,
        )
    } else {
        let shift = bits - VECTORING_BITS;
        (x >> shift, y >> shift)
    };
    let mut angle = 0_i128;
    if x < 0 {
        let original_y = y;
        x = -x;
        y = -y;
        angle = if original_y >= 0 {
            i128::from(HALF_TURN)
        } else {
            -i128::from(HALF_TURN)
        };
    }
    if y == 0 {
        return i64::try_from(angle).map_err(|_| GeographicRoutingError::Overflow);
    }
    for (index, atan) in CORDIC_ATAN_TURNS_Q62.iter().enumerate() {
        let (shifted_x, shifted_y) = (x >> index, y >> index);
        if y > 0 {
            x += shifted_y;
            y -= shifted_x;
            angle += i128::from(*atan);
        } else {
            x -= shifted_y;
            y += shifted_x;
            angle -= i128::from(*atan);
        }
    }
    i64::try_from(angle).map_err(|_| GeographicRoutingError::Overflow)
}

fn integer_hypot(x: i128, y: i128) -> Result<i128, GeographicRoutingError> {
    let squared = x
        .checked_mul(x)
        .and_then(|value| value.checked_add(y.checked_mul(y)?))
        .ok_or(GeographicRoutingError::Overflow)?;
    let mut lower = 0_i128;
    let mut upper = squared.max(1);
    while lower < upper {
        let midpoint = lower + (upper - lower + 1) / 2;
        if midpoint > squared / midpoint {
            upper = midpoint - 1;
        } else {
            lower = midpoint;
        }
    }
    Ok(lower)
}

fn turns_to_degrees_e7(turns: i64) -> i32 {
    let numerator = i128::from(turns) * i128::from(DEGREES_E7_PER_TURN);
    let half = i128::from(ANGLE_SCALE) / 2;
    i32::try_from(if numerator >= 0 {
        (numerator + half) / i128::from(ANGLE_SCALE)
    } else {
        (numerator - half) / i128::from(ANGLE_SCALE)
    })
    .expect("Q62 turn fits geographic E7 domain")
}

fn route_turns_to_s2(
    latitude_turns: i64,
    longitude_turns: i64,
    level: u8,
) -> Result<S2CellId, GeographicRoutingError> {
    if level > MAX_S2_LEVEL {
        return Err(GeographicRoutingError::InvalidLevel(level));
    }
    let (sin_latitude, cos_latitude) = sin_cos_turns_q62(latitude_turns);
    let (sin_longitude, cos_longitude) = sin_cos_turns_q62(longitude_turns);
    let x = scale_product(cos_latitude, cos_longitude)?;
    let y = scale_product(cos_latitude, sin_longitude)?;
    let unflattened_z = scale_single(sin_latitude)?;
    let retained_axis = WGS84_FLATTENING_DENOMINATOR - WGS84_FLATTENING_NUMERATOR;
    let z = i64::try_from(
        i128::from(unflattened_z)
            .checked_mul(
                retained_axis
                    .checked_mul(retained_axis)
                    .ok_or(GeographicRoutingError::Overflow)?,
            )
            .ok_or(GeographicRoutingError::Overflow)?
            / WGS84_FLATTENING_DENOMINATOR
                .checked_mul(WGS84_FLATTENING_DENOMINATOR)
                .ok_or(GeographicRoutingError::Overflow)?,
    )
    .map_err(|_| GeographicRoutingError::Overflow)?;
    route_ecef_ray_to_s2(x, y, z, level)
}

fn degrees_e7_to_turns(value: i32) -> i64 {
    let numerator = i128::from(value) * i128::from(ANGLE_SCALE);
    let denominator = i128::from(DEGREES_E7_PER_TURN);
    let half = denominator / 2;
    let rounded = if numerator >= 0 {
        (numerator + half) / denominator
    } else {
        (numerator - half) / denominator
    };
    i64::try_from(rounded).expect("geographic coordinate domain fits Q62 turns")
}

fn half_arcseconds_to_turns(value: i32) -> i64 {
    let numerator = i128::from(value) * i128::from(ANGLE_SCALE);
    let denominator = i128::from(HALF_ARCSECONDS_PER_TURN);
    let half = denominator / 2;
    let rounded = if numerator >= 0 {
        (numerator + half) / denominator
    } else {
        (numerator - half) / denominator
    };
    i64::try_from(rounded).expect("half-arcsecond coordinate domain fits Q62 turns")
}

fn scale_product(left: i64, right: i64) -> Result<i64, GeographicRoutingError> {
    let normalized = i128::from(left)
        .checked_mul(i128::from(right))
        .ok_or(GeographicRoutingError::Overflow)?
        / i128::from(ANGLE_SCALE);
    i64::try_from(
        normalized
            .checked_mul(i128::from(ECEF_SCALE_MM))
            .ok_or(GeographicRoutingError::Overflow)?
            / i128::from(ANGLE_SCALE),
    )
    .map_err(|_| GeographicRoutingError::Overflow)
}

fn scale_single(value: i64) -> Result<i64, GeographicRoutingError> {
    i64::try_from(
        i128::from(value)
            .checked_mul(i128::from(ECEF_SCALE_MM))
            .ok_or(GeographicRoutingError::Overflow)?
            / i128::from(ANGLE_SCALE),
    )
    .map_err(|_| GeographicRoutingError::Overflow)
}

fn sin_cos_turns_q62(angle: i64) -> (i64, i64) {
    let mut reduced = angle.rem_euclid(ANGLE_SCALE);
    if reduced >= HALF_TURN {
        reduced -= ANGLE_SCALE;
    }
    let mut negate = false;
    if reduced > QUARTER_TURN {
        reduced -= HALF_TURN;
        negate = true;
    }
    if reduced < -QUARTER_TURN {
        reduced += HALF_TURN;
        negate = true;
    }
    let (sin, cos) = cordic_sin_cos(reduced);
    if negate { (-sin, -cos) } else { (sin, cos) }
}

fn cordic_sin_cos(angle: i64) -> (i64, i64) {
    let (mut x, mut y, mut remaining) = (
        i128::from(CORDIC_GAIN_INVERSE_Q62),
        0_i128,
        i128::from(angle),
    );
    for (index, atan) in CORDIC_ATAN_TURNS_Q62.iter().enumerate() {
        let shifted_x = x >> index;
        let shifted_y = y >> index;
        if remaining > 0 {
            x -= shifted_y;
            y += shifted_x;
            remaining -= i128::from(*atan);
        } else {
            x += shifted_y;
            y -= shifted_x;
            remaining += i128::from(*atan);
        }
    }
    (
        i64::try_from(y).expect("CORDIC sine fits Q62"),
        i64::try_from(x).expect("CORDIC cosine fits Q62"),
    )
}

fn route_ecef_ray_to_s2(
    x: i64,
    y: i64,
    z: i64,
    level: u8,
) -> Result<S2CellId, GeographicRoutingError> {
    if x == 0 && y == 0 && z == 0 {
        return Err(GeographicRoutingError::ZeroRay);
    }
    let [x, y, z] = [i128::from(x), i128::from(y), i128::from(z)];
    let absolute = [x.abs(), y.abs(), z.abs()];
    let axis = if absolute[0] > absolute[1] {
        if absolute[0] > absolute[2] { 0 } else { 2 }
    } else if absolute[1] > absolute[2] {
        1
    } else {
        2
    };
    let selected = [x, y, z][axis];
    let face = axis + usize::from(selected < 0) * 3;
    let (u, v, d) = match face {
        0 => (y, z, x),
        1 => (-x, z, y),
        2 => (-x, -y, z),
        3 => (-z, -y, -x),
        4 => (-z, x, -y),
        5 => (y, x, -z),
        _ => unreachable!(),
    };
    let i = quadratic_coordinate_index(u, d, level)?;
    let j = quadratic_coordinate_index(v, d, level)?;
    cell_id_from_face_ij(
        u8::try_from(face).map_err(|_| GeographicRoutingError::Overflow)?,
        i,
        j,
        level,
    )
}

fn quadratic_coordinate_index(n: i128, d: i128, level: u8) -> Result<u32, GeographicRoutingError> {
    let size = 1_u64
        .checked_shl(u32::from(level))
        .ok_or(GeographicRoutingError::Overflow)?;
    let (mut lower, mut upper) = (0_u64, size);
    while lower < upper {
        let midpoint = lower + (upper - lower).div_ceil(2);
        if quadratic_boundary_is_at_or_below(midpoint, size, n, d)? {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }
    u32::try_from(lower.min(size - 1)).map_err(|_| GeographicRoutingError::Overflow)
}

fn quadratic_boundary_is_at_or_below(
    index: u64,
    size: u64,
    n: i128,
    d: i128,
) -> Result<bool, GeographicRoutingError> {
    let (index, size) = (i128::from(index), i128::from(size));
    let size_squared = size
        .checked_mul(size)
        .ok_or(GeographicRoutingError::Overflow)?;
    let numerator = if index
        .checked_mul(2)
        .ok_or(GeographicRoutingError::Overflow)?
        >= size
    {
        index
            .checked_mul(index)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_sub(size_squared))
            .ok_or(GeographicRoutingError::Overflow)?
    } else {
        let complement = size
            .checked_sub(index)
            .ok_or(GeographicRoutingError::Overflow)?;
        size_squared
            .checked_sub(
                complement
                    .checked_mul(complement)
                    .and_then(|v| v.checked_mul(4))
                    .ok_or(GeographicRoutingError::Overflow)?,
            )
            .ok_or(GeographicRoutingError::Overflow)?
    };
    Ok(numerator
        .checked_mul(d)
        .ok_or(GeographicRoutingError::Overflow)?
        <= n.checked_mul(
            size_squared
                .checked_mul(3)
                .ok_or(GeographicRoutingError::Overflow)?,
        )
        .ok_or(GeographicRoutingError::Overflow)?)
}

fn cell_id_from_face_ij(
    face: u8,
    i: u32,
    j: u32,
    level: u8,
) -> Result<S2CellId, GeographicRoutingError> {
    const SWAP: usize = 1;
    const IJ_TO_POS: [[u8; 4]; 4] = [[0, 1, 3, 2], [0, 3, 1, 2], [2, 3, 1, 0], [2, 1, 3, 0]];
    const POS_TO_ORIENTATION: [usize; 4] = [SWAP, 0, 0, 3];
    let mut orientation = usize::from(face) & SWAP;
    let mut position = 0_u64;
    for bit in (0..level).rev() {
        let ij = usize::from(((i >> bit) & 1) != 0) * 2 + usize::from(((j >> bit) & 1) != 0);
        let child = usize::from(IJ_TO_POS[orientation][ij]);
        position = position
            .checked_shl(2)
            .and_then(|v| v.checked_add(u64::try_from(child).ok()?))
            .ok_or(GeographicRoutingError::Overflow)?;
        orientation ^= POS_TO_ORIENTATION[child];
    }
    let shift = 2 * u32::from(MAX_S2_LEVEL - level);
    let value = u64::from(face)
        .checked_shl(61)
        .and_then(|v| v.checked_add(position.checked_shl(shift + 1)?))
        .and_then(|v| v.checked_add(1_u64.checked_shl(shift)?))
        .ok_or(GeographicRoutingError::Overflow)?;
    S2CellId::new(value).map_err(GeographicRoutingError::S2)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GeographicRoutingError {
    #[error("invalid S2 face {0}")]
    InvalidFace(u8),
    #[error("latitude {0}e-7 degrees is outside [-90, 90]")]
    LatitudeOutOfRange(i32),
    #[error("longitude {0}e-7 degrees is outside [-180, 180)")]
    LongitudeOutOfRange(i32),
    #[error("latitude {0} half-arcseconds is outside [-648000, 648000]")]
    LatitudeHalfArcsecondsOutOfRange(i32),
    #[error("longitude {0} half-arcseconds is outside [-1296000, 1296000)")]
    LongitudeHalfArcsecondsOutOfRange(i32),
    #[error("S2 level {0} is outside 0..=30")]
    InvalidLevel(u8),
    #[error("geographic coordinate produced a zero ECEF ray")]
    ZeroRay,
    #[error("fixed-point geographic routing overflowed")]
    Overflow,
    #[error(transparent)]
    S2(#[from] S2CellIdError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::*;

    const GOLDEN_VECTORS: &str = include_str!("../testdata/geographic-s2-v1.json");

    #[derive(Deserialize)]
    struct GoldenSuite {
        schema_version: u16,
        coordinate_frame: String,
        address_bridge: String,
        angle_algorithm: String,
        vectors: Vec<GoldenVector>,
    }

    #[derive(Deserialize)]
    struct GoldenVector {
        name: String,
        latitude_e7: i32,
        longitude_e7: i32,
        cells: BTreeMap<String, String>,
    }

    #[test]
    fn cardinal_geographic_directions_select_the_expected_s2_faces() {
        let cases = [
            (0, 0, 0),
            (0, 900_000_000, 1),
            (900_000_000, 0, 2),
            (0, -900_000_000, 4),
            (-900_000_000, 0, 5),
        ];
        for (latitude, longitude, face) in cases {
            let coordinate =
                GeographicCoordinateE7::new(latitude, longitude).expect("valid coordinate");
            let cell = route_geographic_to_s2(coordinate, 10).expect("routable coordinate");
            assert_eq!(cell.face(), face);
            assert_eq!(cell.level(), 10);
        }
    }

    #[test]
    fn cell_id_hilbert_bits_decode_to_their_exact_face_ij_coordinates() {
        for (latitude, longitude) in [
            (0, 0),
            (387_000_000, -903_000_000),
            (-452_000_000, 1_702_000_000),
        ] {
            let coordinate =
                GeographicCoordinateE7::new(latitude, longitude).expect("valid coordinate");
            let cell = route_geographic_to_s2(coordinate, 14).expect("routable coordinate");
            let decoded = decode_s2_face_ij(cell);
            assert_eq!(decoded.level, 14);
            assert_eq!(
                cell_id_from_face_ij(decoded.face, decoded.i, decoded.j, decoded.level),
                Ok(cell)
            );
        }
    }

    #[test]
    fn ij_cell_centres_use_exact_quadratic_face_coordinates() {
        let lower = S2FaceIj {
            face: 0,
            i: 0,
            j: 0,
            level: 1,
        };
        let upper = S2FaceIj {
            face: 0,
            i: 1,
            j: 1,
            level: 1,
        };
        assert_eq!(
            s2_face_ij_center_uv(lower),
            Ok(S2FaceUv {
                face: 0,
                u_numerator: -5,
                v_numerator: -5,
                denominator: 12
            })
        );
        assert_eq!(
            s2_face_ij_center_uv(upper),
            Ok(S2FaceUv {
                face: 0,
                u_numerator: 5,
                v_numerator: 5,
                denominator: 12
            })
        );
    }

    #[test]
    fn ij_cell_vertices_use_exact_quadratic_face_coordinates() {
        let cell = S2FaceIj {
            face: 0,
            i: 0,
            j: 0,
            level: 1,
        };
        assert_eq!(
            s2_face_ij_vertex_uv(cell, 0, 0),
            Ok(S2FaceUv {
                face: 0,
                u_numerator: -12,
                v_numerator: -12,
                denominator: 12,
            })
        );
        assert_eq!(
            s2_face_ij_vertex_uv(cell, 1, 1),
            Ok(S2FaceUv {
                face: 0,
                u_numerator: 0,
                v_numerator: 0,
                denominator: 12,
            })
        );
        assert!(s2_face_ij_vertex_uv(cell, 3, 0).is_err());
    }

    #[test]
    fn face_uv_maps_to_the_pinned_s2_face_axes() {
        let uv = S2FaceUv {
            face: 4,
            u_numerator: 2,
            v_numerator: -3,
            denominator: 7,
        };
        assert_eq!(
            s2_face_uv_to_ray(uv),
            Ok(S2FaceRay {
                x: -3,
                y: -7,
                z: -2
            })
        );
    }

    #[test]
    fn cardinal_rays_convert_to_exact_geographic_axes() {
        assert_eq!(
            s2_ray_to_geographic_e7(S2FaceRay { x: 1, y: 0, z: 0 }),
            Ok(GeographicCoordinateE7::new(0, 0).expect("origin"))
        );
        assert_eq!(
            s2_ray_to_geographic_e7(S2FaceRay { x: 0, y: 1, z: 0 }),
            Ok(GeographicCoordinateE7::new(0, 900_000_000).expect("east"))
        );
    }

    #[test]
    fn inverse_cell_centres_route_back_to_their_source_cells() {
        for (latitude, longitude) in [
            (0, 0),
            (387_000_000, -903_000_000),
            (-452_000_000, 1_702_000_000),
            (521_000_000, 132_000_000),
        ] {
            let source = GeographicCoordinateE7::new(latitude, longitude).expect("coordinate");
            let cell = route_geographic_to_s2(source, 14).expect("source routes");
            let centre = s2_ray_to_geographic_e7(
                s2_face_uv_to_ray(s2_face_ij_center_uv(decode_s2_face_ij(cell)).expect("centre"))
                    .expect("ray"),
            )
            .expect("centre converts");
            assert_eq!(route_geographic_to_s2(centre, 14), Ok(cell));
        }
    }

    #[test]
    fn every_global_l6_cell_centre_round_trips_through_wgs84_e7() {
        for face in 0_u64..6 {
            let root = S2CellId::new((face << 61) | (1_u64 << 60)).expect("face root");
            let mut cells = vec![root];
            for _ in 0..6 {
                cells = cells
                    .into_iter()
                    .flat_map(|cell| cell.children().expect("children"))
                    .collect();
            }
            for cell in cells {
                let centre = s2_ray_to_geographic_e7(
                    s2_face_uv_to_ray(
                        s2_face_ij_center_uv(decode_s2_face_ij(cell)).expect("centre"),
                    )
                    .expect("ray"),
                )
                .expect("centre converts");
                assert_eq!(route_geographic_to_s2(centre, 6), Ok(cell));
            }
        }
    }

    #[test]
    fn geographic_routing_is_repeatable_and_levels_share_ancestors() {
        let coordinate =
            GeographicCoordinateE7::new(387_000_000, -903_000_000).expect("valid coordinate");
        let finest = route_geographic_to_s2(coordinate, 23).expect("routable coordinate");
        assert_eq!(
            finest,
            route_geographic_to_s2(coordinate, 23).expect("repeatable route")
        );
        for level in [10, 14, 18] {
            assert_eq!(
                finest.ancestor(level),
                Ok(route_geographic_to_s2(coordinate, level).expect("routable ancestor"))
            );
        }
    }

    #[test]
    fn edge_neighbors_preserve_level_and_reverse_across_cube_faces() {
        for face in 0_u64..6 {
            let root = S2CellId::new((face << 61) | (1_u64 << 60)).expect("face root");
            for cell in root.descendants_at(2).expect("small face grid") {
                let neighbors = s2_edge_neighbors(cell).expect("edge neighbors");
                assert!(
                    neighbors
                        .iter()
                        .all(|neighbor| neighbor.level() == cell.level())
                );
                for neighbor in neighbors {
                    assert!(
                        s2_edge_neighbors(neighbor)
                            .expect("neighbor edges")
                            .contains(&cell)
                    );
                }
            }
        }
    }

    #[test]
    fn checked_in_geographic_vectors_pin_the_source_coordinate_contract() {
        let suite: GoldenSuite =
            serde_json::from_str(GOLDEN_VECTORS).expect("valid geographic golden suite");
        assert_eq!(suite.schema_version, 1);
        assert_eq!(suite.coordinate_frame, "WGS84 geodetic e7 degree");
        assert_eq!(suite.address_bridge, "wgs84_ecef_geocentric_ray");
        assert_eq!(suite.angle_algorithm, "cordic_q62_turns_v1");
        for vector in suite.vectors {
            let coordinate = GeographicCoordinateE7::new(vector.latitude_e7, vector.longitude_e7)
                .unwrap_or_else(|error| {
                    panic!("{} has an invalid coordinate: {error}", vector.name)
                });
            for (level, expected) in vector.cells {
                let level = level.parse::<u8>().expect("fixture level is numeric");
                assert_eq!(
                    route_geographic_to_s2(coordinate, level).map(|cell| cell.to_string()),
                    Ok(expected),
                    "{} at L{level}",
                    vector.name
                );
            }
        }
    }

    #[test]
    fn coordinate_domain_and_levels_fail_closed() {
        assert!(GeographicCoordinateE7::new(900_000_001, 0).is_err());
        assert!(GeographicCoordinateE7::new(0, 1_800_000_000).is_err());
        let coordinate = GeographicCoordinateE7::new(0, 0).expect("valid coordinate");
        assert!(matches!(
            route_geographic_to_s2(coordinate, 31),
            Err(GeographicRoutingError::InvalidLevel(31))
        ));
    }

    #[test]
    fn half_arcsecond_coordinates_keep_exact_etopo_center_lattice() {
        let first_etopo_center = GeographicCoordinateHalfArcsecond::new(-647_940, -1_295_940)
            .expect("first ETOPO cell center is valid");
        let last_etopo_center = GeographicCoordinateHalfArcsecond::new(647_940, 1_295_940)
            .expect("last ETOPO cell center is valid");
        assert_eq!(first_etopo_center.latitude_half_arcseconds(), -647_940);
        assert_eq!(last_etopo_center.longitude_half_arcseconds(), 1_295_940);
        assert!(route_half_arcsecond_to_s2(first_etopo_center, 10).is_ok());
        assert!(route_half_arcsecond_to_s2(last_etopo_center, 10).is_ok());
        assert!(GeographicCoordinateHalfArcsecond::new(648_001, 0).is_err());
        assert!(GeographicCoordinateHalfArcsecond::new(0, 1_296_000).is_err());
    }
}
