//! Exact solar-distance forcing geometry for later seasonal and insolation rules.
//!
//! Relative inverse-square forcing is represented as the reduced positive rational
//! `reference_distance_squared / observed_distance_squared`. This layer contains no
//! atmospheric response, surface irradiance, orbital interpretation, or scientific
//! admission claim.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::{Digest, SquaredMillimetres, TdbSecondsSinceJ2000, TideGeometry};

/// A positive rational stored in one canonical reduced form.
///
/// Numerator and denominator serialize as decimal strings for portable verification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct CanonicalPositiveRational {
    #[serde(with = "decimal_u128")]
    numerator: u128,
    #[serde(with = "decimal_u128")]
    denominator: u128,
}

impl CanonicalPositiveRational {
    /// Reduce a nonzero numerator and denominator to their unique canonical form.
    pub fn new(numerator: u128, denominator: u128) -> Result<Self, CanonicalPositiveRationalError> {
        if numerator == 0 {
            return Err(CanonicalPositiveRationalError::ZeroNumerator);
        }
        if denominator == 0 {
            return Err(CanonicalPositiveRationalError::ZeroDenominator);
        }
        let divisor = greatest_common_divisor(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    #[must_use]
    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> u128 {
        self.denominator
    }

    #[must_use]
    pub const fn is_unity(self) -> bool {
        self.numerator == self.denominator
    }
}

impl fmt::Display for CanonicalPositiveRational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}

impl<'de> Deserialize<'de> for CanonicalPositiveRational {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CanonicalPositiveRationalWire::deserialize(deserializer)?;
        let reduced = Self::new(wire.numerator, wire.denominator).map_err(de::Error::custom)?;
        if reduced.numerator != wire.numerator || reduced.denominator != wire.denominator {
            return Err(de::Error::custom(
                "positive rational must use its canonical reduced representation",
            ));
        }
        Ok(reduced)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicalPositiveRationalWire {
    #[serde(with = "decimal_u128")]
    numerator: u128,
    #[serde(with = "decimal_u128")]
    denominator: u128,
}

/// A pinned integer-millimetre reference distance for relative solar forcing.
///
/// The caller chooses the reference and supplies a digest identifying its source or
/// governing specification. This domain layer deliberately does not embed a favored
/// astronomical constant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PinnedSolarReferenceDistance {
    #[serde(with = "decimal_u128")]
    distance_mm: u128,
    distance_squared_mm2: SquaredMillimetres,
    provenance_digest: Digest,
}

impl PinnedSolarReferenceDistance {
    pub fn new(
        distance_mm: u128,
        provenance_digest: Digest,
    ) -> Result<Self, SolarDistanceForcingError> {
        if distance_mm == 0 {
            return Err(SolarDistanceForcingError::ZeroReferenceDistance);
        }
        if provenance_digest == Digest::ZERO {
            return Err(SolarDistanceForcingError::MissingReferenceProvenance);
        }
        let distance_squared = distance_mm
            .checked_mul(distance_mm)
            .ok_or(SolarDistanceForcingError::ReferenceDistanceSquareOverflow)?;
        Ok(Self {
            distance_mm,
            distance_squared_mm2: SquaredMillimetres::new(distance_squared),
            provenance_digest,
        })
    }

    #[must_use]
    pub const fn distance_mm(self) -> u128 {
        self.distance_mm
    }

    #[must_use]
    pub const fn distance_squared_mm2(self) -> SquaredMillimetres {
        self.distance_squared_mm2
    }

    #[must_use]
    pub const fn provenance_digest(self) -> Digest {
        self.provenance_digest
    }
}

impl<'de> Deserialize<'de> for PinnedSolarReferenceDistance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PinnedSolarReferenceDistanceWire::deserialize(deserializer)?;
        let derived =
            Self::new(wire.distance_mm, wire.provenance_digest).map_err(de::Error::custom)?;
        if derived.distance_squared_mm2 != wire.distance_squared_mm2 {
            return Err(de::Error::custom(
                "serialized solar reference square does not match its distance",
            ));
        }
        Ok(derived)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinnedSolarReferenceDistanceWire {
    #[serde(with = "decimal_u128")]
    distance_mm: u128,
    distance_squared_mm2: SquaredMillimetres,
    provenance_digest: Digest,
}

/// Exact relative inverse-square forcing at one TDB instant.
///
/// A value of 1/1 means the observed Earth-Sun distance equals the pinned reference.
/// Values above 1 represent a shorter observed distance; values below 1 represent a
/// longer observed distance. No energy or irradiance unit is implied.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SolarDistanceForcing {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    reference: PinnedSolarReferenceDistance,
    earth_sun_distance_squared_mm2: SquaredMillimetres,
    relative_inverse_square: CanonicalPositiveRational,
}

impl SolarDistanceForcing {
    pub fn derive(
        tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
        reference: PinnedSolarReferenceDistance,
        earth_sun_distance_squared_mm2: SquaredMillimetres,
    ) -> Result<Self, SolarDistanceForcingError> {
        if earth_sun_distance_squared_mm2.get() == 0 {
            return Err(SolarDistanceForcingError::ZeroEarthSunDistance);
        }
        let relative_inverse_square = CanonicalPositiveRational::new(
            reference.distance_squared_mm2().get(),
            earth_sun_distance_squared_mm2.get(),
        )?;
        Ok(Self {
            tdb_seconds_since_j2000,
            reference,
            earth_sun_distance_squared_mm2,
            relative_inverse_square,
        })
    }

    /// Derive forcing directly from the checked Sun distance in shared tide geometry.
    pub fn from_tide_geometry(
        reference: PinnedSolarReferenceDistance,
        tide: TideGeometry,
    ) -> Result<Self, SolarDistanceForcingError> {
        Self::derive(
            tide.tdb_seconds_since_j2000(),
            reference,
            tide.sun().distance_squared_mm2(),
        )
    }

    #[must_use]
    pub const fn tdb_seconds_since_j2000(self) -> TdbSecondsSinceJ2000 {
        self.tdb_seconds_since_j2000
    }

    #[must_use]
    pub const fn reference(self) -> PinnedSolarReferenceDistance {
        self.reference
    }

    #[must_use]
    pub const fn earth_sun_distance_squared_mm2(self) -> SquaredMillimetres {
        self.earth_sun_distance_squared_mm2
    }

    #[must_use]
    pub const fn relative_inverse_square(self) -> CanonicalPositiveRational {
        self.relative_inverse_square
    }
}

impl<'de> Deserialize<'de> for SolarDistanceForcing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SolarDistanceForcingWire::deserialize(deserializer)?;
        let derived = Self::derive(
            wire.tdb_seconds_since_j2000,
            wire.reference,
            wire.earth_sun_distance_squared_mm2,
        )
        .map_err(de::Error::custom)?;
        if derived.relative_inverse_square != wire.relative_inverse_square {
            return Err(de::Error::custom(
                "serialized solar-distance forcing does not match its distances",
            ));
        }
        Ok(derived)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SolarDistanceForcingWire {
    tdb_seconds_since_j2000: TdbSecondsSinceJ2000,
    reference: PinnedSolarReferenceDistance,
    earth_sun_distance_squared_mm2: SquaredMillimetres,
    relative_inverse_square: CanonicalPositiveRational,
}

/// Failure to construct canonical fixed-scale solar-distance forcing.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SolarDistanceForcingError {
    #[error("solar reference distance must be nonzero")]
    ZeroReferenceDistance,
    #[error("solar reference distance provenance digest is required")]
    MissingReferenceProvenance,
    #[error("solar reference distance overflowed while squaring")]
    ReferenceDistanceSquareOverflow,
    #[error("observed Earth-Sun distance must be nonzero")]
    ZeroEarthSunDistance,
    #[error("relative inverse-square forcing is invalid: {0}")]
    InvalidRelativeInverseSquare(#[from] CanonicalPositiveRationalError),
}

/// Failure to construct a canonical positive rational.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CanonicalPositiveRationalError {
    #[error("positive rational numerator must be nonzero")]
    ZeroNumerator,
    #[error("positive rational denominator must be nonzero")]
    ZeroDenominator,
}

const fn greatest_common_divisor(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

mod decimal_u128 {
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse::<u128>().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use crate::{CartesianMillimetres, CelestialState, TideGeometry};

    use super::*;

    fn provenance() -> Digest {
        Digest::sha256(b"solar reference fixture")
    }

    fn reference(distance_mm: u128) -> PinnedSolarReferenceDistance {
        PinnedSolarReferenceDistance::new(distance_mm, provenance())
            .unwrap_or_else(|error| panic!("valid fixture: {error}"))
    }

    #[test]
    fn inverse_square_ratio_is_reduced_exactly() {
        let forcing = SolarDistanceForcing::derive(
            TdbSecondsSinceJ2000::new(7),
            reference(10),
            SquaredMillimetres::new(225),
        );
        assert!(matches!(
            forcing,
            Ok(value)
                if value.reference().distance_squared_mm2() == SquaredMillimetres::new(100)
                    && value.relative_inverse_square()
                        == CanonicalPositiveRational::new(4, 9)
                            .unwrap_or_else(|error| panic!("valid fixture: {error}"))
                    && value.relative_inverse_square().to_string() == "4/9"
        ));
    }

    #[test]
    fn forcing_can_share_the_checked_celestial_geometry() {
        let celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(-3),
            CartesianMillimetres::new(6, 8, 0),
            CartesianMillimetres::new(1, 0, 0),
        );
        let forcing = TideGeometry::from_celestial_state(celestial)
            .ok()
            .map(|tide| SolarDistanceForcing::from_tide_geometry(reference(10), tide));
        assert!(matches!(
            forcing,
            Some(Ok(value))
                if value.tdb_seconds_since_j2000() == TdbSecondsSinceJ2000::new(-3)
                    && value.earth_sun_distance_squared_mm2()
                        == SquaredMillimetres::new(100)
                    && value.relative_inverse_square()
                        == CanonicalPositiveRational::new(1, 1)
                            .unwrap_or_else(|error| panic!("valid fixture: {error}"))
                    && value.relative_inverse_square().is_unity()
        ));
    }

    #[test]
    fn tide_handoff_uses_sun_distance_not_lunar_geometry() {
        let state = |moon| {
            CelestialState::new(
                TdbSecondsSinceJ2000::new(11),
                CartesianMillimetres::new(9, 12, 0),
                moon,
            )
        };
        let derive = |celestial| {
            TideGeometry::from_celestial_state(celestial)
                .ok()
                .and_then(|tide| SolarDistanceForcing::from_tide_geometry(reference(10), tide).ok())
        };
        let first = derive(state(CartesianMillimetres::new(1, 0, 0)));
        let second = derive(state(CartesianMillimetres::new(0, -2, 0)));
        assert!(matches!((first, second), (Some(left), Some(right)) if left == right));
    }

    #[test]
    fn reference_and_observed_distance_errors_are_explicit() {
        assert_eq!(
            PinnedSolarReferenceDistance::new(0, provenance()),
            Err(SolarDistanceForcingError::ZeroReferenceDistance)
        );
        assert_eq!(
            PinnedSolarReferenceDistance::new(1, Digest::ZERO),
            Err(SolarDistanceForcingError::MissingReferenceProvenance)
        );
        assert_eq!(
            PinnedSolarReferenceDistance::new(u128::MAX, provenance()),
            Err(SolarDistanceForcingError::ReferenceDistanceSquareOverflow)
        );
        assert_eq!(
            SolarDistanceForcing::derive(
                TdbSecondsSinceJ2000::new(0),
                reference(1),
                SquaredMillimetres::new(0),
            ),
            Err(SolarDistanceForcingError::ZeroEarthSunDistance)
        );
    }

    #[test]
    fn rational_wire_form_must_already_be_canonical() {
        assert!(matches!(
            serde_json::from_str::<CanonicalPositiveRational>(
                "{\"numerator\":\"2\",\"denominator\":\"4\"}",
            ),
            Err(error) if error.to_string().contains("canonical reduced")
        ));
        assert_eq!(
            CanonicalPositiveRational::new(0, 1),
            Err(CanonicalPositiveRationalError::ZeroNumerator)
        );
        assert_eq!(
            CanonicalPositiveRational::new(1, 0),
            Err(CanonicalPositiveRationalError::ZeroDenominator)
        );
    }

    #[test]
    fn astronomical_unit_scale_remains_exact_and_portable() {
        // The domain does not bless this reference; the fixture merely exercises the
        // magnitude expected when an adapter pins an astronomical-unit-scale source.
        const REFERENCE_MM: u128 = 149_597_870_700_000;
        let reference = reference(REFERENCE_MM);
        let forcing = SolarDistanceForcing::derive(
            TdbSecondsSinceJ2000::new(0),
            reference,
            reference.distance_squared_mm2(),
        );
        assert!(matches!(
            forcing,
            Ok(value)
                if value.reference().distance_mm() == REFERENCE_MM
                    && value.reference().distance_squared_mm2().get()
                        == 22_379_522_917_973_918_490_000_000_000
                    && value.relative_inverse_square().is_unity()
                    && serde_json::to_string(&value).is_ok_and(|encoded| {
                        encoded.contains("\"distance_mm\":\"149597870700000\"")
                            && encoded.contains(
                                "\"distance_squared_mm2\":\"22379522917973918490000000000\"",
                            )
                    })
        ));
    }

    #[test]
    fn serialized_forcing_is_portable_and_revalidated() {
        let forcing = SolarDistanceForcing::derive(
            TdbSecondsSinceJ2000::new(7),
            reference(10),
            SquaredMillimetres::new(225),
        );
        let encoded = forcing.as_ref().map(serde_json::to_string);
        assert!(matches!(
            encoded,
            Ok(Ok(ref value))
                if value.contains("\"distance_mm\":\"10\"")
                    && value.contains("\"distance_squared_mm2\":\"100\"")
                    && value.contains("\"earth_sun_distance_squared_mm2\":\"225\"")
                    && value.ends_with(
                        "\"relative_inverse_square\":{\"numerator\":\"4\",\"denominator\":\"9\"}}"
                    )
        ));
        let decoded = encoded
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref().ok())
            .map(|value| serde_json::from_str::<SolarDistanceForcing>(value));
        assert!(matches!(decoded, Some(Ok(value)) if Some(value) == forcing.ok()));

        let tampered = encoded
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref().ok())
            .map(|value| value.replace("\"numerator\":\"4\"", "\"numerator\":\"5\""));
        assert!(matches!(
            tampered.as_deref().map(serde_json::from_str::<SolarDistanceForcing>),
            Some(Err(error)) if error.to_string().contains("does not match")
        ));
    }

    #[test]
    fn serialized_reference_square_is_revalidated() {
        let encoded = format!(
            concat!(
                "{{\"distance_mm\":\"10\",\"distance_squared_mm2\":\"101\",",
                "\"provenance_digest\":\"{}\"}}"
            ),
            provenance()
        );
        assert!(matches!(
            serde_json::from_str::<PinnedSolarReferenceDistance>(&encoded),
            Err(error) if error.to_string().contains("does not match")
        ));
    }
}
