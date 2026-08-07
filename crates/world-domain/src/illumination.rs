//! Provisional fixed-scale local illumination geometry.
//!
//! An ephemeris geocentric Sun vector is not inherently Earth-fixed. This boundary
//! therefore requires an explicit ECEF vector plus a digest identifying the pinned
//! Earth-orientation transform that produced it. It then classifies the Sun against
//! the geocentric radial horizon using checked integer arithmetic only.
//!
//! This is geometry plumbing, not scientific admission. It does not model an
//! ellipsoid-normal horizon, terrain occlusion, solar-disc extent, or atmospheric
//! refraction and attenuation.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

use crate::{
    CartesianAxis, CartesianMillimetres, CelestialState, Digest, SignedSquaredMillimetres,
    TdbSecondsSinceJ2000,
};

/// Frame label used when reporting an invalid Sun vector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SunVectorFrame {
    Ephemeris,
    EarthFixed,
}

impl fmt::Display for SunVectorFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ephemeris => formatter.write_str("ephemeris"),
            Self::EarthFixed => formatter.write_str("Earth-fixed"),
        }
    }
}

/// A nonzero surface position in the Earth-centered, Earth-fixed frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EcefSurfacePosition(CartesianMillimetres);

impl EcefSurfacePosition {
    pub fn new(position_mm: CartesianMillimetres) -> Result<Self, IlluminationGeometryError> {
        if is_zero(position_mm) {
            return Err(IlluminationGeometryError::ZeroLengthSurfacePosition);
        }
        Ok(Self(position_mm))
    }

    #[must_use]
    pub const fn position_mm(self) -> CartesianMillimetres {
        self.0
    }
}

impl Serialize for EcefSurfacePosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EcefSurfacePosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let position = CartesianMillimetres::deserialize(deserializer)?;
        Self::new(position).map_err(de::Error::custom)
    }
}

/// A geocentric Sun vector explicitly transformed into ECEF at one TDB instant.
///
/// Both source and transformed vectors are retained. `earth_orientation_digest`
/// identifies the exact transform specification or coefficient bundle used by the
/// adapter; a zero digest is rejected as absent provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct EarthFixedSunVector {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    source_geocentric_mm: CartesianMillimetres,
    geocentric_ecef_mm: CartesianMillimetres,
    earth_orientation_digest: Digest,
}

impl EarthFixedSunVector {
    pub fn new(
        tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
        source_geocentric_mm: CartesianMillimetres,
        geocentric_ecef_mm: CartesianMillimetres,
        earth_orientation_digest: Digest,
    ) -> Result<Self, IlluminationGeometryError> {
        if is_zero(source_geocentric_mm) {
            return Err(IlluminationGeometryError::ZeroLengthSunVector {
                frame: SunVectorFrame::Ephemeris,
            });
        }
        if is_zero(geocentric_ecef_mm) {
            return Err(IlluminationGeometryError::ZeroLengthSunVector {
                frame: SunVectorFrame::EarthFixed,
            });
        }
        if earth_orientation_digest == Digest::ZERO {
            return Err(IlluminationGeometryError::MissingEarthOrientationProvenance);
        }
        Ok(Self {
            tdb_seconds_since_j2000,
            source_geocentric_mm,
            geocentric_ecef_mm,
            earth_orientation_digest,
        })
    }

    /// Attach an adapter-produced ECEF vector to its exact celestial source state.
    pub fn from_celestial_state(
        state: CelestialState,
        geocentric_ecef_mm: CartesianMillimetres,
        earth_orientation_digest: Digest,
    ) -> Result<Self, IlluminationGeometryError> {
        Self::new(
            state.tdb_seconds_since_j2000(),
            state.sun_geocentric_mm(),
            geocentric_ecef_mm,
            earth_orientation_digest,
        )
    }

    #[must_use]
    pub const fn tdb_seconds_since_j2000(self) -> TdbSecondsSinceJ2000 {
        self.tdb_seconds_since_j2000
    }

    #[must_use]
    pub const fn source_geocentric_mm(self) -> CartesianMillimetres {
        self.source_geocentric_mm
    }

    #[must_use]
    pub const fn geocentric_ecef_mm(self) -> CartesianMillimetres {
        self.geocentric_ecef_mm
    }

    #[must_use]
    pub const fn earth_orientation_digest(self) -> Digest {
        self.earth_orientation_digest
    }
}

impl<'de> Deserialize<'de> for EarthFixedSunVector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EarthFixedSunVectorWire::deserialize(deserializer)?;
        Self::new(
            wire.tdb_seconds_since_j2000,
            wire.source_geocentric_mm,
            wire.geocentric_ecef_mm,
            wire.earth_orientation_digest,
        )
        .map_err(de::Error::custom)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EarthFixedSunVectorWire {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    source_geocentric_mm: CartesianMillimetres,
    geocentric_ecef_mm: CartesianMillimetres,
    earth_orientation_digest: Digest,
}

/// Exact classification against the provisional geocentric radial horizon.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RadialHorizonClassification {
    Below,
    On,
    Above,
}

/// Provenance-preserving local illumination geometry at one ECEF surface position.
///
/// The signed dot product is `surface_position · surface_to_sun`. Its sign alone
/// determines the radial-horizon classification; it is not a normalized elevation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LocalIlluminationGeometry {
    sun: EarthFixedSunVector,
    surface_ecef_mm: EcefSurfacePosition,
    surface_to_sun_ecef_mm: CartesianMillimetres,
    radial_horizon_dot_mm2: SignedSquaredMillimetres,
    radial_horizon: RadialHorizonClassification,
}

impl LocalIlluminationGeometry {
    pub fn derive(
        sun: EarthFixedSunVector,
        surface_ecef_mm: EcefSurfacePosition,
    ) -> Result<Self, IlluminationGeometryError> {
        let surface_to_sun_ecef_mm =
            checked_surface_to_sun(sun.geocentric_ecef_mm(), surface_ecef_mm.position_mm())?;
        if is_zero(surface_to_sun_ecef_mm) {
            return Err(IlluminationGeometryError::SunCoincidesWithSurfacePosition);
        }
        let radial_horizon_dot_mm2 =
            checked_dot(surface_ecef_mm.position_mm(), surface_to_sun_ecef_mm)?;
        let radial_horizon = match radial_horizon_dot_mm2.get().cmp(&0) {
            std::cmp::Ordering::Less => RadialHorizonClassification::Below,
            std::cmp::Ordering::Equal => RadialHorizonClassification::On,
            std::cmp::Ordering::Greater => RadialHorizonClassification::Above,
        };
        Ok(Self {
            sun,
            surface_ecef_mm,
            surface_to_sun_ecef_mm,
            radial_horizon_dot_mm2,
            radial_horizon,
        })
    }

    #[must_use]
    pub const fn sun(self) -> EarthFixedSunVector {
        self.sun
    }

    #[must_use]
    pub const fn surface_ecef_mm(self) -> EcefSurfacePosition {
        self.surface_ecef_mm
    }

    #[must_use]
    pub const fn surface_to_sun_ecef_mm(self) -> CartesianMillimetres {
        self.surface_to_sun_ecef_mm
    }

    #[must_use]
    pub const fn radial_horizon_dot_mm2(self) -> SignedSquaredMillimetres {
        self.radial_horizon_dot_mm2
    }

    #[must_use]
    pub const fn radial_horizon(self) -> RadialHorizonClassification {
        self.radial_horizon
    }
}

impl<'de> Deserialize<'de> for LocalIlluminationGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LocalIlluminationGeometryWire::deserialize(deserializer)?;
        let derived = Self::derive(wire.sun, wire.surface_ecef_mm).map_err(de::Error::custom)?;
        if derived.surface_to_sun_ecef_mm != wire.surface_to_sun_ecef_mm
            || derived.radial_horizon_dot_mm2 != wire.radial_horizon_dot_mm2
            || derived.radial_horizon != wire.radial_horizon
        {
            return Err(de::Error::custom(
                "serialized illumination geometry does not match its fixed-scale inputs",
            ));
        }
        Ok(derived)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalIlluminationGeometryWire {
    sun: EarthFixedSunVector,
    surface_ecef_mm: EcefSurfacePosition,
    surface_to_sun_ecef_mm: CartesianMillimetres,
    radial_horizon_dot_mm2: SignedSquaredMillimetres,
    radial_horizon: RadialHorizonClassification,
}

/// Failure to establish exact local illumination geometry.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum IlluminationGeometryError {
    #[error("{frame} geocentric Sun vector has zero length")]
    ZeroLengthSunVector { frame: SunVectorFrame },
    #[error("Earth-orientation transform provenance digest is required")]
    MissingEarthOrientationProvenance,
    #[error("ECEF surface-position vector has zero length")]
    ZeroLengthSurfacePosition,
    #[error("surface-to-Sun ECEF {axis}-component overflowed while subtracting")]
    SurfaceToSunComponentOverflow { axis: CartesianAxis },
    #[error("Sun position coincides with the ECEF surface position")]
    SunCoincidesWithSurfacePosition,
    #[error("radial-horizon Cartesian dot product overflowed")]
    RadialHorizonDotProductOverflow,
}

fn is_zero(vector: CartesianMillimetres) -> bool {
    vector.x_mm() == 0 && vector.y_mm() == 0 && vector.z_mm() == 0
}

fn checked_surface_to_sun(
    sun: CartesianMillimetres,
    surface: CartesianMillimetres,
) -> Result<CartesianMillimetres, IlluminationGeometryError> {
    let component = |sun_value: i128,
                     surface_value: i128,
                     axis: CartesianAxis|
     -> Result<i128, IlluminationGeometryError> {
        sun_value
            .checked_sub(surface_value)
            .ok_or(IlluminationGeometryError::SurfaceToSunComponentOverflow { axis })
    };
    Ok(CartesianMillimetres::new(
        component(sun.x_mm(), surface.x_mm(), CartesianAxis::X)?,
        component(sun.y_mm(), surface.y_mm(), CartesianAxis::Y)?,
        component(sun.z_mm(), surface.z_mm(), CartesianAxis::Z)?,
    ))
}

fn checked_dot(
    left: CartesianMillimetres,
    right: CartesianMillimetres,
) -> Result<SignedSquaredMillimetres, IlluminationGeometryError> {
    let x = left
        .x_mm()
        .checked_mul(right.x_mm())
        .ok_or(IlluminationGeometryError::RadialHorizonDotProductOverflow)?;
    let y = left
        .y_mm()
        .checked_mul(right.y_mm())
        .ok_or(IlluminationGeometryError::RadialHorizonDotProductOverflow)?;
    let z = left
        .z_mm()
        .checked_mul(right.z_mm())
        .ok_or(IlluminationGeometryError::RadialHorizonDotProductOverflow)?;
    x.checked_add(y)
        .and_then(|value| value.checked_add(z))
        .map(SignedSquaredMillimetres::new)
        .ok_or(IlluminationGeometryError::RadialHorizonDotProductOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> Digest {
        Digest::sha256(b"provisional Earth orientation fixture v1")
    }

    fn earth_fixed_sun(vector: CartesianMillimetres) -> EarthFixedSunVector {
        EarthFixedSunVector::new(
            TdbSecondsSinceJ2000::new(12),
            CartesianMillimetres::new(100, 20, 30),
            vector,
            provenance(),
        )
        .unwrap_or_else(|error| panic!("valid fixture: {error}"))
    }

    fn surface(vector: CartesianMillimetres) -> EcefSurfacePosition {
        EcefSurfacePosition::new(vector).unwrap_or_else(|error| panic!("valid fixture: {error}"))
    }

    #[test]
    fn radial_horizon_classification_uses_the_surface_to_sun_vector() {
        let above = LocalIlluminationGeometry::derive(
            earth_fixed_sun(CartesianMillimetres::new(100, 0, 0)),
            surface(CartesianMillimetres::new(10, 0, 0)),
        );
        assert!(matches!(
            above,
            Ok(value)
                if value.surface_to_sun_ecef_mm() == CartesianMillimetres::new(90, 0, 0)
                    && value.radial_horizon_dot_mm2()
                        == SignedSquaredMillimetres::new(900)
                    && value.radial_horizon() == RadialHorizonClassification::Above
        ));

        let on = LocalIlluminationGeometry::derive(
            earth_fixed_sun(CartesianMillimetres::new(10, 100, 0)),
            surface(CartesianMillimetres::new(10, 0, 0)),
        );
        assert!(matches!(
            on,
            Ok(value) if value.radial_horizon() == RadialHorizonClassification::On
        ));

        let below = LocalIlluminationGeometry::derive(
            earth_fixed_sun(CartesianMillimetres::new(-100, 0, 0)),
            surface(CartesianMillimetres::new(10, 0, 0)),
        );
        assert!(matches!(
            below,
            Ok(value)
                if value.radial_horizon_dot_mm2()
                    == SignedSquaredMillimetres::new(-1_100)
                    && value.radial_horizon() == RadialHorizonClassification::Below
        ));
    }

    #[test]
    fn frame_and_surface_invariants_fail_closed() {
        assert_eq!(
            EarthFixedSunVector::new(
                TdbSecondsSinceJ2000::new(0),
                CartesianMillimetres::new(0, 0, 0),
                CartesianMillimetres::new(1, 0, 0),
                provenance(),
            ),
            Err(IlluminationGeometryError::ZeroLengthSunVector {
                frame: SunVectorFrame::Ephemeris,
            })
        );
        assert_eq!(
            EarthFixedSunVector::new(
                TdbSecondsSinceJ2000::new(0),
                CartesianMillimetres::new(1, 0, 0),
                CartesianMillimetres::new(1, 0, 0),
                Digest::ZERO,
            ),
            Err(IlluminationGeometryError::MissingEarthOrientationProvenance)
        );
        assert_eq!(
            EcefSurfacePosition::new(CartesianMillimetres::new(0, 0, 0)),
            Err(IlluminationGeometryError::ZeroLengthSurfacePosition)
        );
    }

    #[test]
    fn arithmetic_overflow_is_explicit() {
        let subtraction = LocalIlluminationGeometry::derive(
            earth_fixed_sun(CartesianMillimetres::new(i128::MIN, 1, 0)),
            surface(CartesianMillimetres::new(1, 0, 0)),
        );
        assert_eq!(
            subtraction,
            Err(IlluminationGeometryError::SurfaceToSunComponentOverflow {
                axis: CartesianAxis::X,
            })
        );

        const COMPONENT: i128 = 10_000_000_000_000_000_000;
        let dot = LocalIlluminationGeometry::derive(
            earth_fixed_sun(CartesianMillimetres::new(COMPONENT * 2, COMPONENT * 2, 0)),
            surface(CartesianMillimetres::new(COMPONENT, COMPONENT, 0)),
        );
        assert_eq!(
            dot,
            Err(IlluminationGeometryError::RadialHorizonDotProductOverflow)
        );
    }

    #[test]
    fn serialization_retains_inputs_and_revalidates_derived_geometry() {
        let geometry = LocalIlluminationGeometry::derive(
            earth_fixed_sun(CartesianMillimetres::new(100, 0, 0)),
            surface(CartesianMillimetres::new(10, 0, 0)),
        );
        let encoded = geometry.as_ref().map(serde_json::to_string);
        assert!(matches!(
            encoded,
            Ok(Ok(ref value))
                if value.contains("\"earth_orientation_digest\":")
                    && value.contains("\"source_geocentric_mm\":")
                    && value.contains("\"surface_to_sun_ecef_mm\":")
                    && value.contains("\"radial_horizon_dot_mm2\":\"900\"")
                    && value.ends_with("\"radial_horizon\":\"above\"}")
        ));

        let decoded = encoded
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref().ok())
            .map(|value| serde_json::from_str::<LocalIlluminationGeometry>(value));
        assert!(matches!(decoded, Some(Ok(value)) if Some(value) == geometry.ok()));

        let tampered = encoded
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref().ok())
            .map(|value| {
                value.replace(
                    "\"radial_horizon_dot_mm2\":\"900\"",
                    "\"radial_horizon_dot_mm2\":\"901\"",
                )
            });
        assert!(matches!(
            tampered.as_deref().map(serde_json::from_str::<LocalIlluminationGeometry>),
            Some(Err(error)) if error.to_string().contains("does not match")
        ));
    }
}
