//! Observer-only projections and supporter participation ports.
//!
//! This crate deliberately has no dependency on the simulation engine or runner. It
//! consumes already-committed birth facts and can attach an external observer label;
//! it cannot request, delay, select, or alter a birth.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    BirthCategory, Digest, DomainEvent, EntityId, EventBatch, EventId, EventSequence, OrganismRole,
    SimTick, SpeciesIdentity, WorldId, WorldStatus,
};

pub const PUBLIC_TIMELINE_PROJECTION_VERSION: u16 = 1;
pub const PUBLIC_TIMELINE_PROJECTION_NAME: &str = "public-timeline-v1";
pub const PUBLIC_ORGANISM_PROJECTION_VERSION: u16 = 1;
pub const PUBLIC_ORGANISM_PROJECTION_NAME: &str = "public-organism-v1";
pub const PUBLIC_FINDING_PROJECTION_VERSION: u16 = 1;
pub const PUBLIC_FINDING_PROJECTION_NAME: &str = "public-finding-v1";

/// Observer-facing provenance classes. They never create knowledge inside a world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimProvenance {
    WorldFact,
    ObservedEvidence,
    ContemporaryClaim,
    LaterInterpretation,
    ObserverInference,
    Disputed,
}

/// Deliberately restrained public event types. Raw biological and mortality mechanism
/// detail stays in canonical history, never in this projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicTimelineKind {
    WorldBegan,
    InitialPersonPresent,
    InitialAnimalPresent,
    PersonBorn,
    AnimalBorn,
    LifeEnded,
    PeopleExtinct,
    WorldArchived,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicTimelineItem {
    pub projection_version: u16,
    pub world_id: WorldId,
    pub source_event_id: EventId,
    pub source_sequence: EventSequence,
    pub source_tick: SimTick,
    pub source_event_index: u32,
    pub kind: PublicTimelineKind,
    pub provenance: ClaimProvenance,
    pub title: String,
    pub summary: String,
}

/// Produces a factual, restrained public finding aid from one committed batch.
///
/// The function never consumes wall-clock time, user input, labels, or model output.
/// It has no path back into the simulation and intentionally discards sex category,
/// death cause, parentage, location, and internal scientific identities.
#[must_use]
pub fn project_public_timeline(batch: &EventBatch) -> Vec<PublicTimelineItem> {
    batch
        .events
        .iter()
        .filter_map(|record| {
            let (kind, title, summary) = match &record.event {
                DomainEvent::WorldStarted { .. } => (
                    PublicTimelineKind::WorldBegan,
                    "A world began",
                    "Initial conditions were committed to the public record.",
                ),
                DomainEvent::OrganismInitialized { role, .. } => match role {
                    OrganismRole::Person => (
                        PublicTimelineKind::InitialPersonPresent,
                        "An initial person was present",
                        "The initial population was recorded.",
                    ),
                    OrganismRole::Fauna => (
                        PublicTimelineKind::InitialAnimalPresent,
                        "An initial animal was present",
                        "The initial ecology was recorded.",
                    ),
                },
                DomainEvent::OrganismBorn { role, .. } => match role {
                    OrganismRole::Person => (
                        PublicTimelineKind::PersonBorn,
                        "A person was born",
                        "A new life entered the recorded population.",
                    ),
                    OrganismRole::Fauna => (
                        PublicTimelineKind::AnimalBorn,
                        "An animal was born",
                        "A new animal entered the recorded population.",
                    ),
                },
                DomainEvent::OrganismDied { .. } => (
                    PublicTimelineKind::LifeEnded,
                    "A life ended",
                    "A death was recorded without public mechanism detail.",
                ),
                DomainEvent::WorldExtinct => (
                    PublicTimelineKind::PeopleExtinct,
                    "No people remained",
                    "The world reached its mechanical extinction condition.",
                ),
                DomainEvent::WorldArchived => (
                    PublicTimelineKind::WorldArchived,
                    "The world entered its archive",
                    "Its committed history remains available for observation.",
                ),
                DomainEvent::WorldConfigured { .. }
                | DomainEvent::MaterialInstanceInitialized { .. }
                | DomainEvent::MaterialInstanceHeld { .. }
                | DomainEvent::MaterialInstanceReleased { .. }
                | DomainEvent::TickAdvanced { .. }
                | DomainEvent::OrganismPerceived { .. }
                | DomainEvent::OrganismActed { .. }
                | DomainEvent::OrganismMoved { .. }
                | DomainEvent::OrganismAgeAdvanced { .. }
                | DomainEvent::CelestialStateRecorded { .. } => {
                    return None;
                }
            };
            Some(PublicTimelineItem {
                projection_version: PUBLIC_TIMELINE_PROJECTION_VERSION,
                world_id: batch.world_id,
                source_event_id: record.event_id,
                source_sequence: batch.sequence,
                source_tick: batch.tick,
                source_event_index: record.index,
                kind,
                provenance: ClaimProvenance::WorldFact,
                title: title.to_owned(),
                summary: summary.to_owned(),
            })
        })
        .collect()
}

#[async_trait]
pub trait ObserverTimelineStore: Send + Sync {
    /// Atomically projects one committed batch and advances the durable cursor.
    /// Returns false for a batch that was already projected.
    async fn apply_public_timeline_batch(
        &self,
        batch: &EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError>;

    async fn public_timeline_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError>;

    async fn list_public_timeline(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicTimelineItem>, ObserverProjectionStoreError>;
}

#[derive(Debug, Error)]
pub enum ObserverProjectionStoreError {
    #[error("observer projection storage is unavailable: {0}")]
    Unavailable(String),
    #[error("observer projection data is corrupt: {0}")]
    Corrupt(String),
}

/// Public routing metadata for a world. This is a read-only view of the durable
/// lifecycle cursor, deliberately separate from the simulation write port.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PublicWorldInputStatus {
    #[serde(rename = "provisional-not-scientifically-admitted")]
    ProvisionalNotScientificallyAdmitted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicWorld {
    pub world_id: WorldId,
    pub status: WorldStatus,
    pub through_sequence: EventSequence,
    pub tick: SimTick,
    /// Hash of the immutable world manifest; it contains no observer information.
    pub manifest_hash: Digest,
    /// Hash head of all committed canonical event batches through `through_sequence`.
    pub event_hash: Digest,
    /// Replayable canonical state hash at the public cursor.
    pub state_hash: Digest,
    pub predecessor_world_id: Option<WorldId>,
    /// Observer-side scientific admission status for the world's exact input set.
    /// Absent means the legacy store has not projected composition metadata yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_status: Option<PublicWorldInputStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_hash: Option<Digest>,
}

#[async_trait]
pub trait ObserverWorldStore: Send + Sync {
    async fn list_public_worlds(&self) -> Result<Vec<PublicWorld>, ObserverProjectionStoreError>;
}

/// A deterministic observer finding. It points to evidence rather than narrating a
/// world. `Streak` is reserved until canonical events can establish persistence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicFindingKind {
    First,
    Record,
    Streak,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicFinding {
    pub projection_version: u16,
    pub world_id: WorldId,
    pub source_event_id: EventId,
    pub source_sequence: EventSequence,
    pub source_tick: SimTick,
    pub kind: PublicFindingKind,
    pub finding_key: String,
    pub provenance: ClaimProvenance,
    pub title: String,
    pub summary: String,
}

#[async_trait]
pub trait ObserverFindingStore: Send + Sync {
    /// Atomically derives findings from one checksum-verified committed batch.
    async fn apply_public_finding_batch(
        &self,
        batch: &EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError>;

    async fn public_finding_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError>;

    async fn list_public_findings(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicFinding>, ObserverProjectionStoreError>;
}

/// One restrained observer-facing life record. This is an index over committed facts,
/// not a name or an identity visible inside the world.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicOrganism {
    pub projection_version: u16,
    pub world_id: WorldId,
    pub organism_id: EntityId,
    pub role: OrganismRole,
    pub species: SpeciesIdentity,
    pub provenance: ClaimProvenance,
    pub introduced_event_id: EventId,
    pub introduced_sequence: EventSequence,
    pub introduced_tick: SimTick,
    pub ended_event_id: Option<EventId>,
    pub ended_sequence: Option<EventSequence>,
    pub ended_tick: Option<SimTick>,
}

/// Builds public life-index facts from a committed batch. It deliberately excludes
/// birth category, parentage, location, cause of death, and any observer alias.
#[must_use]
pub fn project_public_organisms(batch: &EventBatch) -> Vec<PublicOrganism> {
    batch
        .events
        .iter()
        .filter_map(|record| match &record.event {
            DomainEvent::OrganismInitialized {
                organism_id,
                species,
                role,
                ..
            }
            | DomainEvent::OrganismBorn {
                organism_id,
                species,
                role,
                ..
            } => Some(PublicOrganism {
                projection_version: PUBLIC_ORGANISM_PROJECTION_VERSION,
                world_id: batch.world_id,
                organism_id: *organism_id,
                role: *role,
                species: species.clone(),
                provenance: ClaimProvenance::WorldFact,
                introduced_event_id: record.event_id,
                introduced_sequence: batch.sequence,
                introduced_tick: batch.tick,
                ended_event_id: None,
                ended_sequence: None,
                ended_tick: None,
            }),
            DomainEvent::WorldStarted { .. }
            | DomainEvent::WorldConfigured { .. }
            | DomainEvent::MaterialInstanceInitialized { .. }
            | DomainEvent::MaterialInstanceHeld { .. }
            | DomainEvent::MaterialInstanceReleased { .. }
            | DomainEvent::TickAdvanced { .. }
            | DomainEvent::OrganismPerceived { .. }
            | DomainEvent::OrganismActed { .. }
            | DomainEvent::OrganismMoved { .. }
            | DomainEvent::OrganismAgeAdvanced { .. }
            | DomainEvent::CelestialStateRecorded { .. }
            | DomainEvent::OrganismDied { .. }
            | DomainEvent::WorldExtinct
            | DomainEvent::WorldArchived => None,
        })
        .collect()
}

#[async_trait]
pub trait ObserverOrganismStore: Send + Sync {
    /// Atomically projects one committed batch into public life records.
    /// Returns false when this projection already processed the batch.
    async fn apply_public_organism_batch(
        &self,
        batch: &EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError>;

    async fn public_organism_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError>;

    async fn list_public_organisms(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicOrganism>, ObserverProjectionStoreError>;

    async fn get_public_organism(
        &self,
        world_id: WorldId,
        organism_id: EntityId,
    ) -> Result<Option<PublicOrganism>, ObserverProjectionStoreError>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationState {
    PendingPayment,
    PendingModeration,
    Active,
    Matched,
    Rejected,
    CancelledBySupporter,
    Expired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ReservationTarget {
    Person,
    Animal { species: SpeciesIdentity },
}

impl ReservationTarget {
    #[must_use]
    pub const fn role(&self) -> OrganismRole {
        match self {
            Self::Person => OrganismRole::Person,
            Self::Animal { .. } => OrganismRole::Fauna,
        }
    }

    #[must_use]
    pub fn species(&self) -> Option<&SpeciesIdentity> {
        match self {
            Self::Person => None,
            Self::Animal { species } => Some(species),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReservationRequest {
    pub reservation_id: Uuid,
    pub world_id: WorldId,
    /// Opaque account-provider subject. It is never a world identity.
    pub supporter_subject: String,
    pub observer_label: String,
    pub target: ReservationTarget,
    pub birth_category: BirthCategory,
}

impl ReservationRequest {
    pub fn validate(&self) -> Result<(), ReservationError> {
        if self.supporter_subject.trim().is_empty() || self.supporter_subject.len() > 256 {
            return Err(ReservationError::InvalidSupporterSubject);
        }
        let label = self.observer_label.trim();
        if label.is_empty()
            || label.len() > 80
            || self.observer_label != label
            || self.observer_label.chars().any(char::is_control)
        {
            return Err(ReservationError::InvalidObserverLabel);
        }
        if let Some(species) = self.target.species() {
            species
                .validate()
                .map_err(|_| ReservationError::InvalidAnimalSpecies)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SupporterReservation {
    pub request: ReservationRequest,
    pub state: ReservationState,
    pub payment_reference: Option<String>,
    pub created_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub matched_birth: Option<MatchedBirth>,
}

/// Immutable link to a birth that had already entered canonical history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MatchedBirth {
    pub world_id: WorldId,
    pub event_id: EventId,
    pub event_sequence: EventSequence,
    pub tick: SimTick,
    pub organism_id: EntityId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommittedBirth {
    pub world_id: WorldId,
    pub event_id: EventId,
    pub event_sequence: EventSequence,
    pub tick: SimTick,
    pub organism_id: EntityId,
    pub role: OrganismRole,
    pub species: SpeciesIdentity,
    pub birth_category: BirthCategory,
}

impl SupporterReservation {
    #[must_use]
    pub fn matches_birth(&self, birth: &CommittedBirth) -> bool {
        self.state == ReservationState::Active
            && self.request.world_id == birth.world_id
            && self.request.birth_category == birth.birth_category
            && self.request.target.role() == birth.role
            && self
                .request
                .target
                .species()
                .is_none_or(|species| species == &birth.species)
    }
}

/// Isolated persistence port for public supporter labels.
///
/// The observer projection invokes `match_committed_birth` only after a canonical
/// birth event is committed. The runner never imports this trait. Implementations must
/// never write canonical events or world state.
#[async_trait]
pub trait SupporterReservationStore: Send + Sync {
    async fn create_reservation(
        &self,
        request: &ReservationRequest,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    /// Called by a verified idempotent payment integration, never a browser redirect.
    /// The reservation remains in moderation until an independent reviewer approves it.
    async fn record_verified_payment(
        &self,
        reservation_id: Uuid,
        payment_reference: &str,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    /// A human moderator approves a paid label after abuse, privacy, impersonation, and
    /// advertising review. Approval is still observer-only.
    async fn approve_reservation(
        &self,
        reservation_id: Uuid,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    async fn reject_reservation(
        &self,
        reservation_id: Uuid,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    /// Observer-side matching for a birth that has already committed in canonical history.
    async fn match_committed_birth(
        &self,
        birth: &CommittedBirth,
    ) -> Result<Option<SupporterReservation>, ReservationStoreError>;

    /// Marks every unmatched reservation as unavailable after immutable world archival.
    /// Matched historical aliases remain intact.
    async fn expire_world_reservations(
        &self,
        world_id: WorldId,
    ) -> Result<u64, ReservationStoreError>;
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReservationError {
    #[error("supporter subject must be non-empty and at most 256 bytes")]
    InvalidSupporterSubject,
    #[error("observer label must be trimmed, non-empty, control-free, and at most 80 bytes")]
    InvalidObserverLabel,
    #[error("animal reservation requires a valid cited species identity")]
    InvalidAnimalSpecies,
}

#[derive(Debug, Error)]
pub enum ReservationStoreError {
    #[error(transparent)]
    Validation(#[from] ReservationError),
    #[error("supporter reservation was not found: {0}")]
    NotFound(Uuid),
    #[error("supporter reservation conflicted: {0}")]
    Conflict(String),
    #[error("supporter reservation storage is unavailable: {0}")]
    Unavailable(String),
    #[error("supporter reservation data is corrupt: {0}")]
    Corrupt(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use world_domain::{Digest, EVENT_SCHEMA_VERSION, WorldManifest, WorldSeed};

    fn species() -> SpeciesIdentity {
        SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("valid species")
    }

    fn birth() -> CommittedBirth {
        CommittedBirth {
            world_id: WorldId::from_uuid(Uuid::from_u128(1)),
            event_id: EventId::from_uuid(Uuid::from_u128(2)),
            event_sequence: EventSequence::new(9),
            tick: SimTick::new(7),
            organism_id: EntityId::from_uuid(Uuid::from_u128(3)),
            role: OrganismRole::Person,
            species: species(),
            birth_category: BirthCategory::new("female").expect("valid category"),
        }
    }

    #[test]
    fn reservation_is_only_eligible_for_an_already_matching_birth() {
        let request = ReservationRequest {
            reservation_id: Uuid::from_u128(4),
            world_id: WorldId::from_uuid(Uuid::from_u128(1)),
            supporter_subject: "account_123".to_owned(),
            observer_label: "Ada".to_owned(),
            target: ReservationTarget::Person,
            birth_category: BirthCategory::new("female").expect("valid category"),
        };
        request.validate().expect("valid request");
        let reservation = SupporterReservation {
            request,
            state: ReservationState::Active,
            payment_reference: Some("stripe-event-1".to_owned()),
            created_at: Utc::now(),
            activated_at: Some(Utc::now()),
            matched_birth: None,
        };
        assert!(reservation.matches_birth(&birth()));

        let mut mismatched = birth();
        mismatched.birth_category = BirthCategory::new("male").expect("valid category");
        assert!(!reservation.matches_birth(&mismatched));
    }

    #[test]
    fn labels_cannot_sneak_control_text_or_empty_subjects_into_public_projections() {
        let request = ReservationRequest {
            reservation_id: Uuid::from_u128(4),
            world_id: WorldId::from_uuid(Uuid::from_u128(1)),
            supporter_subject: " ".to_owned(),
            observer_label: "Ada\n".to_owned(),
            target: ReservationTarget::Person,
            birth_category: BirthCategory::new("female").expect("valid category"),
        };
        assert_eq!(
            request.validate(),
            Err(ReservationError::InvalidSupporterSubject)
        );
    }

    #[test]
    fn public_timeline_is_deterministic_and_withholds_sensitive_mechanism_detail() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(11));
        let manifest = WorldManifest::new(world_id, WorldSeed::new(42), 1);
        let events = vec![
            DomainEvent::WorldStarted { manifest },
            DomainEvent::OrganismBorn {
                organism_id: EntityId::from_uuid(Uuid::from_u128(12)),
                species: species(),
                role: OrganismRole::Person,
                birth_category: BirthCategory::new("female").expect("valid category"),
                parent_ids: vec![EntityId::from_uuid(Uuid::from_u128(13))],
                location_id: Some(EntityId::from_uuid(Uuid::from_u128(14))),
                embodied_patch: None,
                metabolic_rate: None,
            },
            DomainEvent::OrganismDied {
                organism_id: EntityId::from_uuid(Uuid::from_u128(12)),
                cause: world_domain::DeathCause {
                    mechanism: "falling_rock".to_owned(),
                },
            },
            DomainEvent::TickAdvanced {
                from: SimTick::new(6),
                to: SimTick::new(7),
            },
        ];
        let batch = EventBatch::new(
            EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(8),
            SimTick::new(7),
            1,
            Digest::ZERO,
            events,
            Digest::sha256(b"projection state"),
        )
        .expect("valid event batch");
        let first = project_public_timeline(&batch);
        assert_eq!(first, project_public_timeline(&batch));
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|item| {
            item.provenance == ClaimProvenance::WorldFact
                && item.source_sequence == batch.sequence
                && item.source_tick == batch.tick
        }));
        let rendered = first
            .iter()
            .map(|item| format!("{} {}", item.title, item.summary))
            .collect::<String>()
            .to_ascii_lowercase();
        for withheld in ["female", "falling_rock", "parent", "location"] {
            assert!(!rendered.contains(withheld), "must withhold {withheld}");
        }
    }

    #[test]
    fn public_organism_index_retains_citations_but_not_sensitive_life_detail() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(21));
        let batch = EventBatch::new(
            EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(3),
            SimTick::new(2),
            1,
            Digest::ZERO,
            vec![DomainEvent::OrganismBorn {
                organism_id: EntityId::from_uuid(Uuid::from_u128(22)),
                species: species(),
                role: OrganismRole::Person,
                birth_category: BirthCategory::new("female").expect("valid category"),
                parent_ids: vec![EntityId::from_uuid(Uuid::from_u128(23))],
                location_id: Some(EntityId::from_uuid(Uuid::from_u128(24))),
                embodied_patch: None,
                metabolic_rate: None,
            }],
            Digest::sha256(b"organism projection state"),
        )
        .expect("valid event batch");
        let organisms = project_public_organisms(&batch);
        assert_eq!(organisms, project_public_organisms(&batch));
        assert_eq!(organisms.len(), 1);
        assert_eq!(organisms[0].species.scientific_name, "Homo sapiens");
        let rendered = serde_json::to_string(&organisms).expect("serializable index");
        for withheld in ["female", "parent", "location", "birth_category"] {
            assert!(!rendered.contains(withheld), "must withhold {withheld}");
        }
    }

    #[test]
    fn public_world_serializes_explicit_provisional_input_metadata_compatibly() {
        let mut world = PublicWorld {
            world_id: WorldId::from_uuid(Uuid::from_u128(31)),
            status: WorldStatus::Running,
            through_sequence: EventSequence::new(8),
            tick: SimTick::new(7),
            manifest_hash: Digest::sha256(b"manifest"),
            event_hash: Digest::sha256(b"events"),
            state_hash: Digest::sha256(b"state"),
            predecessor_world_id: None,
            input_status: Some(PublicWorldInputStatus::ProvisionalNotScientificallyAdmitted),
            composition_id: Some("full-earth-provisional-v1".to_owned()),
            composition_version: Some("0.1.0".to_owned()),
            composition_hash: Some(Digest::sha256(b"composition")),
        };
        let value = serde_json::to_value(&world).expect("serialize public world");
        assert_eq!(
            value["input_status"],
            "provisional-not-scientifically-admitted"
        );
        assert_eq!(value["composition_id"], "full-earth-provisional-v1");
        assert_eq!(value["composition_version"], "0.1.0");
        assert_eq!(
            value["composition_hash"],
            Digest::sha256(b"composition").to_string()
        );
        assert_eq!(
            serde_json::from_value::<PublicWorld>(value).expect("deserialize public world"),
            world
        );

        world.input_status = None;
        world.composition_id = None;
        world.composition_version = None;
        world.composition_hash = None;
        let legacy = serde_json::to_value(&world).expect("serialize legacy public world");
        for field in [
            "input_status",
            "composition_id",
            "composition_version",
            "composition_hash",
        ] {
            assert!(legacy.get(field).is_none(), "must omit absent {field}");
        }
        let decoded_legacy =
            serde_json::from_value::<PublicWorld>(legacy).expect("deserialize legacy public world");
        assert_eq!(decoded_legacy.input_status, None);
        assert_eq!(decoded_legacy.composition_id, None);
        assert_eq!(decoded_legacy.composition_version, None);
        assert_eq!(decoded_legacy.composition_hash, None);
    }
}
