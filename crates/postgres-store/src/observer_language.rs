use async_trait::async_trait;
use observer_projection::{
    ObserverLanguageStore, ObserverProjectionStoreError, PUBLIC_LANGUAGE_PROJECTION_NAME,
    PUBLIC_LANGUAGE_PROJECTION_VERSION, PUBLIC_ORGANISM_PROJECTION_VERSION, PublicLanguageArchive,
    PublicLanguageConvention, PublicLanguageStage, PublicLanguageThreshold,
};
use sqlx::FromRow;
use world_domain::{DomainEvent, EventId, EventSequence, PrimitiveActionKind, SimTick, WorldId};

use crate::{
    PostgresStore, advance_projection_cursor, lock_projection_cursor, verify_committed_batch_range,
};

const DETECTOR_VERSION: u16 = 2;
const MINIMUM_EVIDENCE_EVENTS: u32 = 12;
const MINIMUM_LEARNERS: u32 = 4;
const MINIMUM_SIGNAL_SOURCES: u32 = 3;
const MINIMUM_TICK_SPAN: u64 = 288;
const MINIMUM_DOMINANCE_PERCENT: u16 = 60;
const CONVENTIONS_FOR_LANGUAGE_CANDIDATE: u16 = 3;

#[derive(FromRow)]
struct ConventionRow {
    signal_form: i16,
    action: String,
    movement_direction: Option<i16>,
    evidence_events: i64,
    learners: i64,
    signal_sources: i64,
    form_events: i64,
    first_event_id: uuid::Uuid,
    first_sequence: i64,
    first_tick: i64,
    latest_event_id: uuid::Uuid,
    latest_sequence: i64,
    latest_tick: i64,
}

#[async_trait]
impl ObserverLanguageStore for PostgresStore {
    async fn apply_public_language_batches(
        &self,
        batches: &[world_domain::EventBatch],
    ) -> Result<u64, ObserverProjectionStoreError> {
        let Some(first) = batches.first() else {
            return Ok(0);
        };
        let mut transaction = self.pool().begin().await.map_err(unavailable)?;
        let cursor = lock_projection_cursor(
            &mut transaction,
            PUBLIC_LANGUAGE_PROJECTION_NAME,
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
            return Err(corrupt("public language batch range is not contiguous"));
        }
        verify_committed_batch_range(&mut transaction, pending).await?;

        for batch in pending {
            for record in &batch.events {
                let DomainEvent::OrganismSignalActionAssociationChanged {
                    observer_id,
                    actor_id,
                    to,
                    ..
                } = &record.event
                else {
                    continue;
                };
                sqlx::query(
                    r#"
                    INSERT INTO observer_language_evidence (
                        projection_version,world_id,source_event_id,source_sequence,source_tick,
                        source_event_index,observer_id,actor_id,signal_form,action,movement_direction
                    ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                    ON CONFLICT (projection_version,world_id,source_event_id) DO NOTHING
                    "#,
                )
                .bind(i32::from(PUBLIC_LANGUAGE_PROJECTION_VERSION))
                .bind(batch.world_id.as_uuid())
                .bind(record.event_id.as_uuid())
                .bind(to_i64(batch.sequence.get(), "source sequence")?)
                .bind(to_i64(batch.tick.get(), "source tick")?)
                .bind(i32::try_from(record.index).map_err(|_| corrupt("source event index"))?)
                .bind(observer_id.as_uuid())
                .bind(actor_id.as_uuid())
                .bind(i16::from(to.signal_intensity))
                .bind(action_code(to.action_kind))
                .bind(to.movement_direction.map(i16::from))
                .execute(&mut *transaction)
                .await
                .map_err(unavailable)?;
            }
        }
        let last_sequence = to_i64(pending[pending.len() - 1].sequence.get(), "source sequence")?;
        advance_projection_cursor(
            &mut transaction,
            PUBLIC_LANGUAGE_PROJECTION_NAME,
            first.world_id,
            last_sequence,
        )
        .await?;
        transaction.commit().await.map_err(unavailable)?;
        u64::try_from(pending.len()).map_err(|_| corrupt("language batch count"))
    }

    async fn public_language_cursor(
        &self,
        world_id: WorldId,
    ) -> Result<EventSequence, ObserverProjectionStoreError> {
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT through_sequence FROM projection_offsets WHERE projection_name=$1 AND world_id=$2",
        )
        .bind(PUBLIC_LANGUAGE_PROJECTION_NAME)
        .bind(world_id.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?
        .unwrap_or(0);
        Ok(EventSequence::new(to_u64(cursor, "language cursor")?))
    }

    async fn public_language_archive(
        &self,
        world_id: WorldId,
    ) -> Result<PublicLanguageArchive, ObserverProjectionStoreError> {
        let through_sequence = self.public_language_cursor(world_id).await?;
        let rows = sqlx::query_as::<_, ConventionRow>(
            r#"
            WITH eligible_evidence AS (
                SELECT evidence.*
                FROM observer_language_evidence evidence
                JOIN observer_organisms learner
                  ON learner.projection_version=$3
                 AND learner.world_id=evidence.world_id
                 AND learner.organism_id=evidence.observer_id
                 AND learner.role='person'
                JOIN observer_organisms source
                  ON source.projection_version=$3
                 AND source.world_id=evidence.world_id
                 AND source.organism_id=evidence.actor_id
                 AND source.role='person'
                WHERE evidence.projection_version=$1
                  AND evidence.world_id=$2
                  AND evidence.action NOT IN ('bite','emit_signal')
            ), meanings AS (
                SELECT signal_form,action,movement_direction,
                    COUNT(*)::BIGINT AS evidence_events,
                    COUNT(DISTINCT observer_id)::BIGINT AS learners,
                    COUNT(DISTINCT actor_id)::BIGINT AS signal_sources,
                    (ARRAY_AGG(source_event_id ORDER BY source_sequence,source_event_index))[1] AS first_event_id,
                    MIN(source_sequence)::BIGINT AS first_sequence,
                    MIN(source_tick)::BIGINT AS first_tick,
                    (ARRAY_AGG(source_event_id ORDER BY source_sequence DESC,source_event_index DESC))[1] AS latest_event_id,
                    MAX(source_sequence)::BIGINT AS latest_sequence,
                    MAX(source_tick)::BIGINT AS latest_tick
                FROM eligible_evidence
                GROUP BY signal_form,action,movement_direction
            ), form_totals AS (
                SELECT signal_form,COUNT(*)::BIGINT AS form_events
                FROM eligible_evidence
                GROUP BY signal_form
            )
            SELECT meanings.*,form_totals.form_events
            FROM meanings JOIN form_totals USING (signal_form)
            ORDER BY signal_form, evidence_events DESC, action, movement_direction NULLS FIRST
            "#,
        )
        .bind(i32::from(PUBLIC_LANGUAGE_PROJECTION_VERSION))
        .bind(world_id.as_uuid())
        .bind(i32::from(PUBLIC_ORGANISM_PROJECTION_VERSION))
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;

        let mut conventions = Vec::new();
        for row in rows {
            let evidence_events = to_u32(row.evidence_events, "language evidence count")?;
            let learners = to_u32(row.learners, "language learner count")?;
            let signal_sources = to_u32(row.signal_sources, "language source count")?;
            let first_tick = to_u64(row.first_tick, "first language tick")?;
            let latest_tick = to_u64(row.latest_tick, "latest language tick")?;
            let dominance_percent = if row.form_events <= 0 {
                0
            } else {
                u16::try_from((row.evidence_events * 100) / row.form_events)
                    .map_err(|_| corrupt("language dominance"))?
            };
            if evidence_events < MINIMUM_EVIDENCE_EVENTS
                || learners < MINIMUM_LEARNERS
                || signal_sources < MINIMUM_SIGNAL_SOURCES
                || latest_tick.saturating_sub(first_tick) < MINIMUM_TICK_SPAN
                || dominance_percent < MINIMUM_DOMINANCE_PERCENT
            {
                continue;
            }
            let action = parse_action(&row.action)?;
            let movement_direction = row
                .movement_direction
                .map(|value| u8::try_from(value).map_err(|_| corrupt("movement direction")))
                .transpose()?;
            conventions.push(PublicLanguageConvention {
                signal_form: u8::try_from(row.signal_form).map_err(|_| corrupt("signal form"))?,
                tentative_gloss: tentative_gloss(action, movement_direction),
                associated_action: action,
                movement_direction,
                evidence_events,
                learners,
                signal_sources,
                dominance_percent,
                first_event_id: EventId::from_uuid(row.first_event_id),
                first_sequence: EventSequence::new(to_u64(row.first_sequence, "first sequence")?),
                first_tick: SimTick::new(first_tick),
                latest_event_id: EventId::from_uuid(row.latest_event_id),
                latest_sequence: EventSequence::new(to_u64(
                    row.latest_sequence,
                    "latest sequence",
                )?),
                latest_tick: SimTick::new(latest_tick),
            });
        }
        conventions.sort_by_key(|item| {
            (
                item.signal_form,
                item.associated_action,
                item.movement_direction,
            )
        });
        let distinct_meanings = conventions
            .iter()
            .map(|item| (item.associated_action, item.movement_direction))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let stage = if distinct_meanings >= usize::from(CONVENTIONS_FOR_LANGUAGE_CANDIDATE) {
            PublicLanguageStage::RudimentaryLanguageCandidate
        } else if conventions.is_empty() {
            PublicLanguageStage::Undetected
        } else {
            PublicLanguageStage::ProtoLexicon
        };
        Ok(PublicLanguageArchive {
            projection_version: PUBLIC_LANGUAGE_PROJECTION_VERSION,
            detector_version: DETECTOR_VERSION,
            world_id,
            through_sequence,
            stage,
            threshold: threshold(),
            conventions,
        })
    }
}

const fn threshold() -> PublicLanguageThreshold {
    PublicLanguageThreshold {
        minimum_evidence_events: MINIMUM_EVIDENCE_EVENTS,
        minimum_learners: MINIMUM_LEARNERS,
        minimum_signal_sources: MINIMUM_SIGNAL_SOURCES,
        minimum_tick_span: MINIMUM_TICK_SPAN,
        minimum_dominance_percent: MINIMUM_DOMINANCE_PERCENT,
        conventions_for_language_candidate: CONVENTIONS_FOR_LANGUAGE_CANDIDATE,
    }
}

fn tentative_gloss(action: PrimitiveActionKind, movement_direction: Option<u8>) -> String {
    match (action, movement_direction) {
        (PrimitiveActionKind::Move, Some(direction)) => format!("movement coordinate {direction}"),
        (PrimitiveActionKind::Move, None) => "movement".to_owned(),
        (PrimitiveActionKind::Orient, _) => "orientation".to_owned(),
        (PrimitiveActionKind::Reach, _) => "reaching".to_owned(),
        (PrimitiveActionKind::Grasp, _) => "grasping".to_owned(),
        (PrimitiveActionKind::Release, _) => "release".to_owned(),
        (PrimitiveActionKind::ApplyForce, _) => "surface pressure".to_owned(),
        (PrimitiveActionKind::Bite, _) => "withheld by presentation policy".to_owned(),
        (PrimitiveActionKind::Chew, _) => "chewing".to_owned(),
        (PrimitiveActionKind::Swallow, _) => "swallowing".to_owned(),
        (PrimitiveActionKind::Rest, _) => "resting".to_owned(),
        (PrimitiveActionKind::EmitSignal, _) => "signal response".to_owned(),
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
        _ => Err(corrupt("language action")),
    }
}

fn to_i64(value: u64, field: &str) -> Result<i64, ObserverProjectionStoreError> {
    i64::try_from(value).map_err(|_| corrupt(field))
}

fn to_u64(value: i64, field: &str) -> Result<u64, ObserverProjectionStoreError> {
    u64::try_from(value).map_err(|_| corrupt(field))
}

fn to_u32(value: i64, field: &str) -> Result<u32, ObserverProjectionStoreError> {
    u32::try_from(value).map_err(|_| corrupt(field))
}

fn unavailable(error: sqlx::Error) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Unavailable(error.to_string())
}

fn corrupt(message: &str) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_threshold_is_deliberately_stricter_than_repetition() {
        let threshold = threshold();
        assert!(threshold.minimum_evidence_events > threshold.minimum_learners);
        assert!(threshold.minimum_signal_sources > 1);
        assert!(threshold.minimum_tick_span > 0);
        assert!(threshold.minimum_dominance_percent > 50);
        assert!(threshold.conventions_for_language_candidate > 1);
    }
}
