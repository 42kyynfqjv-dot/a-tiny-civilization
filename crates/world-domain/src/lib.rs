//! Durable, replay-safe domain primitives shared by the engine and its adapters.

mod celestial;
mod configuration;
mod embodiment;
mod event;
mod geographic;
mod hash;
mod identity;
mod illumination;
mod manifest;
mod solar;
mod spatial;
mod tide;
mod time;

pub use celestial::{
    CartesianAxis, CartesianMillimetres, CelestialError, CelestialState, TdbSecondsSinceJ2000,
    TickDurationSeconds, tdb_seconds_at_tick,
};
pub use configuration::{
    CapacityExhaustionPolicy, EarthResolutionLevels, ExecutionScale, FullEarthGrid,
    LEGACY_WORLD_CONFIGURATION_SCHEMA_VERSION, PartitionedExecution, PersonRepresentation,
    S2Projection, SchedulerKind, SpatialGrid, WORLD_CONFIGURATION_SCHEMA_VERSION,
    WorldConfiguration, WorldConfigurationError, WorldDataBundleReference, WorldGeometry,
};
pub use embodiment::{
    EmbodimentError, NeedKind, NeedSignal, PerceptionChannel, PrimitiveAction, PrimitiveActionKind,
    PropertyReading, SituatedPerception,
};
pub use event::{
    BirthCategory, CONFIGURED_EVENT_SCHEMA_VERSION, CategoryError, DeathCause, DomainEvent,
    EMBODIED_POSITION_EVENT_SCHEMA_VERSION, EVENT_SCHEMA_VERSION, EventBatch, EventBatchError,
    EventRecord, LEGACY_EVENT_SCHEMA_VERSION, OrganismRole,
};
pub use geographic::{
    GeographicCoordinateE7, GeographicCoordinateHalfArcsecond, GeographicRoutingError, S2FaceIj,
    S2FaceRay, S2FaceUv, decode_s2_face_ij, route_geographic_to_s2, route_half_arcsecond_to_s2,
    s2_face_ij_center_uv, s2_face_ij_vertex_uv, s2_face_uv_to_ray, s2_ray_to_geographic_e7,
};
pub use hash::{CanonicalHashError, Digest};
pub use identity::{EntityId, EventId, WorldId};
pub use illumination::{
    EarthFixedSunVector, EcefSurfacePosition, IlluminationGeometryError, LocalIlluminationGeometry,
    RadialHorizonClassification, SunVectorFrame,
};
pub use manifest::{SpeciesIdentity, SpeciesIdentityError, WorldManifest};
pub use solar::{
    CanonicalPositiveRational, CanonicalPositiveRationalError, PinnedSolarReferenceDistance,
    SolarDistanceForcing, SolarDistanceForcingError,
};
pub use spatial::{MAX_S2_LEVEL, S2CellId, S2CellIdError};
pub use tide::{
    SignedSquaredMillimetres, SquaredMillimetres, TideBody, TideBodyGeometry, TideGeometry,
    TideGeometryError,
};
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
