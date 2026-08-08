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
    BirthCategory, Digest, DomainEvent, EntityId, EventBatch, EventId, EventSequence,
    MaterialIdentity, OrganismRole, SimTick, SpeciesIdentity, WorldId, WorldStatus,
};

pub const OBSERVER_LABEL_POLICY_VERSION: u16 = 1;
const BLOCKED_LABEL_TOKENS_V1: &[&str] = &[
    "bitch", "cunt", "faggot", "fuck", "fucker", "fucking", "hitler", "nazi", "nigga", "nigger",
    "porn", "retard", "shit", "slut", "whore",
];
const RESERVED_LABEL_TOKENS_V1: &[&str] = &[
    "admin",
    "administrator",
    "atinycivilization",
    "moderator",
    "official",
    "system",
];

pub const PUBLIC_TIMELINE_PROJECTION_VERSION: u16 = 1;
pub const PUBLIC_TIMELINE_PROJECTION_NAME: &str = "public-timeline-v1";
pub const PUBLIC_ORGANISM_PROJECTION_VERSION: u16 = 1;
pub const PUBLIC_ORGANISM_PROJECTION_NAME: &str = "public-organism-v1";
pub const PUBLIC_FINDING_PROJECTION_VERSION: u16 = 2;
pub const PUBLIC_FINDING_PROJECTION_NAME: &str = "public-finding-v2";
pub const PUBLIC_ARTIFACT_PROJECTION_VERSION: u16 = 1;
pub const PUBLIC_ARTIFACT_PROJECTION_NAME: &str = "public-artifact-v1";
pub const PUBLIC_WIKI_INDEX_VERSION: u16 = 1;

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
/// The function never consumes wall-clock time, user input, or observer labels. It
/// has no path back into the simulation and intentionally discards sex category,
/// death cause, parentage, location, internal scientific identities, and cognition
/// provider mechanics.
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
                | DomainEvent::OrganismAdultBodyMassCommitted { .. }
                | DomainEvent::MaterialInstanceInitialized { .. }
                | DomainEvent::MaterialReservoirCommitted { .. }
                | DomainEvent::MaterialInstanceHeld { .. }
                | DomainEvent::MaterialInstanceReleased { .. }
                | DomainEvent::MaterialSurfaceTraceChanged { .. }
                | DomainEvent::MaterialSurfaceRegionTraceChanged { .. }
                | DomainEvent::MaterialOralPortionTransferred { .. }
                | DomainEvent::MaterialReservoirOralPortionTransferred { .. }
                | DomainEvent::TickAdvanced { .. }
                | DomainEvent::OrganismPerceived { .. }
                | DomainEvent::OrganismActed { .. }
                | DomainEvent::OrganismMoved { .. }
                | DomainEvent::OrganismAgeAdvanced { .. }
                | DomainEvent::OrganismNeedsChanged { .. }
                | DomainEvent::OrganismActionValueChanged { .. }
                | DomainEvent::OrganismMovementDirectionValueChanged { .. }
                | DomainEvent::OrganismSocialActionValueChanged { .. }
                | DomainEvent::OrganismSignalActionAssociationChanged { .. }
                | DomainEvent::CognitionRequestSelected { .. }
                | DomainEvent::CognitionInputRecorded { .. }
                | DomainEvent::ReproductiveDevelopmentStarted { .. }
                | DomainEvent::ReproductiveDevelopmentEnded { .. }
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
    #[error("observer projection resource was not found: {0}")]
    NotFound(String),
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

/// Wall-clock observer telemetry derived from durable storage and disposable read
/// models. These measurements are never simulation inputs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicWorldTelemetry {
    pub world_id: WorldId,
    pub through_sequence: EventSequence,
    pub tick: SimTick,
    pub committed_batches: u64,
    pub committed_events: u64,
    pub canonical_payload_bytes: u64,
    pub last_committed_at: chrono::DateTime<chrono::Utc>,
    pub timeline_through_sequence: EventSequence,
    pub organism_index_through_sequence: EventSequence,
    pub findings_through_sequence: EventSequence,
    pub telemetry_through_sequence: EventSequence,
    pub artifacts_through_sequence: EventSequence,
    pub timeline_lag_batches: u64,
    pub organism_index_lag_batches: u64,
    pub findings_lag_batches: u64,
    pub telemetry_lag_batches: u64,
    pub artifacts_lag_batches: u64,
    pub living_people: u64,
    pub living_fauna: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicHistoryCommitment {
    pub sequence: EventSequence,
    pub tick: SimTick,
    pub event_schema_version: u16,
    pub ruleset_version: u32,
    pub event_count: u32,
    pub previous_event_hash: Digest,
    pub batch_hash: Digest,
    pub post_state_hash: Digest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicHistoryCommitmentPage {
    pub world_id: WorldId,
    pub manifest: world_domain::WorldManifest,
    pub manifest_hash: Digest,
    pub head_sequence: EventSequence,
    pub head_event_hash: Digest,
    pub head_state_hash: Digest,
    pub after_sequence: EventSequence,
    pub commitments: Vec<PublicHistoryCommitment>,
    pub next_after_sequence: Option<EventSequence>,
}

#[async_trait]
pub trait ObserverWorldStore: Send + Sync {
    async fn list_public_worlds(&self) -> Result<Vec<PublicWorld>, ObserverProjectionStoreError>;
    async fn public_world_telemetry(
        &self,
        world_id: WorldId,
    ) -> Result<Option<PublicWorldTelemetry>, ObserverProjectionStoreError>;
}

/// Read-only, payload-free canonical commitments for public audit tooling.
#[async_trait]
pub trait ObserverHistoryCommitmentStore: Send + Sync {
    async fn public_history_commitments(
        &self,
        world_id: WorldId,
        after_sequence: EventSequence,
        limit: u16,
    ) -> Result<PublicHistoryCommitmentPage, ObserverProjectionStoreError>;
}

/// A deterministic observer finding. It points to evidence rather than narrating a
/// world. `Streak` is used only for persistence established directly by committed
/// events, never for inferred customs, meanings, or intentions.
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

/// An observer-side index of a real material object whose canonical history contains
/// at least one force-caused surface trace. "Artifact" is the observatory's filing
/// category, never a fact or concept available inside the world.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicArtifact {
    pub projection_version: u16,
    pub world_id: WorldId,
    pub object_id: EntityId,
    pub material: MaterialIdentity,
    /// The force-caused surface change is committed physical evidence.
    pub trace_provenance: ClaimProvenance,
    /// Filing the altered object as an "artifact" is an observer interpretation.
    pub classification_provenance: ClaimProvenance,
    pub first_trace_event_id: EventId,
    pub first_trace_sequence: EventSequence,
    pub first_trace_tick: SimTick,
    pub latest_trace_event_id: EventId,
    pub latest_trace_sequence: EventSequence,
    pub latest_trace_tick: SimTick,
    pub surface_trace_units: u32,
}

/// One physical trace transition exposed without the private actor or any inferred
/// purpose. The delta is canonical world evidence; the artifact filing remains a
/// separate observer interpretation on `PublicArtifact`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicArtifactTrace {
    pub projection_version: u16,
    pub world_id: WorldId,
    pub object_id: EntityId,
    pub source_event_id: EventId,
    pub source_sequence: EventSequence,
    pub source_tick: SimTick,
    pub provenance: ClaimProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_region: Option<u8>,
    pub from_trace_units: u32,
    pub applied_force_units: u16,
    pub to_trace_units: u32,
}

#[async_trait]
pub trait ObserverArtifactStore: Send + Sync {
    async fn apply_public_artifact_batch(
        &self,
        batch: &EventBatch,
    ) -> Result<bool, ObserverProjectionStoreError>;

    async fn public_artifact_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError>;

    async fn list_public_artifacts(
        &self,
        world_id: WorldId,
        limit: u16,
    ) -> Result<Vec<PublicArtifact>, ObserverProjectionStoreError>;

    async fn list_public_artifact_traces(
        &self,
        world_id: WorldId,
        object_id: EntityId,
        after_sequence: EventSequence,
        limit: u16,
    ) -> Result<Vec<PublicArtifactTrace>, ObserverProjectionStoreError>;
}

/// The kind of evidence-backed page exposed by the generated observer wiki.
/// No variant asserts that the civilization has invented a corresponding concept.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicWikiEntryKind {
    Finding,
    AlteredMaterial,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicWikiEvidenceRole {
    SourceEvent,
    FirstPhysicalTrace,
    LatestPhysicalTrace,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicWikiEvidence {
    pub role: PublicWikiEvidenceRole,
    pub provenance: ClaimProvenance,
    pub event_id: EventId,
    pub sequence: EventSequence,
    pub tick: SimTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicWikiCitation {
    pub label: String,
    pub url: String,
}

/// A generated, read-only wiki entry. The entry's interpretation provenance is
/// separate from the provenance of each piece of evidence it cites.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicWikiEntry {
    pub index_version: u16,
    pub world_id: WorldId,
    pub entry_id: String,
    pub kind: PublicWikiEntryKind,
    pub title: String,
    pub summary: String,
    pub interpretation_provenance: ClaimProvenance,
    pub evidence: Vec<PublicWikiEvidence>,
    pub citations: Vec<PublicWikiCitation>,
}

/// Deterministically composes wiki pages from existing observer projections. It
/// performs no canonical reads, model calls, or free-form narration.
#[must_use]
pub fn compose_public_wiki_entries(
    findings: &[PublicFinding],
    artifacts: &[PublicArtifact],
) -> Vec<PublicWikiEntry> {
    let mut entries = Vec::with_capacity(findings.len() + artifacts.len());
    entries.extend(findings.iter().map(|finding| PublicWikiEntry {
        index_version: PUBLIC_WIKI_INDEX_VERSION,
        world_id: finding.world_id,
        entry_id: format!("finding:{}", finding.finding_key),
        kind: PublicWikiEntryKind::Finding,
        title: finding.title.clone(),
        summary: finding.summary.clone(),
        interpretation_provenance: finding.provenance,
        evidence: vec![PublicWikiEvidence {
            role: PublicWikiEvidenceRole::SourceEvent,
            provenance: ClaimProvenance::WorldFact,
            event_id: finding.source_event_id,
            sequence: finding.source_sequence,
            tick: finding.source_tick,
        }],
        citations: Vec::new(),
    }));
    entries.extend(artifacts.iter().map(|artifact| {
        let mut evidence = vec![PublicWikiEvidence {
            role: PublicWikiEvidenceRole::FirstPhysicalTrace,
            provenance: artifact.trace_provenance,
            event_id: artifact.first_trace_event_id,
            sequence: artifact.first_trace_sequence,
            tick: artifact.first_trace_tick,
        }];
        if artifact.latest_trace_event_id != artifact.first_trace_event_id {
            evidence.push(PublicWikiEvidence {
                role: PublicWikiEvidenceRole::LatestPhysicalTrace,
                provenance: artifact.trace_provenance,
                event_id: artifact.latest_trace_event_id,
                sequence: artifact.latest_trace_sequence,
                tick: artifact.latest_trace_tick,
            });
        }
        PublicWikiEntry {
            index_version: PUBLIC_WIKI_INDEX_VERSION,
            world_id: artifact.world_id,
            entry_id: format!("altered-material:{}", artifact.object_id),
            kind: PublicWikiEntryKind::AlteredMaterial,
            title: format!("Altered {} object", artifact.material.canonical_name),
            summary: format!(
                "The physical record contains a force-caused surface trace of {} units. No purpose, symbol, or meaning is inferred.",
                artifact.surface_trace_units
            ),
            interpretation_provenance: artifact.classification_provenance,
            evidence,
            citations: vec![PublicWikiCitation {
                label: artifact.material.canonical_name.clone(),
                url: artifact.material.source_url.clone(),
            }],
        }
    }));
    entries.sort_by(|left, right| {
        let left_sequence = left
            .evidence
            .iter()
            .map(|evidence| evidence.sequence)
            .max()
            .unwrap_or(EventSequence::ZERO);
        let right_sequence = right
            .evidence
            .iter()
            .map(|evidence| evidence.sequence)
            .max()
            .unwrap_or(EventSequence::ZERO);
        right_sequence
            .cmp(&left_sequence)
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    entries
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
            | DomainEvent::OrganismAdultBodyMassCommitted { .. }
            | DomainEvent::MaterialInstanceInitialized { .. }
            | DomainEvent::MaterialReservoirCommitted { .. }
            | DomainEvent::MaterialInstanceHeld { .. }
            | DomainEvent::MaterialInstanceReleased { .. }
            | DomainEvent::MaterialSurfaceTraceChanged { .. }
            | DomainEvent::MaterialSurfaceRegionTraceChanged { .. }
            | DomainEvent::MaterialOralPortionTransferred { .. }
            | DomainEvent::MaterialReservoirOralPortionTransferred { .. }
            | DomainEvent::TickAdvanced { .. }
            | DomainEvent::OrganismPerceived { .. }
            | DomainEvent::OrganismActed { .. }
            | DomainEvent::OrganismMoved { .. }
            | DomainEvent::OrganismAgeAdvanced { .. }
            | DomainEvent::OrganismNeedsChanged { .. }
            | DomainEvent::OrganismActionValueChanged { .. }
            | DomainEvent::OrganismMovementDirectionValueChanged { .. }
            | DomainEvent::OrganismSocialActionValueChanged { .. }
            | DomainEvent::OrganismSignalActionAssociationChanged { .. }
            | DomainEvent::CognitionRequestSelected { .. }
            | DomainEvent::CognitionInputRecorded { .. }
            | DomainEvent::ReproductiveDevelopmentStarted { .. }
            | DomainEvent::ReproductiveDevelopmentEnded { .. }
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
        if !observer_label_passes_automatic_policy(label) {
            return Err(ReservationError::DisallowedObserverLabel);
        }
        if let Some(species) = self.target.species() {
            species
                .validate()
                .map_err(|_| ReservationError::InvalidAnimalSpecies)?;
        }
        Ok(())
    }
}

/// A narrow deterministic pre-payment screen for obvious abuse and impersonation.
/// Passing it never constitutes approval: every paid label still enters human moderation.
#[must_use]
pub fn observer_label_passes_automatic_policy(label: &str) -> bool {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut compact = String::new();
    for character in label.chars() {
        let mapped = match character.to_ascii_lowercase() {
            '0' => Some('o'),
            '1' | '!' => Some('i'),
            '3' => Some('e'),
            '4' | '@' => Some('a'),
            '5' | '$' => Some('s'),
            '7' => Some('t'),
            value if value.is_ascii_alphabetic() => Some(value),
            _ => None,
        };
        if let Some(value) = mapped {
            token.push(value);
            compact.push(value);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    let denied = |value: &str| {
        BLOCKED_LABEL_TOKENS_V1.contains(&value) || RESERVED_LABEL_TOKENS_V1.contains(&value)
    };
    compact.is_empty() || (!denied(&compact) && !tokens.iter().any(|value| denied(value)))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SupporterReservation {
    pub request: ReservationRequest,
    pub state: ReservationState,
    pub payment_reference: Option<String>,
    #[serde(default)]
    pub payment_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub matched_birth: Option<MatchedBirth>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupporterRefundState {
    Pending,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AccountSupporterReservation {
    pub reservation: SupporterReservation,
    pub refund_state: Option<SupporterRefundState>,
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
        moderator_subject: &str,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    async fn reject_reservation(
        &self,
        reservation_id: Uuid,
        moderator_subject: &str,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    /// Returns paid labels awaiting human review in stable oldest-first order.
    async fn list_pending_moderation(
        &self,
        limit: u32,
    ) -> Result<Vec<SupporterReservation>, ReservationStoreError>;

    /// Cancels only an unmatched reservation owned by the authenticated observer subject.
    async fn cancel_reservation(
        &self,
        reservation_id: Uuid,
        supporter_subject: &str,
    ) -> Result<SupporterReservation, ReservationStoreError>;

    /// Lists only reservations owned by one authenticated observer subject.
    async fn list_account_reservations(
        &self,
        supporter_subject: &str,
        limit: u16,
    ) -> Result<Vec<AccountSupporterReservation>, ReservationStoreError>;

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
    #[error("observer label is rejected by the automatic safety policy")]
    DisallowedObserverLabel,
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
    use world_domain::{
        ACTION_LEARNING_EVENT_SCHEMA_VERSION, ACTION_VALUE_STATE_SCHEMA_VERSION, ActionValueState,
        BODILY_REGULATION_EVENT_SCHEMA_VERSION, BodilyNeedState, BodilyRegulationState, Digest,
        EVENT_SCHEMA_VERSION, HERITABLE_ACTION_KINDS, HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION,
        HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION, HERITABLE_DISPOSITION_SCHEMA_VERSION,
        HeritableActionWeight, HeritableDisposition, HeritableDispositionProfile,
        MATERIAL_INGESTION_EVENT_SCHEMA_VERSION, PhysiologicalEvidenceBasis, PrimitiveActionKind,
        REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION, ReproductiveDevelopmentEnd, WorldManifest,
        WorldSeed,
    };

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

    fn private_heredity() -> (HeritableDispositionProfile, HeritableDisposition) {
        let profile = HeritableDispositionProfile {
            profile_schema_version: HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
            profile_id: "private-heredity-fixture".to_owned(),
            profile_digest: Digest::sha256(b"private heredity fixture"),
            species: species(),
            evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
            minimum_action_weight: 4,
            neutral_action_weight: 16,
            maximum_action_weight: 28,
            founder_variation_steps: 3,
            mutation_probability_millionths: 100_000,
            mutation_max_step: 2,
        };
        let disposition = HeritableDisposition {
            disposition_schema_version: HERITABLE_DISPOSITION_SCHEMA_VERSION,
            profile_digest: profile.profile_digest,
            generation: 1,
            derived_at: SimTick::new(5),
            action_weights: HERITABLE_ACTION_KINDS
                .into_iter()
                .map(|action_kind| HeritableActionWeight {
                    action_kind,
                    weight: profile.neutral_action_weight,
                })
                .collect(),
        };
        (profile, disposition)
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
            payment_verified_at: Some(Utc::now()),
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
    fn obvious_abuse_and_impersonation_are_rejected_before_payment() {
        for label in ["f.u.c.k", "sh1t", "ADMIN", "A Tiny Civilization"] {
            assert!(
                !observer_label_passes_automatic_policy(label),
                "{label} must be rejected"
            );
        }
        for label in ["Ada", "Cassandra", "Ashita", "Zoë", "李明"] {
            assert!(
                observer_label_passes_automatic_policy(label),
                "{label} must remain eligible for human review"
            );
        }
    }

    #[test]
    fn public_timeline_is_deterministic_and_withholds_sensitive_mechanism_detail() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(11));
        let manifest = WorldManifest::new(world_id, WorldSeed::new(42), 15);
        let (private_profile, private_disposition) = private_heredity();
        let events = vec![
            DomainEvent::WorldStarted { manifest },
            DomainEvent::OrganismBorn {
                organism_id: EntityId::from_uuid(Uuid::from_u128(12)),
                development_id: None,
                species: species(),
                role: OrganismRole::Person,
                birth_category: BirthCategory::new("female").expect("valid category"),
                parent_ids: vec![EntityId::from_uuid(Uuid::from_u128(13))],
                location_id: Some(EntityId::from_uuid(Uuid::from_u128(14))),
                embodied_patch: None,
                metabolic_rate: None,
                physiological_regulation: None,
                reproductive_physiology: None,
                heritable_disposition_profile: Some(private_profile),
                heritable_disposition: Some(private_disposition),
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
            HERITABLE_DISPOSITION_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(8),
            SimTick::new(7),
            15,
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
        for withheld in [
            "female",
            "falling_rock",
            "parent",
            "location",
            "heritable",
            "generation",
            "weight",
            "mutation",
        ] {
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
                development_id: None,
                species: species(),
                role: OrganismRole::Person,
                birth_category: BirthCategory::new("female").expect("valid category"),
                parent_ids: vec![EntityId::from_uuid(Uuid::from_u128(23))],
                location_id: Some(EntityId::from_uuid(Uuid::from_u128(24))),
                embodied_patch: None,
                metabolic_rate: None,
                physiological_regulation: None,
                reproductive_physiology: None,
                heritable_disposition_profile: None,
                heritable_disposition: None,
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
    fn wiki_composition_separates_physical_evidence_from_observer_classification() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(31));
        let artifact = PublicArtifact {
            projection_version: PUBLIC_ARTIFACT_PROJECTION_VERSION,
            world_id,
            object_id: EntityId::from_uuid(Uuid::from_u128(32)),
            material: MaterialIdentity::new(
                "pubchem",
                "24261",
                "silicon dioxide",
                "https://pubchem.ncbi.nlm.nih.gov/compound/24261",
            )
            .expect("material identity"),
            trace_provenance: ClaimProvenance::WorldFact,
            classification_provenance: ClaimProvenance::ObserverInference,
            first_trace_event_id: EventId::from_uuid(Uuid::from_u128(33)),
            first_trace_sequence: EventSequence::new(7),
            first_trace_tick: SimTick::new(6),
            latest_trace_event_id: EventId::from_uuid(Uuid::from_u128(34)),
            latest_trace_sequence: EventSequence::new(9),
            latest_trace_tick: SimTick::new(8),
            surface_trace_units: 12,
        };
        let finding = PublicFinding {
            projection_version: PUBLIC_FINDING_PROJECTION_VERSION,
            world_id,
            source_event_id: EventId::from_uuid(Uuid::from_u128(35)),
            source_sequence: EventSequence::new(8),
            source_tick: SimTick::new(7),
            kind: PublicFindingKind::First,
            finding_key: "world_began".to_owned(),
            provenance: ClaimProvenance::WorldFact,
            title: "A world began".to_owned(),
            summary: "Initial conditions were committed.".to_owned(),
        };

        let entries = compose_public_wiki_entries(&[finding], &[artifact]);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, PublicWikiEntryKind::AlteredMaterial);
        assert_eq!(
            entries[0].interpretation_provenance,
            ClaimProvenance::ObserverInference
        );
        assert!(
            entries[0]
                .evidence
                .iter()
                .all(|evidence| evidence.provenance == ClaimProvenance::WorldFact)
        );
        assert_eq!(entries[0].evidence.len(), 2);
        assert_eq!(entries[1].kind, PublicWikiEntryKind::Finding);
        assert_eq!(
            compose_public_wiki_entries(&[], &[]),
            Vec::<PublicWikiEntry>::new()
        );
    }

    #[test]
    fn bodily_regulation_state_is_never_a_public_projection() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(31));
        let batch = EventBatch::new(
            BODILY_REGULATION_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(4),
            SimTick::new(3),
            10,
            Digest::ZERO,
            vec![DomainEvent::OrganismNeedsChanged {
                organism_id: EntityId::from_uuid(Uuid::from_u128(32)),
                from: BodilyRegulationState::default(),
                to: BodilyRegulationState {
                    energy_load_scaled_joules: 300,
                    needs: BodilyNeedState {
                        energy_deficit: 7,
                        ..BodilyNeedState::default()
                    },
                    ..BodilyRegulationState::default()
                },
            }],
            Digest::sha256(b"private bodily state"),
        )
        .expect("valid internal body event");
        assert!(project_public_timeline(&batch).is_empty());
        assert!(project_public_organisms(&batch).is_empty());
    }

    #[test]
    fn oral_material_transfer_is_private_mechanism_not_public_narrative() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(33));
        let batch = EventBatch::new(
            MATERIAL_INGESTION_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(5),
            SimTick::new(4),
            12,
            Digest::ZERO,
            vec![DomainEvent::MaterialOralPortionTransferred {
                object_id: EntityId::from_uuid(Uuid::from_u128(34)),
                organism_id: EntityId::from_uuid(Uuid::from_u128(35)),
                profile_digest: Digest::sha256(b"private oral profile"),
                from_mass_milligrams: 250_000,
                transferred_mass_milligrams: 250_000,
                to_mass_milligrams: 0,
            }],
            Digest::sha256(b"private oral transfer state"),
        )
        .expect("valid internal oral-transfer event");
        assert!(project_public_timeline(&batch).is_empty());
        assert!(project_public_organisms(&batch).is_empty());
    }

    #[test]
    fn material_surface_trace_is_only_exposed_by_the_dedicated_artifact_projection() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(341));
        let batch = EventBatch::new(
            world_domain::MATERIAL_SURFACE_TRACE_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(5),
            SimTick::new(4),
            19,
            Digest::ZERO,
            vec![DomainEvent::MaterialSurfaceTraceChanged {
                object_id: EntityId::from_uuid(Uuid::from_u128(342)),
                organism_id: EntityId::from_uuid(Uuid::from_u128(343)),
                from_trace_units: 0,
                applied_force_units: 7,
                to_trace_units: 7,
            }],
            Digest::sha256(b"private surface trace state"),
        )
        .expect("valid internal surface trace event");
        assert!(project_public_timeline(&batch).is_empty());
        assert!(project_public_organisms(&batch).is_empty());
    }

    #[test]
    fn learned_action_value_is_private_internal_state() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(36));
        let batch = EventBatch::new(
            ACTION_LEARNING_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(6),
            SimTick::new(5),
            13,
            Digest::ZERO,
            vec![DomainEvent::OrganismActionValueChanged {
                organism_id: EntityId::from_uuid(Uuid::from_u128(37)),
                from: None,
                to: ActionValueState {
                    value_schema_version: ACTION_VALUE_STATE_SCHEMA_VERSION,
                    action_kind: PrimitiveActionKind::Swallow,
                    observations: 1,
                    value: 12,
                },
            }],
            Digest::sha256(b"private learned action value"),
        )
        .expect("valid internal learning event");
        assert!(project_public_timeline(&batch).is_empty());
        assert!(project_public_organisms(&batch).is_empty());
    }

    #[test]
    fn reproductive_development_is_private_and_non_explicit() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(38));
        let development_id = EntityId::from_uuid(Uuid::from_u128(39));
        let developing_parent_id = EntityId::from_uuid(Uuid::from_u128(40));
        let other_parent_id = EntityId::from_uuid(Uuid::from_u128(41));
        let batch = EventBatch::new(
            REPRODUCTIVE_PHYSIOLOGY_EVENT_SCHEMA_VERSION,
            world_id,
            EventSequence::new(7),
            SimTick::new(5),
            14,
            Digest::ZERO,
            vec![
                DomainEvent::ReproductiveDevelopmentStarted {
                    development_id,
                    offspring_id: EntityId::from_uuid(Uuid::from_u128(42)),
                    species: species(),
                    role: OrganismRole::Person,
                    birth_category: BirthCategory::new("female").expect("category"),
                    parent_ids: vec![developing_parent_id, other_parent_id],
                    developing_parent_id,
                    profile_digest: Digest::sha256(b"private reproductive profile"),
                    due_tick: SimTick::new(6),
                    parents_available_at: SimTick::new(7),
                    heritable_disposition_profile: None,
                    offspring_heritable_disposition: None,
                },
                DomainEvent::ReproductiveDevelopmentEnded {
                    development_id,
                    developing_parent_id,
                    reason: ReproductiveDevelopmentEnd::DevelopingParentUnavailable,
                },
            ],
            Digest::sha256(b"private reproductive state"),
        )
        .expect("valid internal reproductive events");
        assert!(project_public_timeline(&batch).is_empty());
        assert!(project_public_organisms(&batch).is_empty());
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
