use anyhow::Result;
use application::{
    MemoryOutboxStore, MemoryRetain, MemoryRetainReceipt, StoreError, TransitionEffects,
    WorldStore, advance_world, initialize_or_resume_world, resume_world,
};
use postgres_store::PostgresStore;
use sim_engine::{EngineState, InitialOrganism, RULESET_VERSION, Snapshot, replay};
use sqlx::PgPool;
use uuid::Uuid;
use world_domain::{
    BirthCategory, Digest, EntityId, EventBatch, EventSequence, OrganismRole, SpeciesIdentity,
    WorldId, WorldManifest, WorldSeed, WorldStatus,
};

fn manifest(seed: u64) -> WorldManifest {
    WorldManifest::new(
        WorldId::from_uuid(Uuid::new_v4()),
        WorldSeed::new(seed),
        RULESET_VERSION,
    )
}

fn initial_person(world_id: WorldId) -> InitialOrganism {
    InitialOrganism {
        organism_id: EntityId::deterministic(world_id, b"postgres-proof-person"),
        species: SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("verified test species"),
        role: OrganismRole::Person,
        birth_category: BirthCategory::new("female").expect("valid birth category"),
        initial_age_ticks: 0,
        location_id: None,
    }
}

fn genesis(
    manifest: &WorldManifest,
    organisms: Vec<InitialOrganism>,
) -> Result<(EngineState, EventBatch, Snapshot)> {
    let initial = EngineState::new(manifest.clone());
    let events = initial.plan_genesis(organisms)?;
    let (running, batch) = initial.commit(EventSequence::new(1), Digest::ZERO, events)?;
    let snapshot = Snapshot::new(running.clone(), batch.sequence, batch.batch_hash)?;
    Ok((running, batch, snapshot))
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn commits_loads_and_replays_atomic_history(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool);
    let manifest = manifest(101);
    let created = store.create_world(&manifest, None).await?;
    let (running, genesis_batch, genesis_snapshot) =
        genesis(&manifest, vec![initial_person(manifest.world_id)])?;

    let persisted = store
        .commit_transition(
            created.cursor,
            &genesis_batch,
            &genesis_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    assert_eq!(persisted.status, WorldStatus::Running);
    assert_eq!(persisted.cursor.sequence, EventSequence::new(1));

    let tick_events = running.plan_next_tick()?;
    let (after_tick, tick_batch) =
        running.commit(EventSequence::new(2), genesis_batch.batch_hash, tick_events)?;
    let tick_snapshot = Snapshot::new(
        after_tick.clone(),
        tick_batch.sequence,
        tick_batch.batch_hash,
    )?;
    let persisted = store
        .commit_transition(
            persisted.cursor,
            &tick_batch,
            &tick_snapshot,
            &TransitionEffects::default(),
        )
        .await?;

    let batches = store
        .load_event_batches(manifest.world_id, EventSequence::ZERO)
        .await?;
    let replayed = replay(manifest.clone(), &batches)?;
    let latest_snapshot = store.load_latest_snapshot(manifest.world_id).await?;
    let loaded_world = store.load_world(manifest.world_id).await?;

    assert_eq!(batches, vec![genesis_batch, tick_batch]);
    assert_eq!(replayed.state, after_tick);
    assert_eq!(latest_snapshot, tick_snapshot);
    assert_eq!(loaded_world, persisted);
    assert_eq!(replayed.last_event_hash, persisted.cursor.last_event_hash);
    assert_eq!(replayed.state.state_hash()?, persisted.cursor.state_hash);
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn runtime_replays_and_resumes_at_the_exact_next_sequence(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool);
    let manifest = manifest(151);
    let initialized = initialize_or_resume_world(
        &store,
        manifest.clone(),
        None,
        vec![initial_person(manifest.world_id)],
    )
    .await?;
    assert_eq!(initialized.world.cursor.sequence, EventSequence::new(1));

    let after_first_tick = advance_world(&store, &initialized).await?;
    assert_eq!(
        after_first_tick.world.cursor.sequence,
        EventSequence::new(2)
    );
    assert_eq!(
        after_first_tick.world.cursor.tick,
        world_domain::SimTick::new(1)
    );

    let resumed = resume_world(&store, manifest.world_id).await?;
    assert_eq!(resumed, after_first_tick);
    let after_restart_tick = advance_world(&store, &resumed).await?;
    assert_eq!(
        after_restart_tick.world.cursor.sequence,
        EventSequence::new(3)
    );
    assert_eq!(
        after_restart_tick.world.cursor.tick,
        world_domain::SimTick::new(2)
    );

    let idempotent_initialization = initialize_or_resume_world(
        &store,
        manifest.clone(),
        None,
        vec![initial_person(manifest.world_id)],
    )
    .await?;
    assert_eq!(idempotent_initialization, after_restart_tick);
    assert_eq!(
        store.list_running_world_ids().await?,
        vec![manifest.world_id]
    );

    let batches = store
        .load_event_batches(manifest.world_id, EventSequence::ZERO)
        .await?;
    assert_eq!(
        batches
            .iter()
            .map(|batch| batch.sequence)
            .collect::<Vec<_>>(),
        vec![
            EventSequence::new(1),
            EventSequence::new(2),
            EventSequence::new(3)
        ]
    );
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn subjective_memory_delivery_is_atomic_leased_and_immutable(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(181);
    let person = initial_person(manifest.world_id);
    let person_id = person.organism_id;
    let created = store.create_world(&manifest, None).await?;
    let (_, genesis_batch, genesis_snapshot) = genesis(&manifest, vec![person])?;
    let retain = MemoryRetain::new(
        manifest.world_id,
        person_id,
        genesis_batch.sequence,
        genesis_batch.tick,
        0,
        "A cold gust was followed by discomfort.",
        "direct perception",
    )?;
    let effects = TransitionEffects {
        memory_retains: vec![retain.clone()],
    };
    store
        .commit_transition(created.cursor, &genesis_batch, &genesis_snapshot, &effects)
        .await?;

    let claimed = store
        .claim_next_memory("memory-worker-a", 60)
        .await?
        .expect("committed memory is claimable");
    assert_eq!(claimed.retain, retain);
    assert_eq!(claimed.attempt_count, 1);
    assert!(
        store
            .claim_next_memory("memory-worker-b", 60)
            .await?
            .is_none()
    );

    let receipt = MemoryRetainReceipt {
        operation_id: retain.operation_id,
        remote_operation_id: retain.operation_id.to_string(),
        adapter_version: "test-hindsight-adapter".to_owned(),
    };
    store
        .mark_memory_accepted("memory-worker-a", &claimed, &receipt)
        .await?;
    assert!(
        store
            .claim_next_memory("memory-worker-b", 60)
            .await?
            .is_none()
    );

    let accepted: (
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT remote_operation_id, adapter_version, completed_at FROM memory_outbox WHERE operation_id = $1",
    )
    .bind(retain.operation_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(accepted.0, Some(retain.operation_id.to_string()));
    assert_eq!(accepted.1.as_deref(), Some("test-hindsight-adapter"));
    assert!(accepted.2.is_some());

    let mutation =
        sqlx::query("UPDATE memory_outbox SET payload = payload WHERE operation_id = $1")
            .bind(retain.operation_id)
            .execute(&pool)
            .await;
    assert!(mutation.is_err());
    let deletion = sqlx::query("DELETE FROM memory_outbox WHERE operation_id = $1")
        .bind(retain.operation_id)
        .execute(&pool)
        .await;
    assert!(deletion.is_err());
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn rejects_stale_writer_and_event_mutation(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(202);
    let created = store.create_world(&manifest, None).await?;
    let (_, batch, snapshot) = genesis(&manifest, vec![initial_person(manifest.world_id)])?;
    store
        .commit_transition(
            created.cursor,
            &batch,
            &snapshot,
            &TransitionEffects::default(),
        )
        .await?;

    let stale = store
        .commit_transition(
            created.cursor,
            &batch,
            &snapshot,
            &TransitionEffects::default(),
        )
        .await;
    assert!(matches!(stale, Err(StoreError::Conflict(_))));

    let mutation = sqlx::query(
        "UPDATE event_batches SET payload = payload WHERE world_id = $1 AND sequence = 1",
    )
    .bind(manifest.world_id.as_uuid())
    .execute(&pool)
    .await;
    assert!(mutation.is_err());

    let cursor_jump =
        sqlx::query("UPDATE worlds SET current_sequence = current_sequence + 2 WHERE id = $1")
            .bind(manifest.world_id.as_uuid())
            .execute(&pool)
            .await;
    assert!(cursor_jump.is_err());

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_batches WHERE world_id = $1")
        .bind(manifest.world_id.as_uuid())
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn successor_requires_an_immutable_archived_predecessor(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let first_manifest = manifest(303);
    let created = store.create_world(&first_manifest, None).await?;
    let (running, genesis_batch, genesis_snapshot) = genesis(&first_manifest, Vec::new())?;
    let running_world = store
        .commit_transition(
            created.cursor,
            &genesis_batch,
            &genesis_snapshot,
            &TransitionEffects::default(),
        )
        .await?;

    let premature_manifest = manifest(304);
    let premature = store
        .create_world(&premature_manifest, Some(first_manifest.world_id))
        .await;
    assert!(matches!(premature, Err(StoreError::Conflict(_))));

    let archive_events = running.plan_next_tick()?;
    let (archived, archive_batch) = running.commit(
        EventSequence::new(2),
        genesis_batch.batch_hash,
        archive_events,
    )?;
    let archive_snapshot =
        Snapshot::new(archived, archive_batch.sequence, archive_batch.batch_hash)?;
    let archived_world = store
        .commit_transition(
            running_world.cursor,
            &archive_batch,
            &archive_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    assert_eq!(archived_world.status, WorldStatus::Archived);

    let successor_manifest = manifest(305);
    let successor = store
        .create_world(&successor_manifest, Some(first_manifest.world_id))
        .await?;
    assert_eq!(
        successor.predecessor_world_id,
        Some(first_manifest.world_id)
    );

    let archive_mutation = sqlx::query("UPDATE worlds SET current_tick = 99 WHERE id = $1")
        .bind(first_manifest.world_id.as_uuid())
        .execute(&pool)
        .await;
    assert!(archive_mutation.is_err());
    Ok(())
}
