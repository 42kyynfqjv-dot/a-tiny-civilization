//! Exact, point-scoped ERA5 evidence retained before weather mechanics.
//!
//! Values remain their source IEEE-754 binary32 bits. This artifact neither
//! converts units nor supplies weather, habitat, season, or agent-visible labels.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, GeographicCoordinateE7, S2CellId};

pub const PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-origin-climate-evidence+json";
pub const PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_STATUS: &str =
    "provisional-noncausal-not-scientifically-admitted";
pub const PROVISIONAL_ORIGIN_CLIMATE_NORMALS_SCHEMA_VERSION: u16 = 1;
pub const PROVISIONAL_ORIGIN_CLIMATE_NORMALS_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.provisional-origin-climate-normals+json";
pub const PROVISIONAL_ORIGIN_CLIMATE_NORMALS_STATUS: &str =
    "provisional-weather-input-noncausal-not-scientifically-admitted";
pub const ERA5_NORMAL_FIRST_YEAR: u16 = 1981;
pub const ERA5_NORMAL_LAST_YEAR: u16 = 2010;
pub const ERA5_NORMAL_MONTHS: usize = 360;

const SERIES: [(&str, &str, &str); 6] = [
    ("siconc", "(0 - 1)", "avgua"),
    ("sst", "K", "avgua"),
    ("t2m", "K", "avgua"),
    ("tp", "m", "avgad"),
    ("u10", "m s**-1", "avgua"),
    ("v10", "m s**-1", "avgua"),
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvisionalOriginClimateEvidence {
    pub evidence_schema_version: u16,
    pub status: String,
    pub origin_selection_digest: Digest,
    pub selected_patch: S2CellId,
    pub sample_latitude_e7: i32,
    pub sample_longitude_e7: i32,
    pub source_snapshot_digest: Digest,
    pub source_grid_row: u16,
    pub source_grid_column: u16,
    pub source_grid_latitude_e7: i32,
    pub source_grid_longitude_e7: i32,
    pub source_artifacts: Vec<OriginClimateSourceArtifact>,
    pub series: Vec<OriginClimateSeries>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OriginClimateSourceArtifact {
    pub year: u16,
    pub artifact_path: String,
    pub content_hash: Digest,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OriginClimateSeries {
    pub variable: String,
    pub source_unit: String,
    pub source_step_type: String,
    pub values_ieee754_binary32_bits: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvisionalOriginClimateNormals {
    pub normals_schema_version: u16,
    pub status: String,
    pub origin_climate_evidence_digest: Digest,
    pub conversion_policy: String,
    pub series: Vec<OriginClimateNormalSeries>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OriginClimateNormalSeries {
    pub variable: String,
    pub unit: String,
    pub decimal_places: u8,
    pub months: Vec<OriginClimateNormalMonth>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OriginClimateNormalMonth {
    pub month: u8,
    pub observed_years: u8,
    pub minimum: Option<i64>,
    pub mean: Option<i64>,
    pub maximum: Option<i64>,
}

impl ProvisionalOriginClimateEvidence {
    pub fn validate(&self) -> Result<(), ProvisionalOriginClimateEvidenceError> {
        if self.evidence_schema_version != PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_SCHEMA_VERSION {
            return Err(ProvisionalOriginClimateEvidenceError::UnsupportedSchema(
                self.evidence_schema_version,
            ));
        }
        if self.status != PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_STATUS
            || self.origin_selection_digest == Digest::ZERO
            || self.source_snapshot_digest == Digest::ZERO
            || self.source_grid_row >= 721
            || self.source_grid_column >= 1_440
        {
            return Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence);
        }
        GeographicCoordinateE7::new(self.sample_latitude_e7, self.sample_longitude_e7)
            .map_err(|_| ProvisionalOriginClimateEvidenceError::InvalidEvidence)?;
        GeographicCoordinateE7::new(self.source_grid_latitude_e7, self.source_grid_longitude_e7)
            .map_err(|_| ProvisionalOriginClimateEvidenceError::InvalidEvidence)?;
        if self.source_artifacts.len()
            != usize::from(ERA5_NORMAL_LAST_YEAR - ERA5_NORMAL_FIRST_YEAR + 1)
        {
            return Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence);
        }
        for (offset, artifact) in self.source_artifacts.iter().enumerate() {
            let year = ERA5_NORMAL_FIRST_YEAR
                .checked_add(
                    u16::try_from(offset)
                        .map_err(|_| ProvisionalOriginClimateEvidenceError::InvalidEvidence)?,
                )
                .ok_or(ProvisionalOriginClimateEvidenceError::InvalidEvidence)?;
            if artifact.year != year
                || artifact.artifact_path.is_empty()
                || artifact.content_hash == Digest::ZERO
                || artifact.byte_length == 0
                || !artifact.artifact_path.ends_with(&format!("-{year}.zip"))
            {
                return Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence);
            }
        }
        if self.series.len() != SERIES.len() {
            return Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence);
        }
        for (series, (variable, unit, step_type)) in self.series.iter().zip(SERIES) {
            if series.variable != variable
                || series.source_unit != unit
                || series.source_step_type != step_type
                || series.values_ieee754_binary32_bits.len() != ERA5_NORMAL_MONTHS
                || series.values_ieee754_binary32_bits.iter().any(|bits| {
                    let value = f32::from_bits(*bits);
                    value.is_infinite()
                })
            {
                return Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence);
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProvisionalOriginClimateEvidenceError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| ProvisionalOriginClimateEvidenceError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(
        bytes: &[u8],
    ) -> Result<Self, ProvisionalOriginClimateEvidenceError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalOriginClimateEvidenceError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(ProvisionalOriginClimateEvidenceError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

impl ProvisionalOriginClimateNormals {
    pub fn validate(&self) -> Result<(), ProvisionalOriginClimateEvidenceError> {
        const NORMAL_SERIES: [(&str, &str, u8); 6] = [
            ("siconc", "fraction", 6),
            ("sst", "degC", 3),
            ("t2m", "degC", 3),
            ("tp", "m", 6),
            ("u10", "m/s", 3),
            ("v10", "m/s", 3),
        ];
        if self.normals_schema_version != PROVISIONAL_ORIGIN_CLIMATE_NORMALS_SCHEMA_VERSION
            || self.status != PROVISIONAL_ORIGIN_CLIMATE_NORMALS_STATUS
            || self.origin_climate_evidence_digest == Digest::ZERO
            || self.conversion_policy
                != "binary32-nearest-even-per-observation-then-nearest-even-monthly-mean-v1"
            || self.series.len() != NORMAL_SERIES.len()
        {
            return Err(ProvisionalOriginClimateEvidenceError::InvalidNormals);
        }
        for (series, (variable, unit, decimal_places)) in self.series.iter().zip(NORMAL_SERIES) {
            if series.variable != variable
                || series.unit != unit
                || series.decimal_places != decimal_places
                || series.months.len() != 12
            {
                return Err(ProvisionalOriginClimateEvidenceError::InvalidNormals);
            }
            for (index, month) in series.months.iter().enumerate() {
                let expected_month = u8::try_from(index + 1)
                    .map_err(|_| ProvisionalOriginClimateEvidenceError::InvalidNormals)?;
                let complete = match (month.minimum, month.mean, month.maximum) {
                    (None, None, None) => month.observed_years == 0,
                    (Some(minimum), Some(mean), Some(maximum)) => {
                        (1..=30).contains(&month.observed_years)
                            && minimum <= mean
                            && mean <= maximum
                    }
                    _ => false,
                };
                if month.month != expected_month || !complete {
                    return Err(ProvisionalOriginClimateEvidenceError::InvalidNormals);
                }
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProvisionalOriginClimateEvidenceError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| ProvisionalOriginClimateEvidenceError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(
        bytes: &[u8],
    ) -> Result<Self, ProvisionalOriginClimateEvidenceError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| ProvisionalOriginClimateEvidenceError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(ProvisionalOriginClimateEvidenceError::NonCanonicalEncoding);
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProvisionalOriginClimateEvidenceError {
    #[error("unsupported provisional origin-climate evidence schema {0}")]
    UnsupportedSchema(u16),
    #[error("invalid provisional origin-climate evidence")]
    InvalidEvidence,
    #[error("invalid provisional origin-climate normals")]
    InvalidNormals,
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("noncanonical encoding")]
    NonCanonicalEncoding,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ProvisionalOriginClimateEvidence {
        ProvisionalOriginClimateEvidence {
            evidence_schema_version: PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_SCHEMA_VERSION,
            status: PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_STATUS.to_owned(),
            origin_selection_digest: Digest::sha256(b"origin"),
            selected_patch: S2CellId::new(0x1000_0000_0000_0000).expect("face cell"),
            sample_latitude_e7: 0,
            sample_longitude_e7: 0,
            source_snapshot_digest: Digest::sha256(b"era5"),
            source_grid_row: 360,
            source_grid_column: 0,
            source_grid_latitude_e7: 0,
            source_grid_longitude_e7: 0,
            source_artifacts: (ERA5_NORMAL_FIRST_YEAR..=ERA5_NORMAL_LAST_YEAR)
                .map(|year| OriginClimateSourceArtifact {
                    year,
                    artifact_path: format!("era5/archive-{year}.zip"),
                    content_hash: Digest::sha256(&year.to_be_bytes()),
                    byte_length: 1,
                })
                .collect(),
            series: SERIES
                .into_iter()
                .map(|(variable, unit, step_type)| OriginClimateSeries {
                    variable: variable.to_owned(),
                    source_unit: unit.to_owned(),
                    source_step_type: step_type.to_owned(),
                    values_ieee754_binary32_bits: vec![0; ERA5_NORMAL_MONTHS],
                })
                .collect(),
        }
    }

    #[test]
    fn round_trips_exact_source_bits() {
        let value = fixture();
        let bytes = value.canonical_bytes().expect("canonical evidence");
        assert_eq!(
            ProvisionalOriginClimateEvidence::from_canonical_slice(&bytes),
            Ok(value)
        );
    }

    #[test]
    fn rejects_reordered_series_and_infinity() {
        let mut reordered = fixture();
        reordered.series.swap(0, 1);
        assert_eq!(
            reordered.validate(),
            Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence)
        );
        let mut infinite = fixture();
        infinite.series[0].values_ieee754_binary32_bits[0] = f32::INFINITY.to_bits();
        assert_eq!(
            infinite.validate(),
            Err(ProvisionalOriginClimateEvidenceError::InvalidEvidence)
        );
    }

    #[test]
    fn normals_require_complete_months_and_explicit_missing_values() {
        let normals = ProvisionalOriginClimateNormals {
            normals_schema_version: PROVISIONAL_ORIGIN_CLIMATE_NORMALS_SCHEMA_VERSION,
            status: PROVISIONAL_ORIGIN_CLIMATE_NORMALS_STATUS.to_owned(),
            origin_climate_evidence_digest: Digest::sha256(b"origin climate evidence"),
            conversion_policy:
                "binary32-nearest-even-per-observation-then-nearest-even-monthly-mean-v1".to_owned(),
            series: [
                ("siconc", "fraction", 6),
                ("sst", "degC", 3),
                ("t2m", "degC", 3),
                ("tp", "m", 6),
                ("u10", "m/s", 3),
                ("v10", "m/s", 3),
            ]
            .into_iter()
            .map(
                |(variable, unit, decimal_places)| OriginClimateNormalSeries {
                    variable: variable.to_owned(),
                    unit: unit.to_owned(),
                    decimal_places,
                    months: (1..=12)
                        .map(|month| OriginClimateNormalMonth {
                            month,
                            observed_years: if variable == "sst" { 0 } else { 30 },
                            minimum: (variable != "sst").then_some(1),
                            mean: (variable != "sst").then_some(2),
                            maximum: (variable != "sst").then_some(3),
                        })
                        .collect(),
                },
            )
            .collect(),
        };
        let bytes = normals.canonical_bytes().expect("canonical normals");
        assert_eq!(
            ProvisionalOriginClimateNormals::from_canonical_slice(&bytes),
            Ok(normals.clone())
        );
        let mut partial = normals;
        partial.series[0].months[0].maximum = None;
        assert_eq!(
            partial.validate(),
            Err(ProvisionalOriginClimateEvidenceError::InvalidNormals)
        );
    }
}
