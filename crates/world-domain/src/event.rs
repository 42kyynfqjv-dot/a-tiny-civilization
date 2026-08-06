use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::{
    CanonicalHashError, Digest, EntityId, EventId, EventSequence, PrimitiveAction, SimTick,
    SituatedPerception, SpeciesIdentity, WorldConfiguration, WorldId, WorldManifest,
};

pub const LEGACY_EVENT_SCHEMA_VERSION: u16 = 1;
pub const CONFIGURED_EVENT_SCHEMA_VERSION: u16 = 2;
pub const EVENT_SCHEMA_VERSION: u16 = 3;

/// Engine-level participation tier. This is never exposed as an agent concept.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganismRole {
    Person,
    Fauna,
}

/// Versioned, species-aware category used by reproduction and later observer matching.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BirthCategory(String);

impl BirthCategory {
    pub fn new(value: impl Into<String>) -> Result<Self, CategoryError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(CategoryError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for BirthCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("birth category must be a non-empty lowercase ASCII slug")]
pub struct CategoryError;

/// A versioned physical mortality mechanism code, not observer prose.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeathCause {
    pub mechanism: String,
}

/// Facts that can change canonical world state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum DomainEvent {
    WorldStarted {
        manifest: WorldManifest,
    },
    WorldConfigured {
        configuration: WorldConfiguration,
    },
    OrganismInitialized {
        organism_id: EntityId,
        species: SpeciesIdentity,
        role: OrganismRole,
        birth_category: BirthCategory,
        initial_age_ticks: u64,
        location_id: Option<EntityId>,
    },
    TickAdvanced {
        from: SimTick,
        to: SimTick,
    },
    OrganismBorn {
        organism_id: EntityId,
        species: SpeciesIdentity,
        role: OrganismRole,
        birth_category: BirthCategory,
        parent_ids: Vec<EntityId>,
        location_id: Option<EntityId>,
    },
    OrganismDied {
        organism_id: EntityId,
        cause: DeathCause,
    },
    /// Direct label-free sensory evidence available to one living organism.
    OrganismPerceived {
        organism_id: EntityId,
        perception: SituatedPerception,
    },
    /// One chosen use-neutral bodily operation. World physics resolves its effect.
    OrganismActed {
        organism_id: EntityId,
        action: PrimitiveAction,
    },
    WorldExtinct,
    WorldArchived,
}

/// An event plus its deterministic identity within a committed batch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventRecord {
    pub event_id: EventId,
    pub index: u32,
    pub event: DomainEvent,
}

/// One atomic deterministic transition and its tamper-evident hash-chain link.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventBatch {
    pub event_schema_version: u16,
    pub world_id: WorldId,
    pub sequence: EventSequence,
    pub tick: SimTick,
    pub ruleset_version: u32,
    pub previous_hash: Digest,
    pub events: Vec<EventRecord>,
    pub post_state_hash: Digest,
    pub batch_hash: Digest,
}

impl EventBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_schema_version: u16,
        world_id: WorldId,
        sequence: EventSequence,
        tick: SimTick,
        ruleset_version: u32,
        previous_hash: Digest,
        events: Vec<DomainEvent>,
        post_state_hash: Digest,
    ) -> Result<Self, EventBatchError> {
        validate_schema_version(event_schema_version)?;
        for event in &events {
            validate_event_for_schema(event_schema_version, event)?;
        }
        if sequence == EventSequence::ZERO {
            return Err(EventBatchError::ZeroSequence);
        }
        if ruleset_version == 0 {
            return Err(EventBatchError::ZeroRulesetVersion);
        }
        if events.is_empty() {
            return Err(EventBatchError::Empty);
        }

        let records = events
            .into_iter()
            .enumerate()
            .map(|(index, event)| {
                let index = u32::try_from(index).map_err(|_| EventBatchError::TooManyEvents)?;
                Ok(EventRecord {
                    event_id: EventId::for_position(world_id, sequence.get(), index),
                    index,
                    event,
                })
            })
            .collect::<Result<Vec<_>, EventBatchError>>()?;

        let mut batch = Self {
            event_schema_version,
            world_id,
            sequence,
            tick,
            ruleset_version,
            previous_hash,
            events: records,
            post_state_hash,
            batch_hash: Digest::ZERO,
        };
        batch.batch_hash = batch.calculate_hash()?;
        Ok(batch)
    }

    pub fn verify_integrity(&self) -> Result<(), EventBatchError> {
        validate_schema_version(self.event_schema_version)?;
        for record in &self.events {
            validate_event_for_schema(self.event_schema_version, &record.event)?;
        }
        if self.sequence == EventSequence::ZERO {
            return Err(EventBatchError::ZeroSequence);
        }
        if self.ruleset_version == 0 {
            return Err(EventBatchError::ZeroRulesetVersion);
        }
        if self.events.is_empty() {
            return Err(EventBatchError::Empty);
        }

        for (expected_index, record) in self.events.iter().enumerate() {
            let expected_index =
                u32::try_from(expected_index).map_err(|_| EventBatchError::TooManyEvents)?;
            let expected_id =
                EventId::for_position(self.world_id, self.sequence.get(), expected_index);
            if record.index != expected_index || record.event_id != expected_id {
                return Err(EventBatchError::InvalidEventIdentity {
                    index: expected_index,
                });
            }
        }

        let calculated = self.calculate_hash()?;
        if calculated != self.batch_hash {
            return Err(EventBatchError::HashMismatch {
                expected: self.batch_hash,
                calculated,
            });
        }
        Ok(())
    }

    fn calculate_hash(&self) -> Result<Digest, CanonicalHashError> {
        Digest::canonical(&BatchHashMaterial {
            event_schema_version: self.event_schema_version,
            world_id: self.world_id,
            sequence: self.sequence,
            tick: self.tick,
            ruleset_version: self.ruleset_version,
            previous_hash: self.previous_hash,
            events: &self.events,
            post_state_hash: self.post_state_hash,
        })
    }
}

fn validate_schema_version(event_schema_version: u16) -> Result<(), EventBatchError> {
    if !matches!(
        event_schema_version,
        LEGACY_EVENT_SCHEMA_VERSION | CONFIGURED_EVENT_SCHEMA_VERSION | EVENT_SCHEMA_VERSION
    ) {
        return Err(EventBatchError::UnsupportedSchema(event_schema_version));
    }
    Ok(())
}

fn validate_event_for_schema(
    event_schema_version: u16,
    event: &DomainEvent,
) -> Result<(), EventBatchError> {
    if event_schema_version == LEGACY_EVENT_SCHEMA_VERSION
        && matches!(event, DomainEvent::WorldConfigured { .. })
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    if event_schema_version < EVENT_SCHEMA_VERSION
        && matches!(
            event,
            DomainEvent::OrganismPerceived { .. } | DomainEvent::OrganismActed { .. }
        )
    {
        return Err(EventBatchError::EventRequiresNewerSchema);
    }
    match event {
        DomainEvent::OrganismPerceived { perception, .. } => perception
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        DomainEvent::OrganismActed { action, .. } => action
            .validate()
            .map_err(|error| EventBatchError::InvalidEmbodiedEvent(error.to_string()))?,
        _ => {}
    }
    Ok(())
}

#[derive(Serialize)]
struct BatchHashMaterial<'a> {
    event_schema_version: u16,
    world_id: WorldId,
    sequence: EventSequence,
    tick: SimTick,
    ruleset_version: u32,
    previous_hash: Digest,
    events: &'a [EventRecord],
    post_state_hash: Digest,
}

#[derive(Debug, Error)]
pub enum EventBatchError {
    #[error("event batch sequence zero is reserved for pre-genesis state")]
    ZeroSequence,
    #[error("ruleset version must be greater than zero")]
    ZeroRulesetVersion,
    #[error("event batch must contain at least one event")]
    Empty,
    #[error("event batch contains more than u32::MAX events")]
    TooManyEvents,
    #[error("event schema version {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error("event requires a newer event schema version")]
    EventRequiresNewerSchema,
    #[error("embodied event is invalid: {0}")]
    InvalidEmbodiedEvent(String),
    #[error("event identity at index {index} is not deterministic")]
    InvalidEventIdentity { index: u32 },
    #[error("event batch hash mismatch: stored {expected}, calculated {calculated}")]
    HashMismatch {
        expected: Digest,
        calculated: Digest,
    },
    #[error(transparent)]
    CanonicalHash(#[from] CanonicalHashError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PerceptionChannel, PrimitiveAction, PrimitiveActionKind, PropertyReading,
        SituatedPerception, SpatialGrid, WorldConfiguration, WorldDataBundleReference, WorldSeed,
    };
    use uuid::Uuid;

    fn manifest() -> WorldManifest {
        WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(0x42)),
            WorldSeed::new(17),
            1,
        )
    }

    fn configuration() -> WorldConfiguration {
        WorldConfiguration::new(
            300,
            SpatialGrid {
                epsg: 32_736,
                origin_easting_mm: 500_000_000,
                origin_northing_mm: 9_700_000_000,
                cell_size_mm: 10_000,
                width_cells: 100,
                height_cells: 100,
            },
            WorldDataBundleReference::new(
                1,
                "event-schema-test",
                "0.1.0",
                Digest::sha256(b"event schema test data"),
                "https://data.atinycivilization.com/event-schema-test/0.1.0.json",
                "CC-BY-4.0",
            )
            .expect("valid bundle reference"),
            10_000,
        )
        .expect("valid world configuration")
    }

    #[test]
    fn batch_hash_covers_event_identity_payload_and_post_state() {
        let manifest = manifest();
        let batch = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"post-state"),
        );
        let batch = batch.expect("valid test batch");
        assert!(batch.verify_integrity().is_ok());
        assert_eq!(
            batch.batch_hash.to_string(),
            "f205996446dd9978eb0ce784780c0ee689f14625b36bda0d3a9303a017c895a5"
        );
    }

    #[test]
    fn modified_event_is_detected() {
        let manifest = manifest();
        let mut batch = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldStarted {
                manifest: manifest.clone(),
            }],
            Digest::sha256(b"post-state"),
        )
        .expect("valid test batch");
        batch.events[0].event = DomainEvent::WorldArchived;

        assert!(matches!(
            batch.verify_integrity(),
            Err(EventBatchError::HashMismatch { .. })
        ));
    }

    #[test]
    fn legacy_schema_rejects_world_configuration() {
        let manifest = manifest();
        let result = EventBatch::new(
            LEGACY_EVENT_SCHEMA_VERSION,
            manifest.world_id,
            EventSequence::new(1),
            SimTick::ZERO,
            manifest.ruleset_version,
            Digest::ZERO,
            vec![DomainEvent::WorldConfigured {
                configuration: configuration(),
            }],
            Digest::sha256(b"post-state"),
        );

        assert!(matches!(
            result,
            Err(EventBatchError::EventRequiresNewerSchema)
        ));
    }

    #[test]
    fn embodied_events_require_schema_three_and_reject_privileged_readings() {
        let manifest = manifest();
        let perception = DomainEvent::OrganismPerceived {
            organism_id: EntityId::from_uuid(Uuid::from_u128(3)),
            perception: SituatedPerception {
                subject_id: None,
                readings: vec![PropertyReading {
                    channel: PerceptionChannel::Touch,
                    property_code: "tool".to_owned(),
                    quantized_value: 1,
                    uncertainty: 0,
                }],
            },
        };
        assert!(
            EventBatch::new(
                CONFIGURED_EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![perception.clone()],
                Digest::sha256(b"post-state"),
            )
            .is_err()
        );
        assert!(
            EventBatch::new(
                EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![perception],
                Digest::sha256(b"post-state"),
            )
            .is_err()
        );

        let action = DomainEvent::OrganismActed {
            organism_id: EntityId::from_uuid(Uuid::from_u128(3)),
            action: PrimitiveAction {
                kind: PrimitiveActionKind::ApplyForce,
                target_id: None,
                intensity: 1,
            },
        };
        assert!(
            EventBatch::new(
                EVENT_SCHEMA_VERSION,
                manifest.world_id,
                EventSequence::new(1),
                SimTick::ZERO,
                manifest.ruleset_version,
                Digest::ZERO,
                vec![action],
                Digest::sha256(b"post-state"),
            )
            .is_ok()
        );
    }
}
