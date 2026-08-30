use std::collections::{HashMap, VecDeque};

use async_trait::async_trait;
use observer_projection::{
    ObserverHabitatStore, ObserverProjectionStoreError, PUBLIC_HABITAT_PROJECTION_NAME,
    PUBLIC_HABITAT_PROJECTION_VERSION, PublicHabitatActivity, PublicHabitatCluster,
    PublicHabitatCommunication, PublicHabitatCommunicationKind, PublicHabitatDetail,
    PublicHabitatEntity, PublicHabitatQuery, PublicHabitatView,
};
use sqlx::FromRow;
use world_domain::{
    DomainEvent, EntityId, EventId, EventSequence, OrganismRole, PerceptionChannel,
    PrimitiveActionKind, S2CellId, SimTick, SpeciesIdentity, WorldId, decode_s2_face_ij,
    s2_face_ij_center_uv, s2_face_uv_to_ray, s2_ray_to_geographic_e7,
};

use crate::{
    PostgresStore, advance_projection_cursor, lock_projection_cursor, verify_committed_batch_range,
};

const RETAINED_ACTIVITY: i64 = 512;
const RETAINED_COMMUNICATION: i64 = 512;
const MAX_ENTITY_RESPONSE: u16 = 2_000;
const MAX_CLUSTER_RESPONSE: i64 = 1_024;

#[derive(Clone)]
struct Introduction {
    organism_id: EntityId,
    role: OrganismRole,
    species: SpeciesIdentity,
    patch: S2CellId,
    sequence: EventSequence,
    tick: SimTick,
}

#[derive(Clone, Copy)]
struct Movement {
    from_patch: S2CellId,
    to_patch: S2CellId,
    sequence: EventSequence,
    tick: SimTick,
}

#[derive(Clone, Copy)]
struct Action {
    kind: PrimitiveActionKind,
    signal_form: Option<u8>,
    sequence: EventSequence,
}

#[derive(Clone, Copy)]
struct Ending {
    sequence: EventSequence,
}

#[derive(Clone, Copy)]
struct Activity {
    event_id: EventId,
    sequence: EventSequence,
    tick: SimTick,
    event_index: u32,
    organism_id: EntityId,
    action: PrimitiveActionKind,
    signal_form: Option<u8>,
}

#[derive(Clone, Copy)]
struct Communication {
    event_id: EventId,
    sequence: EventSequence,
    tick: SimTick,
    event_index: u32,
    kind: PublicHabitatCommunicationKind,
    source_organism_id: EntityId,
    observer_organism_id: EntityId,
    preceding_signal: Option<u8>,
    signal_form: u8,
    associated_action: Option<PrimitiveActionKind>,
}

#[derive(FromRow)]
struct EntityRow {
    organism_id: uuid::Uuid,
    role: String,
    species_catalog: String,
    species_identifier: String,
    species_scientific_name: String,
    species_source_url: String,
    embodied_patch: String,
    latitude_e7: i32,
    longitude_e7: i32,
    previous_latitude_e7: i32,
    previous_longitude_e7: i32,
    last_movement_tick: i64,
    last_action: Option<String>,
    signal_form: Option<i32>,
    alive: bool,
}

#[derive(FromRow)]
struct ClusterRow {
    latitude_e7: i32,
    longitude_e7: i32,
    people: i64,
    animals: i64,
    total: i64,
    latitude_bucket: i64,
    longitude_bucket: i64,
}

#[derive(FromRow)]
struct ActivityRow {
    source_event_id: uuid::Uuid,
    source_sequence: i64,
    source_tick: i64,
    source_event_index: i32,
    organism_id: uuid::Uuid,
    action: String,
    signal_form: Option<i32>,
}

#[derive(FromRow)]
struct CommunicationRow {
    source_event_id: uuid::Uuid,
    source_sequence: i64,
    source_tick: i64,
    source_event_index: i32,
    kind: String,
    source_organism_id: uuid::Uuid,
    observer_organism_id: uuid::Uuid,
    preceding_signal: Option<i32>,
    signal_form: i32,
    associated_action: Option<String>,
}

impl PostgresStore {
    pub async fn apply_public_habitat_batches_inner(
        &self,
        batches: &[world_domain::EventBatch],
    ) -> Result<u64, ObserverProjectionStoreError> {
        let Some(first) = batches.first() else {
            return Ok(0);
        };
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let cursor = lock_projection_cursor(
            &mut transaction,
            PUBLIC_HABITAT_PROJECTION_NAME,
            first.world_id,
        )
        .await?;
        let start = batches.partition_point(|batch| {
            i64::try_from(batch.sequence.get()).is_ok_and(|sequence| sequence <= cursor)
        });
        let pending = &batches[start..];
        let Some(first_pending) = pending.first() else {
            transaction.commit().await.map_err(unavailable)?;
            return Ok(0);
        };
        if to_i64(first_pending.sequence.get(), "source sequence")? != cursor + 1 {
            return Err(corrupt(
                "public habitat batch range is not contiguous with its cursor",
            ));
        }
        verify_committed_batch_range(&mut transaction, pending).await?;

        let mut introductions = HashMap::<EntityId, Introduction>::new();
        let mut movements = HashMap::<EntityId, Movement>::new();
        let mut actions = HashMap::<EntityId, Action>::new();
        let mut endings = HashMap::<EntityId, Ending>::new();
        let mut activity = VecDeque::<Activity>::with_capacity(RETAINED_ACTIVITY as usize);
        let mut communication =
            VecDeque::<Communication>::with_capacity(RETAINED_COMMUNICATION as usize);

        for batch in pending {
            for record in &batch.events {
                match &record.event {
                    DomainEvent::OrganismInitialized {
                        organism_id,
                        role,
                        species,
                        embodied_patch: Some(patch),
                        ..
                    }
                    | DomainEvent::OrganismBorn {
                        organism_id,
                        role,
                        species,
                        embodied_patch: Some(patch),
                        ..
                    } => {
                        introductions.insert(
                            *organism_id,
                            Introduction {
                                organism_id: *organism_id,
                                role: *role,
                                species: species.clone(),
                                patch: *patch,
                                sequence: batch.sequence,
                                tick: batch.tick,
                            },
                        );
                    }
                    DomainEvent::OrganismMoved {
                        organism_id,
                        from_patch,
                        to_patch,
                    } => {
                        movements.insert(
                            *organism_id,
                            Movement {
                                from_patch: *from_patch,
                                to_patch: *to_patch,
                                sequence: batch.sequence,
                                tick: batch.tick,
                            },
                        );
                        push_activity(
                            &mut activity,
                            Activity {
                                event_id: record.event_id,
                                sequence: batch.sequence,
                                tick: batch.tick,
                                event_index: record.index,
                                organism_id: *organism_id,
                                action: PrimitiveActionKind::Move,
                                signal_form: None,
                            },
                        );
                    }
                    DomainEvent::OrganismActed {
                        organism_id,
                        action,
                    } => {
                        let signal_form = if action.kind == PrimitiveActionKind::EmitSignal {
                            u8::try_from(action.intensity).ok()
                        } else {
                            None
                        };
                        if public_habitat_action(action.kind) {
                            actions.insert(
                                *organism_id,
                                Action {
                                    kind: action.kind,
                                    signal_form,
                                    sequence: batch.sequence,
                                },
                            );
                        }
                        if action.kind != PrimitiveActionKind::Move
                            && public_habitat_action(action.kind)
                        {
                            push_activity(
                                &mut activity,
                                Activity {
                                    event_id: record.event_id,
                                    sequence: batch.sequence,
                                    tick: batch.tick,
                                    event_index: record.index,
                                    organism_id: *organism_id,
                                    action: action.kind,
                                    signal_form,
                                },
                            );
                        }
                    }
                    DomainEvent::OrganismPerceived {
                        organism_id,
                        perception,
                    } => {
                        let signal_form = perception.readings.iter().find_map(|reading| {
                            (reading.channel == PerceptionChannel::Sound
                                && reading.property_code == "signal_amplitude")
                                .then(|| u8::try_from(reading.quantized_value).ok())
                                .flatten()
                        });
                        if let (Some(source_organism_id), Some(signal_form)) =
                            (perception.subject_id, signal_form)
                            && (1..=world_domain::SIGNAL_FORM_VARIANT_COUNT).contains(&signal_form)
                        {
                            push_communication(
                                &mut communication,
                                Communication {
                                    event_id: record.event_id,
                                    sequence: batch.sequence,
                                    tick: batch.tick,
                                    event_index: record.index,
                                    kind: PublicHabitatCommunicationKind::HeardSignal,
                                    source_organism_id,
                                    observer_organism_id: *organism_id,
                                    preceding_signal: None,
                                    signal_form,
                                    associated_action: None,
                                },
                            );
                        }
                    }
                    DomainEvent::OrganismSignalActionAssociationChanged {
                        observer_id,
                        actor_id,
                        to,
                        ..
                    } if public_habitat_action(to.action_kind) => {
                        push_communication(
                            &mut communication,
                            Communication {
                                event_id: record.event_id,
                                sequence: batch.sequence,
                                tick: batch.tick,
                                event_index: record.index,
                                kind: PublicHabitatCommunicationKind::AssociatedAction,
                                source_organism_id: *actor_id,
                                observer_organism_id: *observer_id,
                                preceding_signal: to.preceding_signal,
                                signal_form: to.signal_intensity,
                                associated_action: Some(to.action_kind),
                            },
                        );
                    }
                    DomainEvent::OrganismDied { organism_id, .. } => {
                        endings.insert(
                            *organism_id,
                            Ending {
                                sequence: batch.sequence,
                            },
                        );
                    }
                    _ => {}
                }
            }
        }

        for introduction in introductions.values() {
            insert_introduction(&mut transaction, first.world_id, introduction).await?;
        }
        for (organism_id, movement) in movements {
            apply_movement(&mut transaction, first.world_id, organism_id, movement).await?;
        }
        for (organism_id, action) in actions {
            apply_action(&mut transaction, first.world_id, organism_id, action).await?;
        }
        for (organism_id, ending) in endings {
            apply_ending(&mut transaction, first.world_id, organism_id, ending).await?;
        }
        for item in activity {
            insert_activity(&mut transaction, first.world_id, item).await?;
        }
        for item in communication {
            insert_communication(&mut transaction, first.world_id, item).await?;
        }
        sqlx::query(
            r#"
            DELETE FROM observer_habitat_activity
            WHERE projection_version = $1 AND world_id = $2 AND source_event_id IN (
                SELECT source_event_id FROM observer_habitat_activity
                WHERE projection_version = $1 AND world_id = $2
                ORDER BY source_sequence DESC, source_event_index DESC
                OFFSET $3
            )
            "#,
        )
        .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
        .bind(first.world_id.as_uuid())
        .bind(RETAINED_ACTIVITY)
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;
        sqlx::query(
            r#"
            DELETE FROM observer_habitat_communication
            WHERE projection_version = $1 AND world_id = $2 AND source_event_id IN (
                SELECT source_event_id FROM observer_habitat_communication
                WHERE projection_version = $1 AND world_id = $2
                ORDER BY source_sequence DESC, source_event_index DESC
                OFFSET $3
            )
            "#,
        )
        .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
        .bind(first.world_id.as_uuid())
        .bind(RETAINED_COMMUNICATION)
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;

        let last_sequence = to_i64(pending[pending.len() - 1].sequence.get(), "source sequence")?;
        advance_projection_cursor(
            &mut transaction,
            PUBLIC_HABITAT_PROJECTION_NAME,
            first.world_id,
            last_sequence,
        )
        .await?;
        transaction.commit().await.map_err(unavailable)?;
        u64::try_from(pending.len()).map_err(|_| corrupt("habitat batch count"))
    }
}

#[async_trait]
impl ObserverHabitatStore for PostgresStore {
    async fn apply_public_habitat_batches(
        &self,
        batches: &[world_domain::EventBatch],
    ) -> Result<u64, ObserverProjectionStoreError> {
        self.apply_public_habitat_batches_inner(batches).await
    }

    async fn public_habitat_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2",
        )
        .bind(PUBLIC_HABITAT_PROJECTION_NAME)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        Ok(EventSequence::new(to_u64(cursor, "habitat cursor")?))
    }

    async fn public_habitat_view(
        &self,
        world_id: WorldId,
        query: PublicHabitatQuery,
    ) -> Result<PublicHabitatView, ObserverProjectionStoreError> {
        let through_sequence = self.public_habitat_cursor(world_id).await?;
        let entity_limit = query.entity_limit.clamp(1, MAX_ENTITY_RESPONSE);
        let (entities, clusters, truncated) = match query.detail {
            PublicHabitatDetail::Local => {
                let rows = sqlx::query_as::<_, EntityRow>(
                    r#"
                    SELECT organism_id,role,species_catalog,species_identifier,
                        species_scientific_name,species_source_url,embodied_patch,
                        latitude_e7,longitude_e7,previous_latitude_e7,previous_longitude_e7,
                        last_movement_tick,
                        CASE WHEN last_action='bite' THEN NULL ELSE last_action END AS last_action,
                        signal_form,alive
                    FROM observer_habitat_entities
                    WHERE projection_version=$1 AND world_id=$2 AND alive
                      AND longitude_e7 BETWEEN $3 AND $4 AND latitude_e7 BETWEEN $5 AND $6
                    ORDER BY CASE role WHEN 'person' THEN 0 ELSE 1 END, organism_id
                    LIMIT $7
                    "#,
                )
                .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
                .bind(world_id.as_uuid())
                .bind(query.west_e7)
                .bind(query.east_e7)
                .bind(query.south_e7)
                .bind(query.north_e7)
                .bind(i64::from(entity_limit) + 1)
                .fetch_all(self.pool())
                .await
                .map_err(unavailable)?;
                let truncated = rows.len() > usize::from(entity_limit);
                let entities = rows
                    .into_iter()
                    .take(usize::from(entity_limit))
                    .map(parse_entity)
                    .collect::<Result<Vec<_>, _>>()?;
                (entities, Vec::new(), truncated)
            }
            PublicHabitatDetail::Planet | PublicHabitatDetail::Region => {
                let bucket = i64::from(query.cell_e7.clamp(10_000, 900_000_000));
                let rows = sqlx::query_as::<_, ClusterRow>(
                    r#"
                    SELECT
                        CAST(AVG(latitude_e7)::BIGINT AS INTEGER) AS latitude_e7,
                        CAST(AVG(longitude_e7)::BIGINT AS INTEGER) AS longitude_e7,
                        COUNT(*) FILTER (WHERE role='person')::BIGINT AS people,
                        COUNT(*) FILTER (WHERE role='fauna')::BIGINT AS animals,
                        COUNT(*)::BIGINT AS total,
                        FLOOR(latitude_e7::NUMERIC / $7)::BIGINT AS latitude_bucket,
                        FLOOR(longitude_e7::NUMERIC / $7)::BIGINT AS longitude_bucket
                    FROM observer_habitat_entities
                    WHERE projection_version=$1 AND world_id=$2 AND alive
                      AND longitude_e7 BETWEEN $3 AND $4 AND latitude_e7 BETWEEN $5 AND $6
                    GROUP BY latitude_bucket,longitude_bucket
                    ORDER BY total DESC,latitude_bucket,longitude_bucket
                    LIMIT $8
                    "#,
                )
                .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
                .bind(world_id.as_uuid())
                .bind(query.west_e7)
                .bind(query.east_e7)
                .bind(query.south_e7)
                .bind(query.north_e7)
                .bind(bucket)
                .bind(MAX_CLUSTER_RESPONSE + 1)
                .fetch_all(self.pool())
                .await
                .map_err(unavailable)?;
                let truncated = rows.len() > MAX_CLUSTER_RESPONSE as usize;
                let clusters = rows
                    .into_iter()
                    .take(MAX_CLUSTER_RESPONSE as usize)
                    .map(parse_cluster)
                    .collect::<Result<Vec<_>, _>>()?;
                (Vec::new(), clusters, truncated)
            }
        };
        let activity = load_activity(self, world_id, query.activity_limit).await?;
        let communication = load_communication(self, world_id, query.activity_limit).await?;
        Ok(PublicHabitatView {
            projection_version: PUBLIC_HABITAT_PROJECTION_VERSION,
            world_id,
            through_sequence,
            detail: query.detail,
            entities,
            clusters,
            activity,
            communication,
            truncated,
            maximum_entities: MAX_ENTITY_RESPONSE,
        })
    }
}

fn push_activity(activity: &mut VecDeque<Activity>, item: Activity) {
    if activity.len() == RETAINED_ACTIVITY as usize {
        activity.pop_front();
    }
    activity.push_back(item);
}

fn push_communication(communication: &mut VecDeque<Communication>, item: Communication) {
    if communication.len() == RETAINED_COMMUNICATION as usize {
        communication.pop_front();
    }
    communication.push_back(item);
}

/// The habitat is a family-safe observer projection. Canonical events remain
/// complete and replayable, while violence-adjacent primitives are not exposed
/// as entertainment in the live ticker or an organism's public status.
const fn public_habitat_action(action: PrimitiveActionKind) -> bool {
    !matches!(action, PrimitiveActionKind::Bite)
}

async fn insert_introduction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world_id: WorldId,
    introduction: &Introduction,
) -> Result<(), ObserverProjectionStoreError> {
    let (latitude_e7, longitude_e7) = coordinate(introduction.patch)?;
    sqlx::query(
        r#"
        INSERT INTO observer_habitat_entities (
            projection_version,world_id,organism_id,role,species_catalog,species_identifier,
            species_scientific_name,species_source_url,embodied_patch,latitude_e7,longitude_e7,
            previous_latitude_e7,previous_longitude_e7,last_movement_sequence,last_movement_tick,
            alive,updated_sequence
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$10,$11,$12,$13,TRUE,$12)
        ON CONFLICT (projection_version,world_id,organism_id) DO NOTHING
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(introduction.organism_id.as_uuid())
    .bind(role_code(introduction.role))
    .bind(&introduction.species.catalog)
    .bind(&introduction.species.identifier)
    .bind(&introduction.species.scientific_name)
    .bind(&introduction.species.source_url)
    .bind(introduction.patch.to_string())
    .bind(latitude_e7)
    .bind(longitude_e7)
    .bind(to_i64(
        introduction.sequence.get(),
        "introduction sequence",
    )?)
    .bind(to_i64(introduction.tick.get(), "introduction tick")?)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    Ok(())
}

async fn apply_movement(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world_id: WorldId,
    organism_id: EntityId,
    movement: Movement,
) -> Result<(), ObserverProjectionStoreError> {
    let (from_latitude, from_longitude) = coordinate(movement.from_patch)?;
    let (to_latitude, to_longitude) = coordinate(movement.to_patch)?;
    let result = sqlx::query(
        r#"
        UPDATE observer_habitat_entities SET embodied_patch=$4,latitude_e7=$5,longitude_e7=$6,
            previous_latitude_e7=$7,previous_longitude_e7=$8,last_movement_sequence=$9,
            last_movement_tick=$10,updated_sequence=GREATEST(updated_sequence,$9)
        WHERE projection_version=$1 AND world_id=$2 AND organism_id=$3
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(organism_id.as_uuid())
    .bind(movement.to_patch.to_string())
    .bind(to_latitude)
    .bind(to_longitude)
    .bind(from_latitude)
    .bind(from_longitude)
    .bind(to_i64(movement.sequence.get(), "movement sequence")?)
    .bind(to_i64(movement.tick.get(), "movement tick")?)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if result.rows_affected() != 1 {
        return Err(corrupt("movement references an unprojected organism"));
    }
    Ok(())
}

async fn apply_action(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world_id: WorldId,
    organism_id: EntityId,
    action: Action,
) -> Result<(), ObserverProjectionStoreError> {
    let result = sqlx::query(
        r#"
        UPDATE observer_habitat_entities SET last_action=$4,signal_form=$5,
            updated_sequence=GREATEST(updated_sequence,$6)
        WHERE projection_version=$1 AND world_id=$2 AND organism_id=$3
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(organism_id.as_uuid())
    .bind(action_code(action.kind))
    .bind(action.signal_form.map(i32::from))
    .bind(to_i64(action.sequence.get(), "action sequence")?)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if result.rows_affected() != 1 {
        return Err(corrupt("action references an unprojected organism"));
    }
    Ok(())
}

async fn apply_ending(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world_id: WorldId,
    organism_id: EntityId,
    ending: Ending,
) -> Result<(), ObserverProjectionStoreError> {
    let result = sqlx::query(
        "UPDATE observer_habitat_entities SET alive=FALSE,updated_sequence=GREATEST(updated_sequence,$4) WHERE projection_version=$1 AND world_id=$2 AND organism_id=$3",
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(organism_id.as_uuid())
    .bind(to_i64(ending.sequence.get(), "ending sequence")?)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if result.rows_affected() != 1 {
        return Err(corrupt("ending references an unprojected organism"));
    }
    Ok(())
}

async fn insert_activity(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world_id: WorldId,
    activity: Activity,
) -> Result<(), ObserverProjectionStoreError> {
    sqlx::query(
        r#"
        INSERT INTO observer_habitat_activity (
            projection_version,world_id,source_event_id,source_sequence,source_tick,
            source_event_index,organism_id,action,signal_form
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
        ON CONFLICT (projection_version,world_id,source_event_id) DO NOTHING
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(activity.event_id.as_uuid())
    .bind(to_i64(activity.sequence.get(), "activity sequence")?)
    .bind(to_i64(activity.tick.get(), "activity tick")?)
    .bind(i32::try_from(activity.event_index).map_err(|_| corrupt("activity event index"))?)
    .bind(activity.organism_id.as_uuid())
    .bind(action_code(activity.action))
    .bind(activity.signal_form.map(i32::from))
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    Ok(())
}

async fn insert_communication(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    world_id: WorldId,
    communication: Communication,
) -> Result<(), ObserverProjectionStoreError> {
    sqlx::query(
        r#"
        INSERT INTO observer_habitat_communication (
            projection_version,world_id,source_event_id,source_sequence,source_tick,
            source_event_index,kind,source_organism_id,observer_organism_id,signal_form,
            preceding_signal,associated_action
        ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
        ON CONFLICT (projection_version,world_id,source_event_id) DO NOTHING
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(communication.event_id.as_uuid())
    .bind(to_i64(
        communication.sequence.get(),
        "communication sequence",
    )?)
    .bind(to_i64(communication.tick.get(), "communication tick")?)
    .bind(
        i32::try_from(communication.event_index)
            .map_err(|_| corrupt("communication event index"))?,
    )
    .bind(communication_kind_code(communication.kind))
    .bind(communication.source_organism_id.as_uuid())
    .bind(communication.observer_organism_id.as_uuid())
    .bind(i32::from(communication.signal_form))
    .bind(communication.preceding_signal.map(i32::from))
    .bind(communication.associated_action.map(action_code))
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    Ok(())
}

async fn load_activity(
    store: &PostgresStore,
    world_id: WorldId,
    limit: u16,
) -> Result<Vec<PublicHabitatActivity>, ObserverProjectionStoreError> {
    let rows = sqlx::query_as::<_, ActivityRow>(
        r#"
        SELECT source_event_id,source_sequence,source_tick,source_event_index,
            organism_id,action,signal_form
        FROM observer_habitat_activity
        WHERE projection_version=$1 AND world_id=$2 AND action <> 'bite'
        ORDER BY source_sequence DESC,source_event_index DESC
        LIMIT $3
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(i64::from(limit.clamp(1, 64)))
    .fetch_all(store.pool())
    .await
    .map_err(unavailable)?;
    rows.into_iter()
        .map(|row| {
            Ok(PublicHabitatActivity {
                source_event_id: EventId::from_uuid(row.source_event_id),
                source_sequence: EventSequence::new(to_u64(
                    row.source_sequence,
                    "activity sequence",
                )?),
                source_tick: SimTick::new(to_u64(row.source_tick, "activity tick")?),
                source_event_index: u32::try_from(row.source_event_index)
                    .map_err(|_| corrupt("activity event index"))?,
                organism_id: EntityId::from_uuid(row.organism_id),
                action: parse_action(&row.action)?,
                signal_form: row
                    .signal_form
                    .map(|value| u8::try_from(value).map_err(|_| corrupt("signal form")))
                    .transpose()?,
            })
        })
        .collect()
}

async fn load_communication(
    store: &PostgresStore,
    world_id: WorldId,
    limit: u16,
) -> Result<Vec<PublicHabitatCommunication>, ObserverProjectionStoreError> {
    let rows = sqlx::query_as::<_, CommunicationRow>(
        r#"
        SELECT communication.source_event_id,communication.source_sequence,
            communication.source_tick,communication.source_event_index,communication.kind,
            communication.source_organism_id,communication.observer_organism_id,
            communication.preceding_signal,communication.signal_form,
            communication.associated_action
        FROM observer_habitat_communication communication
        JOIN observer_habitat_entities source
          ON source.projection_version=communication.projection_version
         AND source.world_id=communication.world_id
         AND source.organism_id=communication.source_organism_id
        JOIN observer_habitat_entities observer
          ON observer.projection_version=communication.projection_version
         AND observer.world_id=communication.world_id
         AND observer.organism_id=communication.observer_organism_id
        WHERE communication.projection_version=$1 AND communication.world_id=$2
          AND source.role='person' AND observer.role='person'
        ORDER BY communication.source_sequence DESC,communication.source_event_index DESC
        LIMIT $3
        "#,
    )
    .bind(i32::from(PUBLIC_HABITAT_PROJECTION_VERSION))
    .bind(world_id.as_uuid())
    .bind(i64::from(limit.clamp(1, 64)))
    .fetch_all(store.pool())
    .await
    .map_err(unavailable)?;
    rows.into_iter()
        .map(|row| {
            Ok(PublicHabitatCommunication {
                source_event_id: EventId::from_uuid(row.source_event_id),
                source_sequence: EventSequence::new(to_u64(
                    row.source_sequence,
                    "communication sequence",
                )?),
                source_tick: SimTick::new(to_u64(row.source_tick, "communication tick")?),
                source_event_index: u32::try_from(row.source_event_index)
                    .map_err(|_| corrupt("communication event index"))?,
                kind: parse_communication_kind(&row.kind)?,
                source_organism_id: EntityId::from_uuid(row.source_organism_id),
                observer_organism_id: EntityId::from_uuid(row.observer_organism_id),
                signal_sequence: communication_signal_sequence(
                    row.preceding_signal,
                    row.signal_form,
                )?,
                signal_form: u8::try_from(row.signal_form)
                    .map_err(|_| corrupt("communication signal form"))?,
                associated_action: row
                    .associated_action
                    .map(|action| parse_action(&action))
                    .transpose()?,
            })
        })
        .collect()
}

fn communication_signal_sequence(
    preceding_signal: Option<i32>,
    signal_form: i32,
) -> Result<Vec<u8>, ObserverProjectionStoreError> {
    let preceding_signal = preceding_signal
        .map(|value| u8::try_from(value).map_err(|_| corrupt("communication preceding signal")))
        .transpose()?;
    let signal_form =
        u8::try_from(signal_form).map_err(|_| corrupt("communication signal form"))?;
    Ok(preceding_signal
        .into_iter()
        .chain(std::iter::once(signal_form))
        .collect())
}

fn parse_entity(row: EntityRow) -> Result<PublicHabitatEntity, ObserverProjectionStoreError> {
    Ok(PublicHabitatEntity {
        organism_id: EntityId::from_uuid(row.organism_id),
        role: parse_role(&row.role)?,
        species: SpeciesIdentity::new(
            row.species_catalog,
            row.species_identifier,
            row.species_scientific_name,
            row.species_source_url,
        )
        .map_err(|error| corrupt(&error.to_string()))?,
        embodied_patch: row
            .embodied_patch
            .parse()
            .map_err(|_| corrupt("embodied patch"))?,
        latitude_e7: row.latitude_e7,
        longitude_e7: row.longitude_e7,
        previous_latitude_e7: row.previous_latitude_e7,
        previous_longitude_e7: row.previous_longitude_e7,
        last_movement_tick: SimTick::new(to_u64(row.last_movement_tick, "movement tick")?),
        last_action: row
            .last_action
            .map(|value| parse_action(&value))
            .transpose()?,
        signal_form: row
            .signal_form
            .map(|value| u8::try_from(value).map_err(|_| corrupt("signal form")))
            .transpose()?,
        alive: row.alive,
    })
}

fn parse_cluster(row: ClusterRow) -> Result<PublicHabitatCluster, ObserverProjectionStoreError> {
    Ok(PublicHabitatCluster {
        cluster_key: format!("{}:{}", row.latitude_bucket, row.longitude_bucket),
        latitude_e7: row.latitude_e7,
        longitude_e7: row.longitude_e7,
        people: to_u64(row.people, "cluster people")?,
        animals: to_u64(row.animals, "cluster animals")?,
        total: to_u64(row.total, "cluster total")?,
    })
}

fn coordinate(patch: S2CellId) -> Result<(i32, i32), ObserverProjectionStoreError> {
    let uv = s2_face_ij_center_uv(decode_s2_face_ij(patch))
        .map_err(|error| corrupt(&error.to_string()))?;
    let ray = s2_face_uv_to_ray(uv).map_err(|error| corrupt(&error.to_string()))?;
    let coordinate = s2_ray_to_geographic_e7(ray).map_err(|error| corrupt(&error.to_string()))?;
    Ok((coordinate.latitude_e7(), coordinate.longitude_e7()))
}

const fn role_code(role: OrganismRole) -> &'static str {
    match role {
        OrganismRole::Person => "person",
        OrganismRole::Fauna => "fauna",
    }
}

fn parse_role(value: &str) -> Result<OrganismRole, ObserverProjectionStoreError> {
    match value {
        "person" => Ok(OrganismRole::Person),
        "fauna" => Ok(OrganismRole::Fauna),
        _ => Err(corrupt("organism role")),
    }
}

const fn communication_kind_code(kind: PublicHabitatCommunicationKind) -> &'static str {
    match kind {
        PublicHabitatCommunicationKind::HeardSignal => "heard_signal",
        PublicHabitatCommunicationKind::AssociatedAction => "associated_action",
    }
}

fn parse_communication_kind(
    value: &str,
) -> Result<PublicHabitatCommunicationKind, ObserverProjectionStoreError> {
    match value {
        "heard_signal" => Ok(PublicHabitatCommunicationKind::HeardSignal),
        "associated_action" => Ok(PublicHabitatCommunicationKind::AssociatedAction),
        _ => Err(corrupt("habitat communication kind")),
    }
}

const fn action_code(action: PrimitiveActionKind) -> &'static str {
    match action {
        PrimitiveActionKind::Move => "move",
        PrimitiveActionKind::Orient => "orient",
        PrimitiveActionKind::Reach => "reach",
        PrimitiveActionKind::Grasp => "grasp",
        PrimitiveActionKind::Release => "release",
        PrimitiveActionKind::ApplyForce => "apply_force",
        PrimitiveActionKind::Bite => "bite",
        PrimitiveActionKind::Chew => "chew",
        PrimitiveActionKind::Swallow => "swallow",
        PrimitiveActionKind::Rest => "rest",
        PrimitiveActionKind::EmitSignal => "emit_signal",
    }
}

fn parse_action(value: &str) -> Result<PrimitiveActionKind, ObserverProjectionStoreError> {
    match value {
        "move" => Ok(PrimitiveActionKind::Move),
        "orient" => Ok(PrimitiveActionKind::Orient),
        "reach" => Ok(PrimitiveActionKind::Reach),
        "grasp" => Ok(PrimitiveActionKind::Grasp),
        "release" => Ok(PrimitiveActionKind::Release),
        "apply_force" => Ok(PrimitiveActionKind::ApplyForce),
        "bite" => Ok(PrimitiveActionKind::Bite),
        "chew" => Ok(PrimitiveActionKind::Chew),
        "swallow" => Ok(PrimitiveActionKind::Swallow),
        "rest" => Ok(PrimitiveActionKind::Rest),
        "emit_signal" => Ok(PrimitiveActionKind::EmitSignal),
        _ => Err(corrupt("primitive action")),
    }
}

fn to_i64(value: u64, field: &str) -> Result<i64, ObserverProjectionStoreError> {
    i64::try_from(value).map_err(|_| corrupt(field))
}

fn to_u64(value: i64, field: &str) -> Result<u64, ObserverProjectionStoreError> {
    u64::try_from(value).map_err(|_| corrupt(field))
}

fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}

fn corrupt(message: &str) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{communication_signal_sequence, public_habitat_action};
    use world_domain::PrimitiveActionKind;

    #[test]
    fn public_habitat_excludes_violence_adjacent_actions() {
        assert!(!public_habitat_action(PrimitiveActionKind::Bite));
        assert!(public_habitat_action(PrimitiveActionKind::Move));
        assert!(public_habitat_action(PrimitiveActionKind::EmitSignal));
    }

    #[test]
    fn habitat_communication_preserves_compositional_prefixes() {
        assert_eq!(
            communication_signal_sequence(None, 5).expect("atomic signal"),
            vec![5]
        );
        assert_eq!(
            communication_signal_sequence(Some(27), 5).expect("compositional signal"),
            vec![27, 5]
        );
        assert!(communication_signal_sequence(Some(300), 5).is_err());
    }
}
