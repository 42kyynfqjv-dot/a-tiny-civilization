use application::WorldStore;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use observer_projection::{
    ObserverHistoryCommitmentStore, ObserverProjectionStoreError, PublicHistoryCommitment,
    PublicHistoryCommitmentPage,
};
use world_domain::{Digest, EventSequence, WorldId};

use crate::{
    PostgresStore,
    world_store::{EventBatchRow, parse_event_batch},
};

#[async_trait]
impl ObserverHistoryCommitmentStore for PostgresStore {
    async fn public_history_commitments(
        &self,
        world_id: WorldId,
        after_sequence: EventSequence,
        limit: u16,
    ) -> Result<PublicHistoryCommitmentPage, ObserverProjectionStoreError> {
        if limit == 0 || limit > 256 {
            return Err(ObserverProjectionStoreError::Corrupt(
                "history commitment limit must be between 1 and 256".to_owned(),
            ));
        }
        let world = self.load_world(world_id).await.map_err(map_store_error)?;
        if after_sequence > world.cursor.sequence {
            return Err(ObserverProjectionStoreError::Corrupt(
                "history commitment cursor exceeds the world head".to_owned(),
            ));
        }
        let after = i64::try_from(after_sequence.get())
            .map_err(|_| corrupt("history commitment cursor overflow"))?;
        let row_limit = i64::from(limit) + 1;
        let expected_previous = if after_sequence == EventSequence::ZERO {
            Digest::ZERO
        } else {
            let encoded = sqlx::query_scalar::<_, Vec<u8>>(
                "SELECT checksum FROM event_batches WHERE world_id=$1 AND sequence=$2",
            )
            .bind(world_id.as_uuid())
            .bind(after)
            .fetch_optional(self.pool())
            .await
            .map_err(|error| ObserverProjectionStoreError::Unavailable(error.to_string()))?
            .ok_or_else(|| corrupt("history commitment cursor does not identify a batch"))?;
            digest_from_bytes(&encoded, "history commitment cursor checksum")?
        };
        let rows = sqlx::query_as::<_, EventBatchRow>(
            r#"
            SELECT world_id,sequence,tick,event_schema_version,ruleset_version,payload,
                   payload_encoding,compressed_payload,uncompressed_payload_bytes,
                   checksum,previous_checksum,post_state_checksum
            FROM event_batches
            WHERE world_id=$1 AND sequence>$2
            ORDER BY sequence ASC
            LIMIT $3
            "#,
        )
        .bind(world_id.as_uuid())
        .bind(after)
        .bind(row_limit)
        .fetch(self.pool());
        // Cancer World stores compressed canonical payloads whose decoded batches
        // can be large. Stream and reduce each row to its public header before
        // fetching the next one; never retain a page of decoded canonical events.
        let mut rows = Box::pin(rows);
        let mut expected_previous = expected_previous;
        let mut expected_sequence = after_sequence.get().saturating_add(1);
        let mut commitments = Vec::with_capacity(usize::from(limit));
        let mut has_more = false;
        while let Some(row) = rows
            .try_next()
            .await
            .map_err(|error| ObserverProjectionStoreError::Unavailable(error.to_string()))?
        {
            if commitments.len() == usize::from(limit) {
                has_more = true;
                break;
            }
            let batch = parse_event_batch(row).map_err(map_store_error)?;
            batch
                .verify_integrity()
                .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?;
            if batch.sequence.get() != expected_sequence || batch.previous_hash != expected_previous
            {
                return Err(corrupt("noncontiguous public history commitment range"));
            }
            let event_count = u32::try_from(batch.events.len())
                .map_err(|_| corrupt("history batch event count overflow"))?;
            commitments.push(PublicHistoryCommitment {
                sequence: batch.sequence,
                tick: batch.tick,
                event_schema_version: batch.event_schema_version,
                ruleset_version: batch.ruleset_version,
                event_count,
                previous_event_hash: batch.previous_hash,
                batch_hash: batch.batch_hash,
                post_state_hash: batch.post_state_hash,
            });
            expected_previous = batch.batch_hash;
            expected_sequence = expected_sequence.saturating_add(1);
        }
        let next_after_sequence = if has_more {
            commitments.last().map(|item| item.sequence)
        } else {
            None
        };
        let manifest_hash = Digest::canonical(&world.manifest)
            .map_err(|error| ObserverProjectionStoreError::Corrupt(error.to_string()))?;
        Ok(PublicHistoryCommitmentPage {
            world_id,
            manifest: world.manifest,
            manifest_hash,
            head_sequence: world.cursor.sequence,
            head_event_hash: world.cursor.last_event_hash,
            head_state_hash: world.cursor.state_hash,
            after_sequence,
            commitments,
            next_after_sequence,
        })
    }
}

fn map_store_error(error: application::StoreError) -> ObserverProjectionStoreError {
    match error {
        application::StoreError::NotFound(message) => {
            ObserverProjectionStoreError::NotFound(message)
        }
        application::StoreError::Corrupt(message) => ObserverProjectionStoreError::Corrupt(message),
        other => ObserverProjectionStoreError::Unavailable(other.to_string()),
    }
}

fn corrupt(message: &str) -> ObserverProjectionStoreError {
    ObserverProjectionStoreError::Corrupt(message.to_owned())
}

fn digest_from_bytes(encoded: &[u8], field: &str) -> Result<Digest, ObserverProjectionStoreError> {
    let bytes: [u8; 32] = encoded
        .try_into()
        .map_err(|_| corrupt(&format!("invalid stored {field}")))?;
    Ok(Digest::from_bytes(bytes))
}
