//! Label-free embodied inputs and primitive motor acts.
//!
//! These are contracts for canonical cognition and action planning. They deliberately
//! express sensed properties and bodily capability, never a culturally learned use.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Digest, EntityId, SpeciesIdentity};

/// A source-pinned measured power carried by a particular organism.
///
/// This preserves exactly one retained observation without making an unsupported
/// claim that it is a basal rate, daily requirement, or environmental response.
/// A later ruleset may use it only alongside an explicit conversion and exposure
/// model. Keeping the commitment in canonical body state prevents a runner or
/// projection from silently changing the evidence that shaped a living world.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MetabolicRateCommitment {
    pub commitment_schema_version: u16,
    pub profile_set_digest: Digest,
    pub observed_species: SpeciesIdentity,
    pub source_record_id: String,
    pub source_record_digest: Digest,
    pub measured_power_value: i64,
    pub measured_power_decimal_places: u8,
}

pub const METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION: u16 = 1;

impl MetabolicRateCommitment {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        if self.commitment_schema_version != METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION {
            return Err(EmbodimentError::UnsupportedMetabolicCommitmentSchema);
        }
        if self.profile_set_digest == Digest::ZERO
            || self.source_record_digest == Digest::ZERO
            || !is_technical(&self.source_record_id)
            || self.measured_power_value <= 0
            || self.measured_power_decimal_places > 9
        {
            return Err(EmbodimentError::InvalidMetabolicCommitment);
        }
        self.observed_species
            .validate()
            .map_err(|_| EmbodimentError::InvalidMetabolicCommitment)?;
        Ok(())
    }
}

const FORBIDDEN_PRIVILEGED_CODES: &[&str] = &[
    "building",
    "edible",
    "food",
    "invention",
    "medicine",
    "paper",
    "prey",
    "shelter",
    "technology",
    "tool",
    "weapon",
    "writing",
];

/// A physiological pressure, not a conclusion about how to resolve it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedKind {
    EnergyDeficit,
    HydrationDeficit,
    ThermalDiscomfort,
    Pain,
    Fatigue,
}

/// A bounded bodily signal available to one organism's deterministic brain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NeedSignal {
    pub kind: NeedKind,
    /// Zero means absent; larger values are more urgent but do not prescribe an action.
    pub intensity: u16,
}

impl NeedSignal {
    pub fn validate(self) -> Result<(), EmbodimentError> {
        if self.intensity == 0 {
            return Err(EmbodimentError::ZeroNeedIntensity);
        }
        Ok(())
    }
}

/// One sensory or interoceptive channel. Channels describe access, not interpretation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionChannel {
    Vision,
    Touch,
    Sound,
    Odour,
    Taste,
    Interoception,
}

/// A quantized observed property. `property_code` may identify a physical dimension
/// such as `temperature` or `surface_roughness`, but never an affordance or modern use.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PropertyReading {
    pub channel: PerceptionChannel,
    pub property_code: String,
    pub quantized_value: i32,
    pub uncertainty: u16,
}

impl PropertyReading {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        if !is_code(&self.property_code) {
            return Err(EmbodimentError::InvalidPropertyCode(
                self.property_code.clone(),
            ));
        }
        if FORBIDDEN_PRIVILEGED_CODES.contains(&self.property_code.as_str()) {
            return Err(EmbodimentError::PrivilegedPropertyCode(
                self.property_code.clone(),
            ));
        }
        Ok(())
    }
}

/// A deterministic bundle of direct readings from one subject or the organism's body.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SituatedPerception {
    pub subject_id: Option<EntityId>,
    pub readings: Vec<PropertyReading>,
}

impl SituatedPerception {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        if self.readings.is_empty() {
            return Err(EmbodimentError::EmptyPerception);
        }
        let mut previous: Option<(&PerceptionChannel, &str)> = None;
        for reading in &self.readings {
            reading.validate()?;
            let key = (&reading.channel, reading.property_code.as_str());
            if let Some(last) = previous
                && key <= last
            {
                return Err(EmbodimentError::UnsortedOrDuplicateReadings);
            }
            previous = Some(key);
        }
        Ok(())
    }
}

/// Closed, use-neutral motor grammar. It contains no invention, material, social-role,
/// food, weapon, tool, writing, or construction action.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveActionKind {
    Move,
    Orient,
    Reach,
    Grasp,
    Release,
    ApplyForce,
    Bite,
    Chew,
    Swallow,
    Rest,
    EmitSignal,
}

/// A primitive bodily command with an optional local target; effects are resolved by
/// world physics, not by an action name implying a desired cultural outcome.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrimitiveAction {
    pub kind: PrimitiveActionKind,
    pub target_id: Option<EntityId>,
    /// Bounded intensity for movement, force, or signal amplitude.
    pub intensity: u16,
}

impl PrimitiveAction {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        if self.intensity == 0 {
            return Err(EmbodimentError::ZeroActionIntensity);
        }
        Ok(())
    }
}

fn is_code(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_technical(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EmbodimentError {
    #[error("unsupported metabolic-rate commitment schema")]
    UnsupportedMetabolicCommitmentSchema,
    #[error("invalid metabolic-rate commitment")]
    InvalidMetabolicCommitment,
    #[error("need signal intensity must be greater than zero")]
    ZeroNeedIntensity,
    #[error("perception property code {0:?} is invalid")]
    InvalidPropertyCode(String),
    #[error("perception property code {0:?} is a forbidden privileged conclusion")]
    PrivilegedPropertyCode(String),
    #[error("situated perception needs at least one reading")]
    EmptyPerception,
    #[error("perception readings must be strictly ordered by channel and property code")]
    UnsortedOrDuplicateReadings,
    #[error("primitive action intensity must be greater than zero")]
    ZeroActionIntensity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_readings_are_allowed_but_privileged_affordances_are_rejected() {
        let perception = SituatedPerception {
            subject_id: None,
            readings: vec![
                PropertyReading {
                    channel: PerceptionChannel::Touch,
                    property_code: "surface_roughness".to_owned(),
                    quantized_value: 12,
                    uncertainty: 1,
                },
                PropertyReading {
                    channel: PerceptionChannel::Touch,
                    property_code: "temperature".to_owned(),
                    quantized_value: 293,
                    uncertainty: 2,
                },
            ],
        };
        perception
            .validate()
            .expect("physical properties are valid");
        let forbidden = PropertyReading {
            channel: PerceptionChannel::Taste,
            property_code: "food".to_owned(),
            quantized_value: 1,
            uncertainty: 0,
        };
        assert!(forbidden.validate().is_err());
    }

    #[test]
    fn action_grammar_has_bodily_operations_not_cultural_outcomes() {
        PrimitiveAction {
            kind: PrimitiveActionKind::ApplyForce,
            target_id: None,
            intensity: 1,
        }
        .validate()
        .expect("physical action");
        assert!(
            PrimitiveAction {
                kind: PrimitiveActionKind::Swallow,
                target_id: None,
                intensity: 0,
            }
            .validate()
            .is_err()
        );
    }
}
