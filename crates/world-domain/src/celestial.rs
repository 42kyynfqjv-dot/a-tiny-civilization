//! Replay-safe fixed-scale celestial coordinates and simulation time conversion.
//!
//! Source adapters may evaluate an ephemeris however their pinned source requires,
//! but values cross into the durable domain only as whole TDB seconds relative to
//! J2000 and Cartesian millimetres. This boundary intentionally contains no host
//! floating-point representation.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

use crate::SimTick;

/// Signed whole TDB seconds relative to the J2000 epoch.
///
/// Negative values are before J2000. JSON represents the value as a decimal string
/// so verifiers do not lose integer precision when crossing language boundaries.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TdbSecondsSinceJ2000(i128);

impl TdbSecondsSinceJ2000 {
    #[must_use]
    pub const fn new(value: i128) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> i128 {
        self.0
    }
}

impl fmt::Display for TdbSecondsSinceJ2000 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for TdbSecondsSinceJ2000 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TdbSecondsSinceJ2000 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse::<i128>().map(Self).map_err(de::Error::custom)
    }
}

/// Positive whole TDB seconds represented by one simulation tick.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TickDurationSeconds(u64);

impl TickDurationSeconds {
    pub fn new(value: u64) -> Result<Self, CelestialError> {
        if value == 0 {
            return Err(CelestialError::ZeroTickDuration);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TickDurationSeconds {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for TickDurationSeconds {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TickDurationSeconds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let value = encoded.parse::<u64>().map_err(de::Error::custom)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// Convert a simulation tick to its exact whole-second TDB instant.
///
/// `epoch` is the signed TDB time of tick zero relative to J2000. Tick duration is
/// positive, so simulation time advances monotonically from that epoch.
pub fn tdb_seconds_at_tick(
    tick: SimTick,
    tick_duration: TickDurationSeconds,
    epoch: TdbSecondsSinceJ2000,
) -> Result<TdbSecondsSinceJ2000, CelestialError> {
    let offset = u128::from(tick.get())
        .checked_mul(u128::from(tick_duration.get()))
        .ok_or(CelestialError::TickOffsetOverflow)?;
    let signed_offset = i128::try_from(offset).map_err(|_| CelestialError::TickOffsetOverflow)?;
    epoch
        .get()
        .checked_add(signed_offset)
        .map(TdbSecondsSinceJ2000)
        .ok_or(CelestialError::TdbSecondsOverflow)
}

/// One axis of a fixed-scale Cartesian celestial vector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CartesianAxis {
    X,
    Y,
    Z,
}

impl fmt::Display for CartesianAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X => formatter.write_str("x"),
            Self::Y => formatter.write_str("y"),
            Self::Z => formatter.write_str("z"),
        }
    }
}

/// A Cartesian position or displacement in exact signed millimetres.
///
/// Components serialize as decimal strings to preserve all 128 bits in portable
/// event logs and verification bundles.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CartesianMillimetres {
    #[serde(with = "decimal_i128")]
    x_mm: i128,
    #[serde(with = "decimal_i128")]
    y_mm: i128,
    #[serde(with = "decimal_i128")]
    z_mm: i128,
}

impl CartesianMillimetres {
    #[must_use]
    pub const fn new(x_mm: i128, y_mm: i128, z_mm: i128) -> Self {
        Self { x_mm, y_mm, z_mm }
    }

    #[must_use]
    pub const fn x_mm(self) -> i128 {
        self.x_mm
    }

    #[must_use]
    pub const fn y_mm(self) -> i128 {
        self.y_mm
    }

    #[must_use]
    pub const fn z_mm(self) -> i128 {
        self.z_mm
    }

    /// Subtract an origin position, yielding this position relative to that origin.
    ///
    /// Ephemeris adapters use this to derive geocentric vectors from barycentric
    /// positions without permitting signed-integer wraparound.
    pub fn checked_relative_to(self, origin: Self) -> Result<Self, CelestialError> {
        Ok(Self {
            x_mm: self.x_mm.checked_sub(origin.x_mm).ok_or(
                CelestialError::VectorComponentOverflow {
                    axis: CartesianAxis::X,
                },
            )?,
            y_mm: self.y_mm.checked_sub(origin.y_mm).ok_or(
                CelestialError::VectorComponentOverflow {
                    axis: CartesianAxis::Y,
                },
            )?,
            z_mm: self.z_mm.checked_sub(origin.z_mm).ok_or(
                CelestialError::VectorComponentOverflow {
                    axis: CartesianAxis::Z,
                },
            )?,
        })
    }
}

/// The fixed-scale celestial inputs available to deterministic world rules at one
/// TDB instant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CelestialState {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    sun_geocentric_mm: CartesianMillimetres,
    moon_geocentric_mm: CartesianMillimetres,
}

impl CelestialState {
    #[must_use]
    pub const fn new(
        tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
        sun_geocentric_mm: CartesianMillimetres,
        moon_geocentric_mm: CartesianMillimetres,
    ) -> Self {
        Self {
            tdb_seconds_since_j2000,
            sun_geocentric_mm,
            moon_geocentric_mm,
        }
    }

    #[must_use]
    pub const fn tdb_seconds_since_j2000(self) -> TdbSecondsSinceJ2000 {
        self.tdb_seconds_since_j2000
    }

    #[must_use]
    pub const fn sun_geocentric_mm(self) -> CartesianMillimetres {
        self.sun_geocentric_mm
    }

    #[must_use]
    pub const fn moon_geocentric_mm(self) -> CartesianMillimetres {
        self.moon_geocentric_mm
    }
}

/// Failure to construct fixed-scale celestial domain inputs exactly.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CelestialError {
    #[error("simulation tick duration must be at least one whole TDB second")]
    ZeroTickDuration,
    #[error("simulation tick offset exceeded the signed TDB-second range")]
    TickOffsetOverflow,
    #[error("TDB seconds relative to J2000 exceeded the signed range")]
    TdbSecondsOverflow,
    #[error("celestial vector {axis}-component overflowed while subtracting its origin")]
    VectorComponentOverflow { axis: CartesianAxis },
}

mod decimal_i128 {
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S>(value: &i128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse::<i128>().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_conversion_is_exact_on_both_sides_of_j2000() {
        let duration = TickDurationSeconds::new(60);
        assert_eq!(
            duration.and_then(|duration| tdb_seconds_at_tick(
                SimTick::new(2),
                duration,
                TdbSecondsSinceJ2000::new(-86_400),
            )),
            Ok(TdbSecondsSinceJ2000::new(-86_280))
        );
    }

    #[test]
    fn invalid_duration_and_time_overflow_are_explicit() {
        assert_eq!(
            TickDurationSeconds::new(0),
            Err(CelestialError::ZeroTickDuration)
        );

        let one_second = TickDurationSeconds::new(1);
        assert_eq!(
            one_second.and_then(|duration| tdb_seconds_at_tick(
                SimTick::new(1),
                duration,
                TdbSecondsSinceJ2000::new(i128::MAX),
            )),
            Err(CelestialError::TdbSecondsOverflow)
        );

        let maximum_duration = TickDurationSeconds::new(u64::MAX);
        assert_eq!(
            maximum_duration.and_then(|duration| tdb_seconds_at_tick(
                SimTick::new(u64::MAX),
                duration,
                TdbSecondsSinceJ2000::new(0),
            )),
            Err(CelestialError::TickOffsetOverflow)
        );
    }

    #[test]
    fn vector_subtraction_is_checked_per_axis() {
        let body = CartesianMillimetres::new(15, -20, 35);
        let earth = CartesianMillimetres::new(10, -8, 5);
        assert_eq!(
            body.checked_relative_to(earth),
            Ok(CartesianMillimetres::new(5, -12, 30))
        );

        let overflow = CartesianMillimetres::new(i128::MIN, 0, 0)
            .checked_relative_to(CartesianMillimetres::new(1, 0, 0));
        assert_eq!(
            overflow,
            Err(CelestialError::VectorComponentOverflow {
                axis: CartesianAxis::X,
            })
        );
    }

    #[test]
    fn celestial_state_uses_portable_decimal_strings() {
        let state = CelestialState::new(
            TdbSecondsSinceJ2000::new(-1),
            CartesianMillimetres::new(i128::MAX, -2, 3),
            CartesianMillimetres::new(-4, 5, i128::MIN),
        );
        let encoded = serde_json::to_string(&state);
        assert!(matches!(
            encoded.as_deref(),
            Ok(value) if value == concat!(
                "{\"tdb_seconds_since_j2000\":\"-1\",",
                "\"sun_geocentric_mm\":{",
                "\"x_mm\":\"170141183460469231731687303715884105727\",",
                "\"y_mm\":\"-2\",\"z_mm\":\"3\"},",
                "\"moon_geocentric_mm\":{\"x_mm\":\"-4\",\"y_mm\":\"5\",",
                "\"z_mm\":\"-170141183460469231731687303715884105728\"}}"
            )
        ));

        let decoded = encoded
            .as_deref()
            .map(serde_json::from_str::<CelestialState>);
        assert!(matches!(decoded, Ok(Ok(value)) if value == state));
    }

    #[test]
    fn tick_duration_json_cannot_bypass_its_nonzero_invariant() {
        assert!(matches!(
            serde_json::from_str::<TickDurationSeconds>("\"0\""),
            Err(error) if error.to_string().contains("at least one")
        ));
        assert!(matches!(
            serde_json::to_string(&TickDurationSeconds(300)),
            Ok(value) if value == "\"300\""
        ));
    }
}
