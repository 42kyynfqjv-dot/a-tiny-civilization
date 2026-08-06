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
    BirthCategory, EntityId, EventId, EventSequence, OrganismRole, SimTick, SpeciesIdentity,
    WorldId,
};

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

    /// Marks still-active reservations as unavailable after immutable world archival.
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
}
