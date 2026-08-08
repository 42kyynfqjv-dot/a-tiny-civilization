//! Point-near, source-addressable fauna occurrence evidence.
//!
//! A retained observation corroborates recent local presence. It is not abundance,
//! habitat suitability, native status, or permission to create an organism.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{Digest, GeographicCoordinateE7};

pub const LOCAL_FAUNA_OCCURRENCE_EVIDENCE_SCHEMA_VERSION: u16 = 1;
pub const LOCAL_FAUNA_OCCURRENCE_EVIDENCE_MEDIA_TYPE: &str =
    "application/vnd.atinycivilization.local-fauna-occurrence-evidence+json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalFaunaOccurrenceEvidenceSet {
    pub evidence_schema_version: u16,
    pub source_manifest_digest: Digest,
    pub query_latitude_e7: i32,
    pub query_longitude_e7: i32,
    pub radius_kilometers: u16,
    pub records: Vec<LocalFaunaOccurrenceRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalFaunaOccurrenceRecord {
    pub observation_id: u64,
    pub inaturalist_taxon_id: u64,
    pub scientific_name: String,
    pub observed_on: String,
    pub observation_license: String,
    pub source_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positional_accuracy_meters: Option<u32>,
}

impl LocalFaunaOccurrenceEvidenceSet {
    pub fn validate(&self) -> Result<(), LocalFaunaOccurrenceEvidenceError> {
        if self.evidence_schema_version != LOCAL_FAUNA_OCCURRENCE_EVIDENCE_SCHEMA_VERSION {
            return Err(LocalFaunaOccurrenceEvidenceError::UnsupportedSchema);
        }
        if self.source_manifest_digest == Digest::ZERO
            || !(1..=100).contains(&self.radius_kilometers)
            || self.records.is_empty()
        {
            return Err(LocalFaunaOccurrenceEvidenceError::InvalidEvidenceSet);
        }
        GeographicCoordinateE7::new(self.query_latitude_e7, self.query_longitude_e7)
            .map_err(|_| LocalFaunaOccurrenceEvidenceError::InvalidEvidenceSet)?;
        for record in &self.records {
            record.validate()?;
        }
        if self
            .records
            .windows(2)
            .any(|pair| pair[0].observation_id >= pair[1].observation_id)
        {
            return Err(LocalFaunaOccurrenceEvidenceError::NonCanonicalOrder);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, LocalFaunaOccurrenceEvidenceError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| LocalFaunaOccurrenceEvidenceError::Encoding(error.to_string()))
    }

    pub fn from_canonical_slice(bytes: &[u8]) -> Result<Self, LocalFaunaOccurrenceEvidenceError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| LocalFaunaOccurrenceEvidenceError::Decode(error.to_string()))?;
        if value.canonical_bytes()? != bytes {
            return Err(LocalFaunaOccurrenceEvidenceError::NonCanonicalEncoding);
        }
        Ok(value)
    }

    #[must_use]
    pub fn corroborates_taxon(&self, inaturalist_taxon_id: u64) -> bool {
        self.records
            .iter()
            .any(|record| record.inaturalist_taxon_id == inaturalist_taxon_id)
    }
}

impl LocalFaunaOccurrenceRecord {
    fn validate(&self) -> Result<(), LocalFaunaOccurrenceEvidenceError> {
        if self.observation_id == 0
            || self.inaturalist_taxon_id == 0
            || self.scientific_name.trim().is_empty()
            || !matches!(self.observation_license.as_str(), "cc0" | "cc-by")
            || self.source_url
                != format!(
                    "https://www.inaturalist.org/observations/{}",
                    self.observation_id
                )
            || !date(&self.observed_on)
        {
            return Err(LocalFaunaOccurrenceEvidenceError::InvalidRecord);
        }
        Ok(())
    }
}

fn date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        && value[5..7]
            .parse::<u8>()
            .is_ok_and(|month| (1..=12).contains(&month))
        && value[8..10]
            .parse::<u8>()
            .is_ok_and(|day| (1..=31).contains(&day))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LocalFaunaOccurrenceEvidenceError {
    #[error("unsupported local-fauna occurrence evidence schema")]
    UnsupportedSchema,
    #[error("invalid local-fauna occurrence evidence set")]
    InvalidEvidenceSet,
    #[error("invalid local-fauna occurrence record")]
    InvalidRecord,
    #[error("local-fauna occurrence records are not strictly ordered")]
    NonCanonicalOrder,
    #[error("local-fauna occurrence evidence is not canonical JSON")]
    NonCanonicalEncoding,
    #[error("decode local-fauna occurrence evidence: {0}")]
    Decode(String),
    #[error("encode local-fauna occurrence evidence: {0}")]
    Encoding(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(observation_id: u64, taxon_id: u64) -> LocalFaunaOccurrenceRecord {
        LocalFaunaOccurrenceRecord {
            observation_id,
            inaturalist_taxon_id: taxon_id,
            scientific_name: "Cyrtonyx montezumae".to_owned(),
            observed_on: "2025-03-14".to_owned(),
            observation_license: "cc-by".to_owned(),
            source_url: format!("https://www.inaturalist.org/observations/{observation_id}"),
            positional_accuracy_meters: Some(20),
        }
    }

    #[test]
    fn occurrence_evidence_is_canonical_and_taxon_search_is_explicit() {
        let evidence = LocalFaunaOccurrenceEvidenceSet {
            evidence_schema_version: 1,
            source_manifest_digest: Digest::sha256(b"source manifest"),
            query_latitude_e7: 236_449_522,
            query_longitude_e7: -1_034_974_258,
            radius_kilometers: 75,
            records: vec![record(10, 1_392), record(20, 18_167)],
        };
        let bytes = evidence.canonical_bytes().expect("canonical evidence");
        assert_eq!(
            LocalFaunaOccurrenceEvidenceSet::from_canonical_slice(&bytes),
            Ok(evidence.clone())
        );
        assert!(evidence.corroborates_taxon(1_392));
        assert!(!evidence.corroborates_taxon(999));
    }

    #[test]
    fn occurrence_evidence_rejects_reordering_and_noncommercial_records() {
        let mut evidence = LocalFaunaOccurrenceEvidenceSet {
            evidence_schema_version: 1,
            source_manifest_digest: Digest::sha256(b"source manifest"),
            query_latitude_e7: 0,
            query_longitude_e7: 0,
            radius_kilometers: 75,
            records: vec![record(20, 1), record(10, 2)],
        };
        assert_eq!(
            evidence.validate(),
            Err(LocalFaunaOccurrenceEvidenceError::NonCanonicalOrder)
        );
        evidence.records.sort_by_key(|record| record.observation_id);
        evidence.records[0].observation_license = "cc-by-nc".to_owned();
        assert_eq!(
            evidence.validate(),
            Err(LocalFaunaOccurrenceEvidenceError::InvalidRecord)
        );
    }
}
