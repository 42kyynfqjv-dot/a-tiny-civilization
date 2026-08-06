//! Durable, replay-safe domain primitives shared by the engine and its adapters.

mod configuration;
mod event;
mod hash;
mod identity;
mod manifest;
mod time;

pub use configuration::{
    CapacityExhaustionPolicy, EarthResolutionLevels, ExecutionScale, FullEarthGrid,
    LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION, PartitionedExecution, PersonRepresentation,
    S2Projection, SchedulerKind, SpatialGrid, WORLD_CONFIGURATION_SCHEMA_VERSION,
    WorldConfiguration, WorldConfigurationError, WorldDataBundleReference, WorldGeometry,
};
pub use event::{
    BirthCategory, CategoryError, DeathCause, DomainEvent, EVENT_SCHEMA_VERSION, EventBatch,
    EventBatchError, EventRecord, LEGACY_EVENT_SCHEMA_VERSION, OrganismRole,
};
pub use hash::{CanonicalHashError, Digest};
pub use identity::{EntityId, EventId, WorldId};
pub use manifest::{SpeciesIdentity, SpeciesIdentityError, WorldManifest};
pub use time::{EventSequence, SequenceOverflow, SimTick, TimeOverflow, WorldSeed};

use serde::{Deserialize, Serialize};

/// Durable lifecycle state for a world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Initializing,
    Running,
    Extinct,
    Archived,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_status_uses_stable_snake_case_json() {
        let encoded = serde_json::to_string(&WorldStatus::Initializing);
        assert!(matches!(encoded.as_deref(), Ok("\"initializing\"")));
    }
}
