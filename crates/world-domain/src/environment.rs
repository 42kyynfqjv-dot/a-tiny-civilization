//! Source-bound local environmental inputs for provisional execution.
//!
//! The simulation engine receives this compact, immutable contract at genesis. It
//! never opens a raster, calls a service, or infers habitat during replay.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Digest, S2CellId};

pub const NORMAL_YEAR_PHASE_COUNT: usize = 12;
const PROVISIONAL_STATUS: &str = "provisional-evidence-only";
const PROVISIONAL_WEATHER_STATUS: &str = "provisional-weather-input-not-scientifically-admitted";
const PROVISIONAL_SURFACE_STATUS: &str = "provisional-surface-input-not-scientifically-admitted";

/// Pinned, physical evidence at one active patch. Values are temperature normals,
/// not weather, habitat suitability, food availability, or an organism's beliefs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalLocalEnvironmentBaseline {
    pub status: String,
    pub source_evidence_digest: Digest,
    pub evidence_patch: S2CellId,
    pub active_patch: S2CellId,
    pub air_temperature_unit: String,
    pub air_temperature_decimal_places: u8,
    pub air_temperature_normal_minimum: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub air_temperature_normal_mean: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub air_temperature_normal_maximum: [i64; NORMAL_YEAR_PHASE_COUNT],
}

/// Fixed-point normal-period inputs for a deterministic provisional weather
/// driver. These are source-bound physical dimensions, not a forecast, habitat
/// classification, or anything an organism can recognize by name.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalLocalWeatherBaseline {
    pub status: String,
    pub source_normals_digest: Digest,
    pub evidence_patch: S2CellId,
    pub active_patch: S2CellId,
    pub air_temperature_unit: String,
    pub air_temperature_decimal_places: u8,
    pub air_temperature_normal_minimum: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub air_temperature_normal_mean: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub air_temperature_normal_maximum: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub precipitation_unit: String,
    pub precipitation_decimal_places: u8,
    pub precipitation_normal_mean: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub eastward_wind_unit: String,
    pub eastward_wind_decimal_places: u8,
    pub eastward_wind_normal_mean: [i64; NORMAL_YEAR_PHASE_COUNT],
    pub northward_wind_unit: String,
    pub northward_wind_decimal_places: u8,
    pub northward_wind_normal_mean: [i64; NORMAL_YEAR_PHASE_COUNT],
}

/// Source-domain local surface values committed for later causal mappings.
///
/// This private configuration contract deliberately retains upstream domains. It
/// does not make source codes, property order, or scientific names perceptible to
/// an organism and does not imply a use for any value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionalLocalSurfaceBaseline {
    pub status: String,
    pub source_evidence_digest: Digest,
    pub evidence_patch: S2CellId,
    pub active_patch: S2CellId,
    pub terrain_minimum_millimetres: i64,
    pub terrain_mean_millimetres: i64,
    pub terrain_maximum_millimetres: i64,
    pub surface_water_occurrence_source_code: u8,
    /// Ordered SoilGrids source values: nine schema-pinned properties, each
    /// containing Q0.05, Q0.5, and Q0.95 in the upstream signed-i16 domain.
    pub topsoil_source_quantiles: [[i16; 3]; 9],
}

impl ProvisionalLocalEnvironmentBaseline {
    pub fn validate(&self) -> Result<(), LocalEnvironmentError> {
        if self.status != PROVISIONAL_STATUS
            || self.source_evidence_digest == Digest::ZERO
            || self.evidence_patch.level() != 10
            || self.active_patch.level() < self.evidence_patch.level()
            || !self.evidence_patch.contains(self.active_patch)
        {
            return Err(LocalEnvironmentError::InvalidIdentity);
        }
        if !unit(&self.air_temperature_unit) || self.air_temperature_decimal_places > 9 {
            return Err(LocalEnvironmentError::InvalidTemperatureUnit);
        }
        if self
            .air_temperature_normal_minimum
            .iter()
            .zip(self.air_temperature_normal_mean.iter())
            .zip(self.air_temperature_normal_maximum.iter())
            .any(|((minimum, mean), maximum)| minimum > mean || mean > maximum)
        {
            return Err(LocalEnvironmentError::InvalidTemperatureRange);
        }
        Ok(())
    }

    pub fn mean_at_normal_phase(&self, phase: usize) -> Result<i64, LocalEnvironmentError> {
        self.validate()?;
        self.air_temperature_normal_mean
            .get(phase)
            .copied()
            .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))
    }
}

impl ProvisionalLocalWeatherBaseline {
    pub fn validate(&self) -> Result<(), LocalEnvironmentError> {
        if self.status != PROVISIONAL_WEATHER_STATUS
            || self.source_normals_digest == Digest::ZERO
            || self.evidence_patch.level() != 10
            || self.active_patch.level() < self.evidence_patch.level()
            || !self.evidence_patch.contains(self.active_patch)
        {
            return Err(LocalEnvironmentError::InvalidWeatherIdentity);
        }
        if self.air_temperature_unit != "degC"
            || self.air_temperature_decimal_places != 3
            || self.precipitation_unit != "m"
            || self.precipitation_decimal_places != 6
            || self.eastward_wind_unit != "m/s"
            || self.eastward_wind_decimal_places != 3
            || self.northward_wind_unit != "m/s"
            || self.northward_wind_decimal_places != 3
        {
            return Err(LocalEnvironmentError::InvalidWeatherUnit);
        }
        if self
            .air_temperature_normal_minimum
            .iter()
            .zip(self.air_temperature_normal_mean.iter())
            .zip(self.air_temperature_normal_maximum.iter())
            .any(|((minimum, mean), maximum)| minimum > mean || mean > maximum)
            || self
                .precipitation_normal_mean
                .iter()
                .any(|value| *value < 0)
        {
            return Err(LocalEnvironmentError::InvalidWeatherRange);
        }
        Ok(())
    }

    pub fn temperature_range_at_normal_phase(
        &self,
        phase: usize,
    ) -> Result<(i64, i64, i64), LocalEnvironmentError> {
        self.validate()?;
        Ok((
            *self
                .air_temperature_normal_minimum
                .get(phase)
                .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))?,
            *self
                .air_temperature_normal_mean
                .get(phase)
                .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))?,
            *self
                .air_temperature_normal_maximum
                .get(phase)
                .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))?,
        ))
    }

    pub fn flux_means_at_normal_phase(
        &self,
        phase: usize,
    ) -> Result<(i64, i64, i64), LocalEnvironmentError> {
        self.validate()?;
        Ok((
            *self
                .precipitation_normal_mean
                .get(phase)
                .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))?,
            *self
                .eastward_wind_normal_mean
                .get(phase)
                .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))?,
            *self
                .northward_wind_normal_mean
                .get(phase)
                .ok_or(LocalEnvironmentError::NormalPhaseOutOfRange(phase))?,
        ))
    }
}

impl ProvisionalLocalSurfaceBaseline {
    pub fn validate(&self) -> Result<(), LocalEnvironmentError> {
        if self.status != PROVISIONAL_SURFACE_STATUS
            || self.source_evidence_digest == Digest::ZERO
            || self.evidence_patch.level() != 10
            || self.active_patch.level() < self.evidence_patch.level()
            || !self.evidence_patch.contains(self.active_patch)
        {
            return Err(LocalEnvironmentError::InvalidSurfaceIdentity);
        }
        if self.terrain_minimum_millimetres > self.terrain_mean_millimetres
            || self.terrain_mean_millimetres > self.terrain_maximum_millimetres
            || self.topsoil_source_quantiles.iter().any(|values| {
                values.contains(&i16::MIN) || values[0] > values[1] || values[1] > values[2]
            })
        {
            return Err(LocalEnvironmentError::InvalidSurfaceRange);
        }
        Ok(())
    }
}

fn unit(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'^' | b'-' | b'_' | b'.')
        })
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LocalEnvironmentError {
    #[error("invalid provisional local-environment identity or spatial binding")]
    InvalidIdentity,
    #[error("invalid local-environment temperature unit")]
    InvalidTemperatureUnit,
    #[error("local-environment temperature normal has an invalid range")]
    InvalidTemperatureRange,
    #[error("invalid provisional local-weather identity or spatial binding")]
    InvalidWeatherIdentity,
    #[error("invalid provisional local-weather physical unit contract")]
    InvalidWeatherUnit,
    #[error("provisional local-weather normals have an invalid range")]
    InvalidWeatherRange,
    #[error("invalid provisional local-surface identity or spatial binding")]
    InvalidSurfaceIdentity,
    #[error("provisional local-surface values have an invalid range or missing source value")]
    InvalidSurfaceRange,
    #[error("normal-year phase {0} is out of range")]
    NormalPhaseOutOfRange(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> ProvisionalLocalEnvironmentBaseline {
        let evidence_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        ProvisionalLocalEnvironmentBaseline {
            status: PROVISIONAL_STATUS.to_owned(),
            source_evidence_digest: Digest::sha256(b"source evidence"),
            evidence_patch,
            active_patch: evidence_patch.children().expect("children")[0],
            air_temperature_unit: "degC".to_owned(),
            air_temperature_decimal_places: 1,
            air_temperature_normal_minimum: [1; NORMAL_YEAR_PHASE_COUNT],
            air_temperature_normal_mean: [2; NORMAL_YEAR_PHASE_COUNT],
            air_temperature_normal_maximum: [3; NORMAL_YEAR_PHASE_COUNT],
        }
    }

    #[test]
    fn source_bound_baseline_accepts_physical_normal_values_only() {
        let baseline = baseline();
        assert_eq!(baseline.mean_at_normal_phase(0), Ok(2));
        assert_eq!(
            baseline.mean_at_normal_phase(NORMAL_YEAR_PHASE_COUNT),
            Err(LocalEnvironmentError::NormalPhaseOutOfRange(
                NORMAL_YEAR_PHASE_COUNT
            ))
        );
    }

    #[test]
    fn baseline_rejects_a_patch_outside_the_evidence_cell() {
        let mut baseline = baseline();
        baseline.active_patch = "3000010000000000".parse().expect("other face");
        assert_eq!(
            baseline.validate(),
            Err(LocalEnvironmentError::InvalidIdentity)
        );
    }

    #[test]
    fn weather_baseline_requires_physical_units_ranges_and_source_binding() {
        let evidence_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let mut weather = ProvisionalLocalWeatherBaseline {
            status: PROVISIONAL_WEATHER_STATUS.to_owned(),
            source_normals_digest: Digest::sha256(b"ERA5 fixed-point normals"),
            evidence_patch,
            active_patch: evidence_patch.children().expect("children")[0],
            air_temperature_unit: "degC".to_owned(),
            air_temperature_decimal_places: 3,
            air_temperature_normal_minimum: [10_000; NORMAL_YEAR_PHASE_COUNT],
            air_temperature_normal_mean: [15_000; NORMAL_YEAR_PHASE_COUNT],
            air_temperature_normal_maximum: [20_000; NORMAL_YEAR_PHASE_COUNT],
            precipitation_unit: "m".to_owned(),
            precipitation_decimal_places: 6,
            precipitation_normal_mean: [1_000; NORMAL_YEAR_PHASE_COUNT],
            eastward_wind_unit: "m/s".to_owned(),
            eastward_wind_decimal_places: 3,
            eastward_wind_normal_mean: [500; NORMAL_YEAR_PHASE_COUNT],
            northward_wind_unit: "m/s".to_owned(),
            northward_wind_decimal_places: 3,
            northward_wind_normal_mean: [-500; NORMAL_YEAR_PHASE_COUNT],
        };
        assert_eq!(
            weather.temperature_range_at_normal_phase(0),
            Ok((10_000, 15_000, 20_000))
        );
        assert_eq!(
            weather.flux_means_at_normal_phase(0),
            Ok((1_000, 500, -500))
        );
        weather.precipitation_normal_mean[0] = -1;
        assert_eq!(
            weather.validate(),
            Err(LocalEnvironmentError::InvalidWeatherRange)
        );
    }

    #[test]
    fn surface_baseline_retains_source_domains_without_accepting_missing_soil() {
        let evidence_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let mut surface = ProvisionalLocalSurfaceBaseline {
            status: PROVISIONAL_SURFACE_STATUS.to_owned(),
            source_evidence_digest: Digest::sha256(b"origin surface evidence"),
            evidence_patch,
            active_patch: evidence_patch.children().expect("children")[0],
            terrain_minimum_millimetres: 1_000,
            terrain_mean_millimetres: 2_000,
            terrain_maximum_millimetres: 3_000,
            surface_water_occurrence_source_code: 0,
            topsoil_source_quantiles: [[1, 2, 3]; 9],
        };
        assert_eq!(surface.validate(), Ok(()));
        surface.topsoil_source_quantiles[3][1] = i16::MIN;
        assert_eq!(
            surface.validate(),
            Err(LocalEnvironmentError::InvalidSurfaceRange)
        );
    }
}
