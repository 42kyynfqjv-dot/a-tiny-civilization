use serde::{Deserialize, Deserializer, Serialize, de};
use thiserror::Error;

use crate::{
    CanonicalHashError, Digest, EntityId, EventId, EventSequence, SimTick, SpeciesIdentity,
    WorldId, WorldManifest,
};

pub const EVENT_SCHEMA_VERSION: u16 = 1;

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
    pub fn new(
        world_id: WorldId,
        sequence: EventSequence,
        tick: SimTick,
        ruleset_version: u32,
        previous_hash: Digest,
        events: Vec<DomainEvent>,
        post_state_hash: Digest,
    ) -> Result<Self, EventBatchError> {
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
            event_schema_version: EVENT_SCHEMA_VERSION,
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
        if self.event_schema_version != EVENT_SCHEMA_VERSION {
            return Err(EventBatchError::UnsupportedSchema(
                self.event_schema_version,
            ));
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
    use crate::WorldSeed;
    use uuid::Uuid;

    fn manifest() -> WorldManifest {
        WorldManifest::new(
            WorldId::from_uuid(Uuid::from_u128(0x42)),
            WorldSeed::new(17),
            1,
        )
    }

    #[test]
    fn batch_hash_covers_event_identity_payload_and_post_state() {
        let manifest = manifest();
        let batch = EventBatch::new(
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
}
