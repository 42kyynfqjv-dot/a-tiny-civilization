//! Label-free embodied inputs and primitive motor acts.
//!
//! These are contracts for canonical cognition and action planning. They deliberately
//! express sensed properties and bodily capability, never a culturally learned use.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Digest, EntityId, SpeciesIdentity};

/// A source-pinned power value carried by a particular organism.
///
/// This preserves either one retained observation or one explicitly classified
/// provisional assumption without making an unsupported claim that it is a basal
/// rate, daily requirement, or environmental response.
/// A later ruleset may use it only alongside an explicit conversion and exposure
/// model. Keeping the commitment in canonical body state prevents a runner or
/// projection from silently changing the evidence that shaped a living world.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MetabolicRateCommitment {
    pub commitment_schema_version: u16,
    #[serde(
        default,
        skip_serializing_if = "PhysiologicalEvidenceBasis::is_source_measurement"
    )]
    pub evidence_basis: PhysiologicalEvidenceBasis,
    pub profile_set_digest: Digest,
    pub observed_species: SpeciesIdentity,
    pub source_record_id: String,
    pub source_record_digest: Digest,
    pub measured_power_value: i64,
    pub measured_power_decimal_places: u8,
}

pub const LEGACY_METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION: u16 = 1;
pub const METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION: u16 = 2;
pub const PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION: u16 = 1;

impl MetabolicRateCommitment {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        match self.commitment_schema_version {
            LEGACY_METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION => {
                if self.evidence_basis != PhysiologicalEvidenceBasis::SourceMeasurement {
                    return Err(EmbodimentError::InvalidMetabolicCommitment);
                }
            }
            METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION => {}
            _ => return Err(EmbodimentError::UnsupportedMetabolicCommitmentSchema),
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

/// The weakest evidence class used by any parameter in a committed regulation
/// profile. This makes an engineering placeholder impossible to present as a sourced
/// measurement merely because the same profile also contains measured values.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysiologicalEvidenceBasis {
    #[default]
    SourceMeasurement,
    LiteratureApproximation,
    EngineeringAssumption,
}

impl PhysiologicalEvidenceBasis {
    #[must_use]
    pub const fn is_source_measurement(&self) -> bool {
        matches!(self, Self::SourceMeasurement)
    }
}

/// Immutable species-specific parameters for the first canonical bodily regulator.
///
/// The profile digest addresses the independently reviewable evidence/assumption
/// artifact. Values are physical durations, temperatures, energy, and exposure—not
/// agent-facing conclusions about how a pressure can be relieved.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysiologicalRegulationCommitment {
    pub commitment_schema_version: u16,
    pub profile_id: String,
    pub profile_digest: Digest,
    pub species: SpeciesIdentity,
    pub evidence_basis: PhysiologicalEvidenceBasis,
    pub usable_energy_reserve_joules: u64,
    pub hydration_failure_seconds: u64,
    pub fatigue_failure_seconds: u64,
    pub fatigue_recovery_seconds: u64,
    pub thermoneutral_min_millicelsius: i32,
    pub thermoneutral_max_millicelsius: i32,
    /// Integrated temperature distance outside the thermoneutral range. For
    /// example, 1,000 millicelsius of excess for 60 seconds consumes 60,000 units.
    pub thermal_failure_millicelsius_seconds: u64,
    pub thermal_recovery_seconds: u64,
}

impl PhysiologicalRegulationCommitment {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        if self.commitment_schema_version != PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION {
            return Err(EmbodimentError::UnsupportedPhysiologicalRegulationSchema);
        }
        self.species
            .validate()
            .map_err(|_| EmbodimentError::InvalidPhysiologicalRegulationCommitment)?;
        if !is_technical(&self.profile_id)
            || self.profile_digest == Digest::ZERO
            || self.usable_energy_reserve_joules == 0
            || self.hydration_failure_seconds == 0
            || self.fatigue_failure_seconds == 0
            || self.fatigue_recovery_seconds == 0
            || self.thermoneutral_min_millicelsius >= self.thermoneutral_max_millicelsius
            || self.thermal_failure_millicelsius_seconds == 0
            || self.thermal_recovery_seconds == 0
        {
            return Err(EmbodimentError::InvalidPhysiologicalRegulationCommitment);
        }
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

/// Canonical bounded bodily pressures. Intensities are normalized regulator state,
/// not physical measurements or instructions. Zero means absent and `u16::MAX`
/// means that the corresponding regulation budget has been exhausted.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BodilyNeedState {
    pub energy_deficit: u16,
    pub hydration_deficit: u16,
    pub thermal_discomfort: u16,
    pub pain: u16,
    pub fatigue: u16,
}

impl BodilyNeedState {
    #[must_use]
    pub const fn intensity(self, kind: NeedKind) -> u16 {
        match kind {
            NeedKind::EnergyDeficit => self.energy_deficit,
            NeedKind::HydrationDeficit => self.hydration_deficit,
            NeedKind::ThermalDiscomfort => self.thermal_discomfort,
            NeedKind::Pain => self.pain,
            NeedKind::Fatigue => self.fatigue,
        }
    }

    #[must_use]
    pub const fn signal(self, kind: NeedKind) -> Option<NeedSignal> {
        let intensity = self.intensity(kind);
        if intensity == 0 {
            None
        } else {
            Some(NeedSignal { kind, intensity })
        }
    }

    #[must_use]
    pub const fn is_clear(&self) -> bool {
        self.energy_deficit == 0
            && self.hydration_deficit == 0
            && self.thermal_discomfort == 0
            && self.pain == 0
            && self.fatigue == 0
    }
}

/// Exact causal load retained beneath the normalized need signals. These accumulators
/// prevent repeated quantization from turning a tiny per-tick exposure into a whole
/// pressure unit. The corresponding need state is stored so replay can audit both the
/// physical integration and the internal signal that was available to policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BodilyRegulationState {
    /// `measured_power_value * seconds`; the metabolic commitment supplies the
    /// decimal scale and the profile supplies the reserve in joules.
    pub energy_load_scaled_joules: u64,
    pub hydration_load_seconds: u64,
    /// Common units whose capacity is
    /// `fatigue_failure_seconds * fatigue_recovery_seconds`.
    pub fatigue_load_second_squared: u64,
    /// Common units whose capacity is `thermal_failure_millicelsius_seconds *
    /// thermal_recovery_seconds`.
    pub thermal_load_millicelsius_second_squared: u64,
    pub needs: BodilyNeedState,
}

impl BodilyRegulationState {
    #[must_use]
    pub const fn is_clear(&self) -> bool {
        self.energy_load_scaled_joules == 0
            && self.hydration_load_seconds == 0
            && self.fatigue_load_second_squared == 0
            && self.thermal_load_millicelsius_second_squared == 0
            && self.needs.is_clear()
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

pub const ACTION_VALUE_STATE_SCHEMA_VERSION: u16 = 1;
pub const ACTION_VALUE_MIN: i16 = -128;
pub const ACTION_VALUE_MAX: i16 = 128;

/// Bounded experience retained for one primitive action kind. The value is a
/// label-free association with total bodily pressure change, not a belief about an
/// object, a use, a goal, or the scientific cause of an outcome.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ActionValueState {
    pub value_schema_version: u16,
    pub action_kind: PrimitiveActionKind,
    pub observations: u32,
    pub value: i16,
}

impl ActionValueState {
    pub fn validate(self) -> Result<(), EmbodimentError> {
        if self.value_schema_version != ACTION_VALUE_STATE_SCHEMA_VERSION {
            return Err(EmbodimentError::UnsupportedActionValueSchema);
        }
        if self.observations == 0 || !(ACTION_VALUE_MIN..=ACTION_VALUE_MAX).contains(&self.value) {
            return Err(EmbodimentError::InvalidActionValueState);
        }
        Ok(())
    }
}

/// A primitive bodily command with an optional local target; effects are resolved by
/// world physics, not by an action name implying a desired cultural outcome.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrimitiveAction {
    pub kind: PrimitiveActionKind,
    pub target_id: Option<EntityId>,
    /// Bounded intensity for movement, force, or signal amplitude.
    pub intensity: u16,
    /// Optional label-free contact region for a force action. It is a motor
    /// coordinate, not a glyph, character, purpose, or inferred surface meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_region: Option<u8>,
}

impl PrimitiveAction {
    pub fn validate(&self) -> Result<(), EmbodimentError> {
        if self.intensity == 0 {
            return Err(EmbodimentError::ZeroActionIntensity);
        }
        if self.contact_region.is_some_and(|region| region >= 8)
            || (self.contact_region.is_some() && self.kind != PrimitiveActionKind::ApplyForce)
        {
            return Err(EmbodimentError::InvalidContactRegion);
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
    #[error("unsupported physiological-regulation commitment schema")]
    UnsupportedPhysiologicalRegulationSchema,
    #[error("invalid physiological-regulation commitment")]
    InvalidPhysiologicalRegulationCommitment,
    #[error("unsupported action-value state schema")]
    UnsupportedActionValueSchema,
    #[error("invalid action-value state")]
    InvalidActionValueState,
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
    #[error("contact region must be absent or a bounded apply-force motor coordinate")]
    InvalidContactRegion,
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
            contact_region: Some(7),
        }
        .validate()
        .expect("physical action");
        assert!(
            PrimitiveAction {
                kind: PrimitiveActionKind::Swallow,
                target_id: None,
                intensity: 0,
                contact_region: None,
            }
            .validate()
            .is_err()
        );
        assert!(
            PrimitiveAction {
                kind: PrimitiveActionKind::Move,
                target_id: None,
                intensity: 1,
                contact_region: Some(0),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn action_values_are_bounded_observations_not_use_labels() {
        ActionValueState {
            value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
            action_kind: PrimitiveActionKind::Swallow,
            observations: 1,
            value: 7,
        }
        .validate()
        .expect("bounded action experience");
        assert!(matches!(
            ActionValueState {
                value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
                action_kind: PrimitiveActionKind::Swallow,
                observations: 0,
                value: 0,
            }
            .validate(),
            Err(EmbodimentError::InvalidActionValueState)
        ));
    }

    #[test]
    fn physiological_profiles_are_species_bound_and_assumptions_stay_explicit() {
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("real taxon");
        let profile = PhysiologicalRegulationCommitment {
            commitment_schema_version: PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION,
            profile_id: "provisional-human-v1".to_owned(),
            profile_digest: Digest::sha256(b"explicit engineering assumptions"),
            species,
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            usable_energy_reserve_joules: 1_000_000,
            hydration_failure_seconds: 259_200,
            fatigue_failure_seconds: 86_400,
            fatigue_recovery_seconds: 28_800,
            thermoneutral_min_millicelsius: 10_000,
            thermoneutral_max_millicelsius: 30_000,
            thermal_failure_millicelsius_seconds: 86_400_000,
            thermal_recovery_seconds: 43_200,
        };
        profile.validate().expect("valid explicit profile");

        let mut invalid = profile;
        invalid.thermoneutral_min_millicelsius = invalid.thermoneutral_max_millicelsius;
        assert!(matches!(
            invalid.validate(),
            Err(EmbodimentError::InvalidPhysiologicalRegulationCommitment)
        ));

        let needs = BodilyNeedState {
            energy_deficit: 7,
            ..BodilyNeedState::default()
        };
        assert_eq!(
            needs.signal(NeedKind::EnergyDeficit),
            Some(NeedSignal {
                kind: NeedKind::EnergyDeficit,
                intensity: 7,
            })
        );
        assert_eq!(needs.signal(NeedKind::Pain), None);
    }

    #[test]
    fn metabolic_commitments_preserve_legacy_bytes_and_label_assumptions() {
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("real taxon");
        let mut commitment = MetabolicRateCommitment {
            commitment_schema_version: LEGACY_METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
            evidence_basis: PhysiologicalEvidenceBasis::SourceMeasurement,
            profile_set_digest: Digest::sha256(b"retained profiles"),
            observed_species: species,
            source_record_id: "retained-row-1".to_owned(),
            source_record_digest: Digest::sha256(b"retained row"),
            measured_power_value: 125,
            measured_power_decimal_places: 3,
        };
        commitment.validate().expect("legacy source commitment");
        let legacy_bytes = serde_json::to_vec(&commitment).expect("legacy JSON");
        assert!(
            !String::from_utf8(legacy_bytes.clone())
                .expect("UTF-8 JSON")
                .contains("evidence_basis")
        );
        assert_eq!(
            serde_json::from_slice::<MetabolicRateCommitment>(&legacy_bytes)
                .expect("decode legacy commitment"),
            commitment
        );

        commitment.commitment_schema_version = METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION;
        commitment.evidence_basis = PhysiologicalEvidenceBasis::EngineeringAssumption;
        commitment
            .validate()
            .expect("explicit provisional assumption");
        assert!(
            String::from_utf8(serde_json::to_vec(&commitment).expect("assumption JSON"))
                .expect("UTF-8 JSON")
                .contains("engineering_assumption")
        );

        commitment.commitment_schema_version = LEGACY_METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION;
        assert_eq!(
            commitment.validate(),
            Err(EmbodimentError::InvalidMetabolicCommitment)
        );
    }
}
