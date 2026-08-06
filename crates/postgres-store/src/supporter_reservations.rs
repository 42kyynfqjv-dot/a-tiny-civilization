use async_trait::async_trait;
use chrono::{DateTime, Utc};
use observer_projection::{
    CommittedBirth, MatchedBirth, ReservationRequest, ReservationState, ReservationStoreError,
    ReservationTarget, SupporterReservation, SupporterReservationStore,
};
use sqlx::FromRow;
use uuid::Uuid;
use world_domain::{
    BirthCategory, EntityId, EventId, EventSequence, OrganismRole, SimTick, SpeciesIdentity,
    WorldId,
};

use crate::PostgresStore;

#[derive(FromRow)]
struct ReservationRow {
    id: Uuid,
    world_id: Uuid,
    supporter_subject: String,
    observer_label: String,
    target_role: String,
    species_catalog: Option<String>,
    species_identifier: Option<String>,
    species_scientific_name: Option<String>,
    species_source_url: Option<String>,
    birth_category: String,
    state: String,
    payment_reference: Option<String>,
    created_at: DateTime<Utc>,
    activated_at: Option<DateTime<Utc>>,
    matched_birth_event_id: Option<Uuid>,
    matched_event_sequence: Option<i64>,
    matched_tick: Option<i64>,
    matched_organism_id: Option<Uuid>,
}

#[async_trait]
impl SupporterReservationStore for PostgresStore {
    async fn create_reservation(
        &self,
        request: &ReservationRequest,
    ) -> Result<SupporterReservation, ReservationStoreError> {
        request.validate()?;
        let (target_role, species) = target_parts(&request.target);
        let row = sqlx::query_as::<_, ReservationRow>(
            r#"
            INSERT INTO supporter_reservations (
                id, world_id, supporter_subject, observer_label, target_role,
                species_catalog, species_identifier, species_scientific_name, species_source_url,
                birth_category, state
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending_payment')
            RETURNING id, world_id, supporter_subject, observer_label, target_role,
                species_catalog, species_identifier, species_scientific_name, species_source_url,
                birth_category, state, payment_reference, created_at, activated_at,
                matched_birth_event_id, matched_event_sequence, matched_tick, matched_organism_id
            "#,
        )
        .bind(request.reservation_id)
        .bind(request.world_id.as_uuid())
        .bind(&request.supporter_subject)
        .bind(&request.observer_label)
        .bind(target_role)
        .bind(species.map(|value| value.catalog.as_str()))
        .bind(species.map(|value| value.identifier.as_str()))
        .bind(species.map(|value| value.scientific_name.as_str()))
        .bind(species.map(|value| value.source_url.as_str()))
        .bind(request.birth_category.as_str())
        .fetch_one(self.pool())
        .await
        .map_err(operation_error)?;
        parse_reservation(row)
    }

    async fn record_verified_payment(
        &self,
        reservation_id: Uuid,
        payment_reference: &str,
    ) -> Result<SupporterReservation, ReservationStoreError> {
        validate_payment_reference(payment_reference)?;
        let row = sqlx::query_as::<_, ReservationRow>(
            r#"
            UPDATE supporter_reservations
            SET state = 'pending_moderation', payment_reference = $2, payment_verified_at = NOW()
            WHERE id = $1
              AND state = 'pending_payment'
            RETURNING id, world_id, supporter_subject, observer_label, target_role,
                species_catalog, species_identifier, species_scientific_name, species_source_url,
                birth_category, state, payment_reference, created_at, activated_at,
                matched_birth_event_id, matched_event_sequence, matched_tick, matched_organism_id
            "#,
        )
        .bind(reservation_id)
        .bind(payment_reference)
        .fetch_optional(self.pool())
        .await
        .map_err(operation_error)?;
        match row {
            Some(row) => parse_reservation(row),
            None => load_or_idempotent_payment(self, reservation_id, payment_reference).await,
        }
    }

    async fn approve_reservation(
        &self,
        reservation_id: Uuid,
    ) -> Result<SupporterReservation, ReservationStoreError> {
        transition_reservation(self, reservation_id, "pending_moderation", "active", true).await
    }

    async fn reject_reservation(
        &self,
        reservation_id: Uuid,
    ) -> Result<SupporterReservation, ReservationStoreError> {
        transition_reservation(
            self,
            reservation_id,
            "pending_moderation",
            "rejected",
            false,
        )
        .await
    }

    async fn match_committed_birth(
        &self,
        birth: &CommittedBirth,
    ) -> Result<Option<SupporterReservation>, ReservationStoreError> {
        birth
            .species
            .validate()
            .map_err(|error| ReservationStoreError::Corrupt(error.to_string()))?;

        let mut transaction = self.pool().begin().await.map_err(operation_error)?;
        let row = sqlx::query_as::<_, ReservationRow>(
            r#"
            SELECT id, world_id, supporter_subject, observer_label, target_role,
                species_catalog, species_identifier, species_scientific_name, species_source_url,
                birth_category, state, payment_reference, created_at, activated_at,
                matched_birth_event_id, matched_event_sequence, matched_tick, matched_organism_id
            FROM supporter_reservations
            WHERE world_id = $1
              AND state = 'active'
              AND target_role = $2
              AND birth_category = $3
              AND (
                    target_role = 'person'
                    OR (species_catalog = $4 AND species_identifier = $5)
              )
            ORDER BY activated_at ASC, id ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            "#,
        )
        .bind(birth.world_id.as_uuid())
        .bind(role_code(birth.role))
        .bind(birth.birth_category.as_str())
        .bind(&birth.species.catalog)
        .bind(&birth.species.identifier)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(operation_error)?;

        let Some(row) = row else {
            transaction.commit().await.map_err(operation_error)?;
            return Ok(None);
        };
        let updated = sqlx::query_as::<_, ReservationRow>(
            r#"
            UPDATE supporter_reservations
            SET state = 'matched',
                matched_birth_event_id = $2,
                matched_event_sequence = $3,
                matched_tick = $4,
                matched_organism_id = $5
            WHERE id = $1
            RETURNING id, world_id, supporter_subject, observer_label, target_role,
                species_catalog, species_identifier, species_scientific_name, species_source_url,
                birth_category, state, payment_reference, created_at, activated_at,
                matched_birth_event_id, matched_event_sequence, matched_tick, matched_organism_id
            "#,
        )
        .bind(row.id)
        .bind(birth.event_id.as_uuid())
        .bind(to_i64(birth.event_sequence.get(), "event sequence")?)
        .bind(to_i64(birth.tick.get(), "tick")?)
        .bind(birth.organism_id.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(operation_error)?;
        transaction.commit().await.map_err(operation_error)?;
        parse_reservation(updated).map(Some)
    }

    async fn expire_world_reservations(
        &self,
        world_id: WorldId,
    ) -> Result<u64, ReservationStoreError> {
        let result = sqlx::query(
            "UPDATE supporter_reservations SET state = 'expired' WHERE world_id = $1 AND state IN ('pending_moderation', 'active')",
        )
        .bind(world_id.as_uuid())
        .execute(self.pool())
        .await
        .map_err(operation_error)?;
        Ok(result.rows_affected())
    }
}

async fn transition_reservation(
    store: &PostgresStore,
    reservation_id: Uuid,
    expected: &str,
    next: &str,
    set_activated_at: bool,
) -> Result<SupporterReservation, ReservationStoreError> {
    let row = sqlx::query_as::<_, ReservationRow>(
        r#"
        UPDATE supporter_reservations
        SET state = $3,
            activated_at = CASE WHEN $4 THEN NOW() ELSE activated_at END
        WHERE id = $1 AND state = $2
        RETURNING id, world_id, supporter_subject, observer_label, target_role,
            species_catalog, species_identifier, species_scientific_name, species_source_url,
            birth_category, state, payment_reference, created_at, activated_at,
            matched_birth_event_id, matched_event_sequence, matched_tick, matched_organism_id
        "#,
    )
    .bind(reservation_id)
    .bind(expected)
    .bind(next)
    .bind(set_activated_at)
    .fetch_optional(store.pool())
    .await
    .map_err(operation_error)?;
    match row {
        Some(row) => parse_reservation(row),
        None => Err(ReservationStoreError::Conflict(format!(
            "reservation {reservation_id} is not {expected}"
        ))),
    }
}

async fn load_or_idempotent_payment(
    store: &PostgresStore,
    reservation_id: Uuid,
    payment_reference: &str,
) -> Result<SupporterReservation, ReservationStoreError> {
    let row = load_reservation(store, reservation_id).await?;
    if row.payment_reference.as_deref() == Some(payment_reference)
        && matches!(
            row.state,
            ReservationState::PendingModeration
                | ReservationState::Active
                | ReservationState::Matched
        )
    {
        return Ok(row);
    }
    Err(ReservationStoreError::Conflict(format!(
        "reservation {reservation_id} cannot accept this payment reference"
    )))
}

async fn load_reservation(
    store: &PostgresStore,
    reservation_id: Uuid,
) -> Result<SupporterReservation, ReservationStoreError> {
    let row = sqlx::query_as::<_, ReservationRow>(
        r#"
        SELECT id, world_id, supporter_subject, observer_label, target_role,
            species_catalog, species_identifier, species_scientific_name, species_source_url,
            birth_category, state, payment_reference, created_at, activated_at,
            matched_birth_event_id, matched_event_sequence, matched_tick, matched_organism_id
        FROM supporter_reservations WHERE id = $1
        "#,
    )
    .bind(reservation_id)
    .fetch_optional(store.pool())
    .await
    .map_err(operation_error)?
    .ok_or(ReservationStoreError::NotFound(reservation_id))?;
    parse_reservation(row)
}

fn target_parts(target: &ReservationTarget) -> (&'static str, Option<&SpeciesIdentity>) {
    (role_code(target.role()), target.species())
}

const fn role_code(role: OrganismRole) -> &'static str {
    match role {
        OrganismRole::Person => "person",
        OrganismRole::Fauna => "fauna",
    }
}

fn parse_reservation(row: ReservationRow) -> Result<SupporterReservation, ReservationStoreError> {
    let target = match row.target_role.as_str() {
        "person" => ReservationTarget::Person,
        "fauna" => ReservationTarget::Animal {
            species: SpeciesIdentity::new(
                required_species_part(row.species_catalog, "catalog")?,
                required_species_part(row.species_identifier, "identifier")?,
                required_species_part(row.species_scientific_name, "scientific_name")?,
                required_species_part(row.species_source_url, "source_url")?,
            )
            .map_err(|error| ReservationStoreError::Corrupt(error.to_string()))?,
        },
        other => {
            return Err(ReservationStoreError::Corrupt(format!(
                "unknown target role {other:?}"
            )));
        }
    };
    let state = match row.state.as_str() {
        "pending_payment" => ReservationState::PendingPayment,
        "pending_moderation" => ReservationState::PendingModeration,
        "active" => ReservationState::Active,
        "matched" => ReservationState::Matched,
        "rejected" => ReservationState::Rejected,
        "cancelled_by_supporter" => ReservationState::CancelledBySupporter,
        "expired" => ReservationState::Expired,
        other => {
            return Err(ReservationStoreError::Corrupt(format!(
                "unknown reservation state {other:?}"
            )));
        }
    };
    let request = ReservationRequest {
        reservation_id: row.id,
        world_id: WorldId::from_uuid(row.world_id),
        supporter_subject: row.supporter_subject,
        observer_label: row.observer_label,
        target,
        birth_category: BirthCategory::new(row.birth_category)
            .map_err(|error| ReservationStoreError::Corrupt(error.to_string()))?,
    };
    request.validate()?;
    let matched_birth = match (
        row.matched_birth_event_id,
        row.matched_event_sequence,
        row.matched_tick,
        row.matched_organism_id,
    ) {
        (None, None, None, None) => None,
        (Some(event_id), Some(sequence), Some(tick), Some(organism_id)) => Some(MatchedBirth {
            world_id: request.world_id,
            event_id: EventId::from_uuid(event_id),
            event_sequence: EventSequence::new(to_u64(sequence, "matched event sequence")?),
            tick: SimTick::new(to_u64(tick, "matched tick")?),
            organism_id: EntityId::from_uuid(organism_id),
        }),
        _ => {
            return Err(ReservationStoreError::Corrupt(
                "partial matched birth fields".to_owned(),
            ));
        }
    };
    Ok(SupporterReservation {
        request,
        state,
        payment_reference: row.payment_reference,
        created_at: row.created_at,
        activated_at: row.activated_at,
        matched_birth,
    })
}

fn required_species_part(
    value: Option<String>,
    field: &str,
) -> Result<String, ReservationStoreError> {
    value.ok_or_else(|| {
        ReservationStoreError::Corrupt(format!("animal reservation is missing species {field}"))
    })
}

fn validate_payment_reference(value: &str) -> Result<(), ReservationStoreError> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ReservationStoreError::Conflict(
            "invalid payment reference".to_owned(),
        ));
    }
    Ok(())
}

fn to_i64(value: u64, field: &str) -> Result<i64, ReservationStoreError> {
    i64::try_from(value)
        .map_err(|_| ReservationStoreError::Corrupt(format!("{field} exceeds PostgreSQL range")))
}

fn to_u64(value: i64, field: &str) -> Result<u64, ReservationStoreError> {
    u64::try_from(value)
        .map_err(|_| ReservationStoreError::Corrupt(format!("stored {field} is negative")))
}

fn operation_error(error: sqlx::Error) -> ReservationStoreError {
    ReservationStoreError::Unavailable(error.to_string())
}
