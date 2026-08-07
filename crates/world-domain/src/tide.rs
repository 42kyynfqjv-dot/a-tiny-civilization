//! Deterministic geometric inputs for world and observer tide calculations.
//!
//! These types do not claim a scientifically admitted ocean-response model. They
//! establish one replay-safe geometry boundary, derived from the pinned celestial
//! state, that both canonical world mechanics and observer projections can consume.
//! Runtime evaluation uses only checked integer arithmetic.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

use crate::{CartesianAxis, CartesianMillimetres, CelestialState, TdbSecondsSinceJ2000};

/// A tide-generating celestial body represented at the domain boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TideBody {
    Sun,
    Moon,
}

impl fmt::Display for TideBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sun => formatter.write_str("sun"),
            Self::Moon => formatter.write_str("moon"),
        }
    }
}

/// An exact non-negative squared distance in square millimetres.
///
/// JSON uses a decimal string because the value can exceed portable JSON integer
/// ranges even for ordinary Earth-to-Sun distances.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SquaredMillimetres(u128);

impl SquaredMillimetres {
    #[must_use]
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }
}

impl fmt::Display for SquaredMillimetres {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for SquaredMillimetres {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SquaredMillimetres {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse::<u128>().map(Self).map_err(de::Error::custom)
    }
}

/// An exact signed Cartesian dot product in square millimetres.
///
/// The sign records whether the geocentric Sun and Moon vectors occupy the same or
/// opposite hemispheres. Consumers must not treat it as a normalized angle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SignedSquaredMillimetres(i128);

impl SignedSquaredMillimetres {
    #[must_use]
    pub const fn new(value: i128) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> i128 {
        self.0
    }
}

impl fmt::Display for SignedSquaredMillimetres {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for SignedSquaredMillimetres {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SignedSquaredMillimetres {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse::<i128>().map(Self).map_err(de::Error::custom)
    }
}

/// Checked geocentric geometry for one tide-generating body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TideBodyGeometry {
    geocentric_mm: CartesianMillimetres,
    distance_squared_mm2: SquaredMillimetres,
}

impl TideBodyGeometry {
    #[must_use]
    pub const fn geocentric_mm(self) -> CartesianMillimetres {
        self.geocentric_mm
    }

    #[must_use]
    pub const fn distance_squared_mm2(self) -> SquaredMillimetres {
        self.distance_squared_mm2
    }
}

/// Shared, replay-safe Sun/Moon geometry from which tide models may derive forcing.
///
/// Deserialization recomputes every redundant scalar from the vectors and rejects
/// tampering, so observers cannot publish geometry inconsistent with world inputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TideGeometry {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    sun: TideBodyGeometry,
    moon: TideBodyGeometry,
    sun_moon_dot_mm2: SignedSquaredMillimetres,
}

impl TideGeometry {
    /// Derive checked tide geometry from one fixed-scale celestial state.
    pub fn from_celestial_state(state: CelestialState) -> Result<Self, TideGeometryError> {
        let sun_vector = state.sun_geocentric_mm();
        let moon_vector = state.moon_geocentric_mm();
        let sun = body_geometry(TideBody::Sun, sun_vector)?;
        let moon = body_geometry(TideBody::Moon, moon_vector)?;
        let sun_moon_dot_mm2 = checked_dot(sun_vector, moon_vector)?;
        Ok(Self {
            tdb_seconds_since_j2000: state.tdb_seconds_since_j2000(),
            sun,
            moon,
            sun_moon_dot_mm2,
        })
    }

    #[must_use]
    pub const fn tdb_seconds_since_j2000(self) -> TdbSecondsSinceJ2000 {
        self.tdb_seconds_since_j2000
    }

    #[must_use]
    pub const fn sun(self) -> TideBodyGeometry {
        self.sun
    }

    #[must_use]
    pub const fn moon(self) -> TideBodyGeometry {
        self.moon
    }

    #[must_use]
    pub const fn sun_moon_dot_mm2(self) -> SignedSquaredMillimetres {
        self.sun_moon_dot_mm2
    }

    #[must_use]
    pub const fn celestial_state(self) -> CelestialState {
        CelestialState::new(
            self.tdb_seconds_since_j2000,
            self.sun.geocentric_mm,
            self.moon.geocentric_mm,
        )
    }
}

impl<'de> Deserialize<'de> for TideGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TideGeometryWire::deserialize(deserializer)?;
        let state = CelestialState::new(
            wire.tdb_seconds_since_j2000,
            wire.sun.geocentric_mm,
            wire.moon.geocentric_mm,
        );
        let derived = Self::from_celestial_state(state).map_err(de::Error::custom)?;
        if derived.sun.distance_squared_mm2 != wire.sun.distance_squared_mm2
            || derived.moon.distance_squared_mm2 != wire.moon.distance_squared_mm2
            || derived.sun_moon_dot_mm2 != wire.sun_moon_dot_mm2
        {
            return Err(de::Error::custom(
                "serialized tide geometry does not match its celestial vectors",
            ));
        }
        Ok(derived)
    }
}

#[derive(Deserialize)]
struct TideGeometryWire {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    sun: TideBodyGeometryWire,
    moon: TideBodyGeometryWire,
    sun_moon_dot_mm2: SignedSquaredMillimetres,
}

#[derive(Deserialize)]
struct TideBodyGeometryWire {
    geocentric_mm: CartesianMillimetres,
    distance_squared_mm2: SquaredMillimetres,
}

/// Failure to derive tide geometry exactly from a celestial state.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TideGeometryError {
    #[error("{body} geocentric vector has zero length")]
    ZeroLengthBodyVector { body: TideBody },
    #[error("{body} geocentric {axis}-component overflowed while squaring")]
    SquaredComponentOverflow { body: TideBody, axis: CartesianAxis },
    #[error("{body} squared geocentric distance overflowed while summing components")]
    SquaredDistanceOverflow { body: TideBody },
    #[error("Sun/Moon Cartesian dot product overflowed")]
    SunMoonDotProductOverflow,
}

fn body_geometry(
    body: TideBody,
    vector: CartesianMillimetres,
) -> Result<TideBodyGeometry, TideGeometryError> {
    let x_squared = checked_square(body, CartesianAxis::X, vector.x_mm())?;
    let y_squared = checked_square(body, CartesianAxis::Y, vector.y_mm())?;
    let z_squared = checked_square(body, CartesianAxis::Z, vector.z_mm())?;
    let distance_squared = x_squared
        .checked_add(y_squared)
        .and_then(|value| value.checked_add(z_squared))
        .ok_or(TideGeometryError::SquaredDistanceOverflow { body })?;
    if distance_squared == 0 {
        return Err(TideGeometryError::ZeroLengthBodyVector { body });
    }
    Ok(TideBodyGeometry {
        geocentric_mm: vector,
        distance_squared_mm2: SquaredMillimetres(distance_squared),
    })
}

fn checked_square(
    body: TideBody,
    axis: CartesianAxis,
    value: i128,
) -> Result<u128, TideGeometryError> {
    let magnitude = value.unsigned_abs();
    magnitude
        .checked_mul(magnitude)
        .ok_or(TideGeometryError::SquaredComponentOverflow { body, axis })
}

fn checked_dot(
    left: CartesianMillimetres,
    right: CartesianMillimetres,
) -> Result<SignedSquaredMillimetres, TideGeometryError> {
    let x = left
        .x_mm()
        .checked_mul(right.x_mm())
        .ok_or(TideGeometryError::SunMoonDotProductOverflow)?;
    let y = left
        .y_mm()
        .checked_mul(right.y_mm())
        .ok_or(TideGeometryError::SunMoonDotProductOverflow)?;
    let z = left
        .z_mm()
        .checked_mul(right.z_mm())
        .ok_or(TideGeometryError::SunMoonDotProductOverflow)?;
    x.checked_add(y)
        .and_then(|value| value.checked_add(z))
        .map(SignedSquaredMillimetres)
        .ok_or(TideGeometryError::SunMoonDotProductOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_state() -> CelestialState {
        CelestialState::new(
            TdbSecondsSinceJ2000::new(-42),
            CartesianMillimetres::new(3, 4, 0),
            CartesianMillimetres::new(-4, 0, 3),
        )
    }

    #[test]
    fn geometry_is_derived_exactly_from_the_celestial_state() {
        let geometry = TideGeometry::from_celestial_state(fixture_state());
        assert!(matches!(
            geometry,
            Ok(value)
                if value.tdb_seconds_since_j2000() == TdbSecondsSinceJ2000::new(-42)
                    && value.sun().distance_squared_mm2() == SquaredMillimetres::new(25)
                    && value.moon().distance_squared_mm2() == SquaredMillimetres::new(25)
                    && value.sun_moon_dot_mm2() == SignedSquaredMillimetres::new(-12)
                    && value.celestial_state() == fixture_state()
        ));
    }

    #[test]
    fn zero_length_and_component_overflow_are_explicit() {
        let zero_sun = CelestialState::new(
            TdbSecondsSinceJ2000::new(0),
            CartesianMillimetres::new(0, 0, 0),
            CartesianMillimetres::new(1, 0, 0),
        );
        assert_eq!(
            TideGeometry::from_celestial_state(zero_sun),
            Err(TideGeometryError::ZeroLengthBodyVector {
                body: TideBody::Sun,
            })
        );

        let overflowing_sun = CelestialState::new(
            TdbSecondsSinceJ2000::new(0),
            CartesianMillimetres::new(i128::MAX, 0, 0),
            CartesianMillimetres::new(1, 0, 0),
        );
        assert_eq!(
            TideGeometry::from_celestial_state(overflowing_sun),
            Err(TideGeometryError::SquaredComponentOverflow {
                body: TideBody::Sun,
                axis: CartesianAxis::X,
            })
        );
    }

    #[test]
    fn dot_product_sum_overflow_is_explicit() {
        const COMPONENT: i128 = 10_000_000_000_000_000_000;
        let state = CelestialState::new(
            TdbSecondsSinceJ2000::new(0),
            CartesianMillimetres::new(COMPONENT, COMPONENT, 0),
            CartesianMillimetres::new(COMPONENT, COMPONENT, 0),
        );
        assert_eq!(
            TideGeometry::from_celestial_state(state),
            Err(TideGeometryError::SunMoonDotProductOverflow)
        );
    }

    #[test]
    fn serialized_geometry_is_portable_and_revalidated() {
        let geometry = TideGeometry::from_celestial_state(fixture_state());
        let encoded = geometry.as_ref().map(serde_json::to_string);
        assert!(matches!(
            encoded,
            Ok(Ok(ref value)) if value == concat!(
                "{\"tdb_seconds_since_j2000\":\"-42\",",
                "\"sun\":{\"geocentric_mm\":{\"x_mm\":\"3\",\"y_mm\":\"4\",",
                "\"z_mm\":\"0\"},\"distance_squared_mm2\":\"25\"},",
                "\"moon\":{\"geocentric_mm\":{\"x_mm\":\"-4\",\"y_mm\":\"0\",",
                "\"z_mm\":\"3\"},\"distance_squared_mm2\":\"25\"},",
                "\"sun_moon_dot_mm2\":\"-12\"}"
            )
        ));

        let decoded = encoded
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref().ok())
            .map(|value| serde_json::from_str::<TideGeometry>(value));
        assert!(matches!(decoded, Some(Ok(value)) if Some(value) == geometry.ok()));
    }

    #[test]
    fn deserialization_rejects_inconsistent_derived_values() {
        let tampered = concat!(
            "{\"tdb_seconds_since_j2000\":\"-42\",",
            "\"sun\":{\"geocentric_mm\":{\"x_mm\":\"3\",\"y_mm\":\"4\",",
            "\"z_mm\":\"0\"},\"distance_squared_mm2\":\"26\"},",
            "\"moon\":{\"geocentric_mm\":{\"x_mm\":\"-4\",\"y_mm\":\"0\",",
            "\"z_mm\":\"3\"},\"distance_squared_mm2\":\"25\"},",
            "\"sun_moon_dot_mm2\":\"-12\"}"
        );
        assert!(matches!(
            serde_json::from_str::<TideGeometry>(tampered),
            Err(error) if error.to_string().contains("does not match")
        ));
    }
}
