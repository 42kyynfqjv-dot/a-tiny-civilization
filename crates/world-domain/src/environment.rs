//! Source-bound local environmental inputs for provisional execution.
//!
//! The simulation engine receives this compact, immutable contract at genesis. It
//! never opens a raster, calls a service, or infers habitat during replay.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Digest, S2CellId};

pub const NORMAL_YEAR_PHASE_COUNT: usize = 12;
const PROVISIONAL_STATUS: &str = "provisional-evidence-only";

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
}
