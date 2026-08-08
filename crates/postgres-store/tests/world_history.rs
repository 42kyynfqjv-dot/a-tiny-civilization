use anyhow::Result;
use application::{
    AgentMemory, COGNITION_MODEL_CONTRACT_VERSION, CognitionAttemptPersistenceState,
    CognitionJobEntry, CognitionJobStore, CognitionModel, CognitionModelError, CognitionModelRoute,
    CognitionProviderId, CognitionRecallRecord, CognitionRouteAttempt, CognitionRouteAttemptStatus,
    CognitionRoutePurpose, CognitionRouteRegistry, CognitionWorkerConfiguration,
    CognitionWorkerStep, MemoryAdapterError, MemoryFactKind, MemoryOutboxStore,
    MemoryRecallOutcome, MemoryRecallRequest, MemoryRetain, MemoryRetainReceipt,
    ModelCognitionLadderResult, ModelCognitionReceipt, ModelCognitionRequest, ModelTokenUsage,
    PaidCognitionReservationDecision, RecallUnavailableReason, RecalledMemory, StoreError,
    StoredWorld, TransitionEffects, WorldStore, advance_world,
    initialize_or_resume_configured_world, initialize_or_resume_configured_world_with_materials,
    initialize_or_resume_world, process_next_cognition_job, resume_world,
    resume_world_from_snapshot,
};
use async_trait::async_trait;
use observer_projection::{
    CommittedBirth, ObserverFindingStore, ObserverOrganismStore, ObserverTimelineStore,
    ObserverWorldStore, PublicWorldInputStatus, ReservationRequest, ReservationState,
    ReservationTarget, SupporterReservationStore,
};
use postgres_store::PostgresStore;
use sim_engine::{
    COGNITION_RULESET_VERSION, EngineState, InitialMaterialInstance, InitialOrganism,
    MATERIAL_RESERVOIR_RULESET_VERSION, RULESET_VERSION, Snapshot, replay,
};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use stripe_adapter::{
    StripeCheckoutSession, StripeCheckoutSessionStore, StripeCheckoutStoreError,
    StripeWebhookDisposition, StripeWebhookStore, StripeWebhookStoreError, VerifiedCheckoutPayment,
    VerifiedStripeEvent,
};
use uuid::Uuid;
use world_domain::{
    BirthCategory, CapacityExhaustionPolicy, CartesianMillimetres, CelestialState,
    CognitionInputOutcome, CognitionUnavailableReason, DeathCause, Digest, DomainEvent,
    EarthResolutionLevels, EntityId, EventBatch, EventId, EventSequence, FullEarthGrid,
    HeritableDispositionProfile, MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION, MaterialIdentity,
    MaterialReservoirCommitment, MetabolicRateCommitment, OralTransferCommitment,
    OralTransferEvidenceBasis, OrganismRole, PartitionedExecution, PersonRepresentation,
    PhysiologicalEvidenceBasis, PhysiologicalRegulationCommitment, PrimitiveActionKind,
    ProvisionalLocalEnvironmentBaseline, ProvisionalWorldCompositionReference,
    ReproductiveCategoryPair, ReproductivePhysiologyCommitment, S2CellId, S2Projection,
    SchedulerKind, SimTick, SpeciesIdentity, TdbSecondsSinceJ2000, WorldConfiguration, WorldId,
    WorldManifest, WorldSeed, WorldStatus,
};

#[sqlx::test(migrations = "../../db/migrations")]
async fn canonical_runner_writer_lock_is_exclusive_and_crash_released(pool: PgPool) -> Result<()> {
    let first = PostgresStore::from_pool(pool.clone());
    let second = PostgresStore::from_pool(pool);
    let held = first.acquire_runner_writer_lock().await?;
    assert!(matches!(
        second.acquire_runner_writer_lock().await,
        Err(StoreError::Conflict(message)) if message.contains("canonical-writer lock")
    ));
    drop(held);
    let reacquired = second.acquire_runner_writer_lock().await?;
    drop(reacquired);
    Ok(())
}

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
        embodied_patch: None,
        metabolic_rate: None,
        physiological_regulation: None,
        reproductive_physiology: None,
        heritable_disposition_profile: None,
    }
}

fn provisional_configuration() -> WorldConfiguration {
    WorldConfiguration::new_provisional_full_earth(
        300,
        FullEarthGrid {
            physics_crs_epsg: 4_978,
            catalog_crs_epsg: 4_979,
            vertical_crs_epsg: 3_855,
            s2_definition_url: "https://s2geometry.io/devguide/s2cell_hierarchy".to_owned(),
            s2_library_revision: "0123456789abcdef".to_owned(),
            s2_definition_hash: Digest::sha256(b"PostgreSQL provisional S2 fixture"),
            s2_projection: S2Projection::Quadratic,
            levels: EarthResolutionLevels {
                planetary_aggregate: 10,
                regional_ecology: 14,
                active_landscape: 18,
                embodied_patch: 23,
            },
            refinement_policy_version: 1,
        },
        ProvisionalWorldCompositionReference::new(
            1,
            "full-earth-breadth-first",
            "0.1.0",
            Digest::sha256(b"PostgreSQL provisional composition fixture"),
        )
        .expect("valid provisional reference"),
        PartitionedExecution {
            scheduler_schema_version: 1,
            scheduler: SchedulerKind::DeterministicEventQueue,
            partition_s2_level: 10,
            person_representation: PersonRepresentation::DurableIndividuals,
            capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
            max_events_per_partition_transition: 10_000,
        },
    )
    .expect("valid provisional configuration")
}

fn provisional_initial_person(world_id: WorldId) -> InitialOrganism {
    let mut person = initial_person(world_id);
    person.embodied_patch = Some(
        "0000000000004000"
            .parse::<S2CellId>()
            .expect("valid L23 cell"),
    );
    person
}

fn cognition_configuration() -> WorldConfiguration {
    let provisional = provisional_configuration();
    let active_patch = "0000000000004000"
        .parse::<S2CellId>()
        .expect("valid L23 cell");
    WorldConfiguration::new_provisional_full_earth_with_environment_baseline(
        300,
        provisional.full_earth_grid().expect("grid").clone(),
        provisional
            .provisional_world_composition()
            .expect("composition")
            .clone(),
        provisional
            .partitioned_execution()
            .expect("execution")
            .clone(),
        ProvisionalLocalEnvironmentBaseline {
            status: "provisional-evidence-only".to_owned(),
            source_evidence_digest: Digest::sha256(b"cognition PostgreSQL local evidence"),
            evidence_patch: active_patch.ancestor(10).expect("L10 ancestor"),
            active_patch,
            air_temperature_unit: "degC".to_owned(),
            air_temperature_decimal_places: 1,
            air_temperature_normal_minimum: [1; 12],
            air_temperature_normal_mean: [2; 12],
            air_temperature_normal_maximum: [3; 12],
        },
    )
    .expect("cognition configuration")
}

fn cognition_initial_person(world_id: WorldId) -> InitialOrganism {
    let mut person = provisional_initial_person(world_id);
    person.initial_age_ticks = 20;
    person.metabolic_rate = Some(MetabolicRateCommitment {
        commitment_schema_version: world_domain::METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
        evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
        profile_set_digest: Digest::sha256(b"cognition PostgreSQL metabolic profiles"),
        observed_species: person.species.clone(),
        source_record_id: "cognition-postgres-rate".to_owned(),
        source_record_digest: Digest::sha256(b"cognition PostgreSQL metabolic row"),
        measured_power_value: 1,
        measured_power_decimal_places: 0,
    });
    person.physiological_regulation = Some(PhysiologicalRegulationCommitment {
        commitment_schema_version: world_domain::PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION,
        profile_id: "cognition-postgres-regulation-v1".to_owned(),
        profile_digest: Digest::sha256(b"cognition PostgreSQL regulation assumptions"),
        species: person.species.clone(),
        evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
        usable_energy_reserve_joules: 10_000_000_000,
        hydration_failure_seconds: 10_000_000,
        fatigue_failure_seconds: 600,
        fatigue_recovery_seconds: 600,
        thermoneutral_min_millicelsius: -1_000,
        thermoneutral_max_millicelsius: 1_000,
        thermal_failure_millicelsius_seconds: 600_000,
        thermal_recovery_seconds: 600,
    });
    person.reproductive_physiology = Some(ReproductivePhysiologyCommitment {
        commitment_schema_version: world_domain::REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
        profile_id: "cognition-postgres-reproduction-v1".to_owned(),
        profile_digest: Digest::sha256(b"cognition PostgreSQL reproduction assumptions"),
        species: person.species.clone(),
        evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
        tick_duration_seconds: 300,
        maturity_age_ticks: 10,
        development_ticks: 2,
        recovery_ticks: 2,
        opportunity_interval_ticks: 1,
        initiation_probability_millionths: world_domain::REPRODUCTIVE_PROBABILITY_SCALE,
        compatible_pairs: vec![ReproductiveCategoryPair {
            first: BirthCategory::new("female").expect("category"),
            second: BirthCategory::new("male").expect("category"),
            developing_parent: BirthCategory::new("female").expect("category"),
        }],
        offspring_categories: vec![
            world_domain::OffspringCategoryWeight {
                category: BirthCategory::new("female").expect("category"),
                weight: 1,
            },
            world_domain::OffspringCategoryWeight {
                category: BirthCategory::new("male").expect("category"),
                weight: 1,
            },
        ],
    });
    person.heritable_disposition_profile = Some(HeritableDispositionProfile {
        profile_schema_version: world_domain::HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
        profile_id: "cognition-postgres-heredity-v1".to_owned(),
        profile_digest: Digest::sha256(b"cognition PostgreSQL heredity assumptions"),
        species: person.species.clone(),
        evidence_basis: PhysiologicalEvidenceBasis::EngineeringAssumption,
        minimum_action_weight: 4,
        neutral_action_weight: 16,
        maximum_action_weight: 28,
        founder_variation_steps: 3,
        mutation_probability_millionths: 100_000,
        mutation_max_step: 2,
    });
    person
}

fn shared_water_reservoir(world_id: WorldId, person: &InitialOrganism) -> InitialMaterialInstance {
    let material = MaterialIdentity::new(
        "pubchem",
        "962",
        "water",
        "https://pubchem.ncbi.nlm.nih.gov/compound/962",
    )
    .expect("citable water identity");
    let embodied_patch = person.embodied_patch.expect("embodied founder patch");
    InitialMaterialInstance {
        object_id: EntityId::deterministic(world_id, b"postgres-shared-water-reservoir"),
        material: material.clone(),
        embodied_patch,
        initial_mass_milligrams: Some(1_000_000),
        oral_transfer_profiles: vec![OralTransferCommitment {
            commitment_schema_version: world_domain::ORAL_TRANSFER_COMMITMENT_SCHEMA_VERSION,
            profile_id: "postgres-water-human-v1".to_owned(),
            profile_digest: Digest::sha256(b"PostgreSQL water response fixture"),
            material: material.clone(),
            species: person.species.clone(),
            evidence_basis: OralTransferEvidenceBasis::EngineeringAssumption,
            transfer_mass_milligrams: 250_000,
            recoverable_energy_joules: 0,
            hydration_recovery_seconds: 14_400,
        }],
        reservoir: Some(MaterialReservoirCommitment {
            commitment_schema_version: world_domain::MATERIAL_RESERVOIR_COMMITMENT_SCHEMA_VERSION,
            profile_id: "postgres-shared-water-reservoir-v1".to_owned(),
            profile_digest: Digest::sha256(b"PostgreSQL shared water reservoir fixture"),
            material,
            evidence_basis: OralTransferEvidenceBasis::EngineeringAssumption,
            coverage_patch: embodied_patch.ancestor(10).expect("L10 coverage"),
            maximum_mass_milligrams: 10_000_000,
            replenishment_mass_milligrams_per_tick: 1_000_000,
        }),
    }
}

async fn claimed_cognition_job(
    pool: &PgPool,
    seed: u64,
    worker_id: &str,
) -> Result<(PostgresStore, CognitionJobEntry)> {
    let store = PostgresStore::from_pool(pool.clone());
    let world_id = WorldId::from_uuid(Uuid::new_v4());
    let manifest = WorldManifest::new(world_id, WorldSeed::new(seed), COGNITION_RULESET_VERSION);
    let person = cognition_initial_person(world_id);
    let person_id = person.organism_id;
    let created = store.create_world(&manifest, None).await?;
    let initial = EngineState::new(manifest);
    let genesis_events =
        initial.plan_configured_genesis(cognition_configuration(), vec![person])?;
    let (running, genesis_batch) =
        initial.commit(EventSequence::new(1), Digest::ZERO, genesis_events)?;
    let genesis_snapshot = Snapshot::new(
        running.clone(),
        genesis_batch.sequence,
        genesis_batch.batch_hash,
    )?;
    let persisted = store
        .commit_transition(
            created.cursor,
            &genesis_batch,
            &genesis_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let selection_events = running.plan_cognition_request(person_id)?;
    let (pending, selection_batch) = running.commit(
        EventSequence::new(2),
        genesis_batch.batch_hash,
        selection_events,
    )?;
    let selection_snapshot = Snapshot::new(
        pending,
        selection_batch.sequence,
        selection_batch.batch_hash,
    )?;
    store
        .commit_transition(
            persisted.cursor,
            &selection_batch,
            &selection_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let entry = store
        .claim_next_cognition_request(worker_id, 60)
        .await?
        .expect("committed cognition selection is claimable");
    Ok((store, entry))
}

async fn cognition_world_before_deadline(
    pool: &PgPool,
    seed: u64,
    worker_id: &str,
) -> Result<(PostgresStore, CognitionJobEntry, EngineState, StoredWorld)> {
    let store = PostgresStore::from_pool(pool.clone());
    let world_id = WorldId::from_uuid(Uuid::new_v4());
    let manifest = WorldManifest::new(world_id, WorldSeed::new(seed), COGNITION_RULESET_VERSION);
    let mut person = cognition_initial_person(world_id);
    let person_id = person.organism_id;
    let regulation = person
        .physiological_regulation
        .as_mut()
        .expect("cognition fixture has regulation");
    regulation.hydration_failure_seconds = 100_000_000;
    regulation.fatigue_failure_seconds = 100_000_000;
    regulation.thermal_failure_millicelsius_seconds = 100_000_000_000;

    let created = store.create_world(&manifest, None).await?;
    let initial = EngineState::new(manifest);
    let genesis_events =
        initial.plan_configured_genesis(cognition_configuration(), vec![person])?;
    let (running, genesis_batch) =
        initial.commit(EventSequence::new(1), Digest::ZERO, genesis_events)?;
    let genesis_snapshot = Snapshot::new(
        running.clone(),
        genesis_batch.sequence,
        genesis_batch.batch_hash,
    )?;
    let persisted = store
        .commit_transition(
            created.cursor,
            &genesis_batch,
            &genesis_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let selection_events = running.plan_cognition_request(person_id)?;
    let (mut state, selection_batch) = running.commit(
        EventSequence::new(2),
        genesis_batch.batch_hash,
        selection_events,
    )?;
    let selection_snapshot = Snapshot::new(
        state.clone(),
        selection_batch.sequence,
        selection_batch.batch_hash,
    )?;
    let mut world = store
        .commit_transition(
            persisted.cursor,
            &selection_batch,
            &selection_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let entry = store
        .claim_next_cognition_request(worker_id, 3_600)
        .await?
        .expect("committed cognition selection is claimable");

    while state.tick().checked_next()? < entry.selection.deadline_tick {
        let next_tick = state.tick().checked_next()?;
        let celestial = CelestialState::new(
            TdbSecondsSinceJ2000::new(i128::from(next_tick.get()) * 300),
            CartesianMillimetres::new(1, 2, 3),
            CartesianMillimetres::new(4, 5, 6),
        );
        let events = state.plan_next_tick_with_celestial(celestial)?;
        let sequence = world.cursor.sequence.checked_next()?;
        let (next, batch) = state.commit(sequence, world.cursor.last_event_hash, events)?;
        let snapshot = Snapshot::new(next.clone(), batch.sequence, batch.batch_hash)?;
        world = store
            .commit_transition(
                world.cursor,
                &batch,
                &snapshot,
                &TransitionEffects::default(),
            )
            .await?;
        state = next;
    }
    assert_eq!(state.tick().checked_next()?, entry.selection.deadline_tick);
    Ok((store, entry, state, world))
}

fn unavailable_cognition_recall(entry: &CognitionJobEntry) -> Result<CognitionRecallRecord> {
    let request = MemoryRecallRequest::from_cognition_selection(&entry.selection)?;
    let outcome = MemoryRecallOutcome::unavailable(&request, RecallUnavailableReason::Disabled)?;
    Ok(CognitionRecallRecord {
        request,
        outcome,
        admitted_memories: Vec::new(),
    })
}

fn route_attempt(
    registry: &CognitionRouteRegistry,
    route_index: usize,
    status: CognitionRouteAttemptStatus,
) -> CognitionRouteAttempt {
    let route = &registry.routes[route_index];
    CognitionRouteAttempt {
        route_index: u16::try_from(route_index).expect("small route registry"),
        provider: route.provider.clone(),
        requested_model: route.requested_model.clone(),
        billing_class: route.billing_class,
        status,
    }
}

#[derive(Clone, Default)]
struct FakeCognitionMemory {
    recall_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentMemory for FakeCognitionMemory {
    async fn retain(
        &self,
        _memory: &MemoryRetain,
    ) -> Result<MemoryRetainReceipt, MemoryAdapterError> {
        Err(MemoryAdapterError::Rejected(
            "worker test never retains".to_owned(),
        ))
    }

    async fn recall(&self, request: &MemoryRecallRequest) -> MemoryRecallOutcome {
        self.recall_calls.fetch_add(1, Ordering::SeqCst);
        MemoryRecallOutcome::unavailable(request, RecallUnavailableReason::Disabled)
            .expect("valid worker recall request")
    }
}

#[derive(Clone)]
struct PersistedBeforeCallModel {
    pool: PgPool,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl CognitionModel for PersistedBeforeCallModel {
    async fn infer(
        &self,
        route: &CognitionModelRoute,
        request: &ModelCognitionRequest,
    ) -> Result<ModelCognitionReceipt, CognitionModelError> {
        let dispatch_state: Option<(String, bool)> = sqlx::query_as(
            r#"
            SELECT dispatch_state, network_dispatched
            FROM cognition_route_attempts
            WHERE request_id = $1
              AND provider_slug = $2
              AND requested_model = $3
            "#,
        )
        .bind(request.request_id)
        .bind(route.provider.as_str())
        .bind(&route.requested_model)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| CognitionModelError::Unavailable(error.to_string()))?;
        if dispatch_state != Some(("dispatched".to_owned(), true)) {
            return Err(CognitionModelError::InvalidResponse(
                "provider was called before durable dispatch".to_owned(),
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ModelCognitionReceipt {
            contract_version: COGNITION_MODEL_CONTRACT_VERSION,
            request_id: request.request_id,
            request_hash: request
                .canonical_hash()
                .map_err(|error| CognitionModelError::InvalidResponse(error.to_string()))?,
            provider: route.provider.clone(),
            requested_model: route.requested_model.clone(),
            resolved_model: route.requested_model.clone(),
            provider_response_id: "fake-free-response-1".to_owned(),
            usage: ModelTokenUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
            },
            billed_micro_usd: 0,
            action_kind: PrimitiveActionKind::Rest,
            provider_response_hash: Digest::sha256(b"fake free cognition response"),
            adapter_version: "postgres-worker-test-v1".to_owned(),
        })
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
    let public_worlds = store.list_public_worlds().await?;
    assert_eq!(public_worlds.len(), 1);
    assert_eq!(public_worlds[0].world_id, manifest.world_id);
    assert_eq!(public_worlds[0].status, WorldStatus::Initializing);
    assert_eq!(
        public_worlds[0].manifest_hash,
        Digest::canonical(&manifest).expect("canonical manifest hash")
    );
    assert_eq!(public_worlds[0].event_hash, Digest::ZERO);
    assert_eq!(public_worlds[0].state_hash, created.cursor.state_hash);
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
    let fast_resumed = resume_world_from_snapshot(&store, manifest.world_id).await?;

    assert_eq!(batches, vec![genesis_batch, tick_batch]);
    assert_eq!(replayed.state, after_tick);
    assert_eq!(
        latest_snapshot, genesis_snapshot,
        "running snapshots are sparse replay caches"
    );
    assert_eq!(loaded_world, persisted);
    assert_eq!(fast_resumed.world, persisted);
    assert_eq!(fast_resumed.state, after_tick);
    assert_eq!(replayed.last_event_hash, persisted.cursor.last_event_hash);
    assert_eq!(replayed.state.state_hash()?, persisted.cursor.state_hash);
    let public_worlds = store.list_public_worlds().await?;
    assert_eq!(public_worlds[0].through_sequence, persisted.cursor.sequence);
    assert_eq!(
        public_worlds[0].event_hash,
        persisted.cursor.last_event_hash
    );
    assert_eq!(public_worlds[0].state_hash, persisted.cursor.state_hash);
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn projection_ranges_are_atomic_complete_and_idempotent(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool);
    let manifest = manifest(101_101);
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
    let tick_events = running.plan_next_tick()?;
    let (after_tick, tick_batch) =
        running.commit(EventSequence::new(2), genesis_batch.batch_hash, tick_events)?;
    let tick_snapshot = Snapshot::new(after_tick, tick_batch.sequence, tick_batch.batch_hash)?;
    store
        .commit_transition(
            persisted.cursor,
            &tick_batch,
            &tick_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let batches = vec![genesis_batch, tick_batch];

    assert_eq!(store.apply_public_timeline_batches(&batches).await?, 2);
    assert_eq!(store.apply_public_organism_batches(&batches).await?, 2);
    assert_eq!(store.apply_public_finding_batches(&batches).await?, 2);
    assert_eq!(
        store.apply_public_world_telemetry_batches(&batches).await?,
        2
    );
    assert_eq!(store.apply_public_timeline_batches(&batches).await?, 0);
    assert_eq!(store.apply_public_organism_batches(&batches).await?, 0);
    assert_eq!(store.apply_public_finding_batches(&batches).await?, 0);
    assert_eq!(
        store.apply_public_world_telemetry_batches(&batches).await?,
        0
    );
    assert_eq!(
        store.public_timeline_cursor(manifest.world_id).await?,
        EventSequence::new(2)
    );
    assert_eq!(
        store.public_organism_cursor(manifest.world_id).await?,
        EventSequence::new(2)
    );
    assert_eq!(
        store.public_finding_cursor(manifest.world_id).await?,
        EventSequence::new(2)
    );
    assert_eq!(
        store
            .list_public_organisms(manifest.world_id, 10)
            .await?
            .len(),
        1
    );
    assert!(
        store
            .list_public_timeline(manifest.world_id, 10)
            .await?
            .len()
            >= 2
    );
    assert!(
        store
            .list_public_findings(manifest.world_id, 10)
            .await?
            .len()
            >= 2
    );
    let telemetry = store
        .public_world_telemetry(manifest.world_id)
        .await?
        .expect("public telemetry");
    assert_eq!(telemetry.through_sequence, EventSequence::new(2));
    assert_eq!(telemetry.committed_batches, 2);
    assert_eq!(
        telemetry.committed_events,
        u64::try_from(
            batches
                .iter()
                .map(|batch| batch.events.len())
                .sum::<usize>()
        )?
    );
    assert!(telemetry.canonical_payload_bytes > 0);
    assert_eq!(telemetry.timeline_lag_batches, 0);
    assert_eq!(telemetry.organism_index_lag_batches, 0);
    assert_eq!(telemetry.findings_lag_batches, 0);
    assert_eq!(telemetry.telemetry_lag_batches, 0);
    assert_eq!(telemetry.living_people, 1);
    assert_eq!(telemetry.living_fauna, 0);
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
async fn provisional_full_earth_resumes_and_projects_exact_input_status(
    pool: PgPool,
) -> Result<()> {
    let store = PostgresStore::from_pool(pool);
    let manifest = manifest(152);
    let configuration = provisional_configuration();
    let reference = configuration
        .provisional_world_composition()
        .expect("provisional reference")
        .clone();
    let initialized = initialize_or_resume_configured_world(
        &store,
        manifest.clone(),
        None,
        configuration,
        vec![provisional_initial_person(manifest.world_id)],
    )
    .await?;
    let advanced = advance_world(&store, &initialized).await?;
    let resumed = resume_world(&store, manifest.world_id).await?;

    assert_eq!(resumed, advanced);
    assert_eq!(resumed.world.status, WorldStatus::Running);
    assert_eq!(resumed.world.cursor.tick, SimTick::new(1));
    let worlds = store.list_public_worlds().await?;
    assert_eq!(worlds.len(), 1);
    assert_eq!(
        worlds[0].input_status,
        Some(PublicWorldInputStatus::ProvisionalNotScientificallyAdmitted)
    );
    assert_eq!(
        worlds[0].composition_id.as_deref(),
        Some("full-earth-breadth-first")
    );
    assert_eq!(worlds[0].composition_version.as_deref(), Some("0.1.0"));
    assert_eq!(worlds[0].composition_hash, Some(reference.content_hash));
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn material_genesis_is_atomic_idempotent_and_replayable(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool);
    let world_id = WorldId::from_uuid(Uuid::new_v4());
    let manifest = WorldManifest::new(
        world_id,
        WorldSeed::new(17_017),
        MATERIAL_RESERVOIR_RULESET_VERSION,
    );
    let mut female = cognition_initial_person(world_id);
    female.organism_id = EntityId::deterministic(world_id, b"postgres-resource-female");
    female.birth_category = BirthCategory::new("female").expect("category");
    let mut male = female.clone();
    male.organism_id = EntityId::deterministic(world_id, b"postgres-resource-male");
    male.birth_category = BirthCategory::new("male").expect("category");
    let material = shared_water_reservoir(world_id, &female);

    let initialized = initialize_or_resume_configured_world_with_materials(
        &store,
        manifest.clone(),
        None,
        cognition_configuration(),
        vec![female.clone(), male.clone()],
        vec![material.clone()],
    )
    .await?;
    let idempotent = initialize_or_resume_configured_world_with_materials(
        &store,
        manifest,
        None,
        cognition_configuration(),
        vec![male, female],
        vec![material],
    )
    .await?;

    assert_eq!(idempotent, initialized);
    assert_eq!(resume_world(&store, world_id).await?, initialized);
    let batches = store
        .load_event_batches(world_id, EventSequence::ZERO)
        .await?;
    assert_eq!(batches.len(), 1, "genesis must remain one atomic append");
    assert_eq!(
        batches[0].event_schema_version,
        MATERIAL_RESERVOIR_EVENT_SCHEMA_VERSION
    );
    assert_eq!(
        batches[0]
            .events
            .iter()
            .filter(|event| matches!(event.event, DomainEvent::MaterialReservoirCommitted { .. }))
            .count(),
        1
    );
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn running_snapshots_are_sparse_and_anchor_fast_resume(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(64_064);
    let mut current = initialize_or_resume_world(
        &store,
        manifest.clone(),
        None,
        vec![initial_person(manifest.world_id)],
    )
    .await?;
    for _ in 0..63 {
        current = advance_world(&store, &current).await?;
    }
    assert_eq!(current.world.cursor.sequence, EventSequence::new(64));

    let rows: Vec<i64> = sqlx::query_scalar(
        "SELECT through_sequence FROM snapshots WHERE world_id = $1 ORDER BY through_sequence",
    )
    .bind(manifest.world_id.as_uuid())
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows, vec![0, 1, 64]);
    assert_eq!(
        resume_world_from_snapshot(&store, manifest.world_id).await?,
        current
    );
    assert_eq!(resume_world(&store, manifest.world_id).await?, current);
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

    let recall_request = MemoryRecallRequest::new(
        manifest.world_id,
        person_id,
        SimTick::new(2),
        SimTick::new(4),
        0,
        "cold gust",
        256,
    )?;
    let recalled = RecalledMemory {
        rank: 0,
        remote_memory_id: "hindsight-memory-1".to_owned(),
        document_id: retain.document_id,
        source_sequence: retain.source_sequence,
        sim_tick: retain.sim_tick,
        ordinal: retain.ordinal,
        text: retain.content.clone(),
        kind: MemoryFactKind::Experience,
        context: retain.context.clone(),
    };
    let recall = MemoryRecallOutcome::available(
        &recall_request,
        "test-hindsight-adapter",
        Digest::sha256(b"test recall response"),
        vec![recalled.clone()],
    )?;
    let admitted = store
        .admit_recall_for_cognition(&recall_request, &recall)
        .await?;
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].document_id, retain.document_id);
    assert_eq!(admitted[0].content, retain.content);

    let forged_recall = MemoryRecallOutcome::available(
        &recall_request,
        "test-hindsight-adapter",
        Digest::sha256(b"forged recall response"),
        vec![RecalledMemory {
            text: "An experience that was never retained.".to_owned(),
            ..recalled
        }],
    )?;
    assert!(
        store
            .admit_recall_for_cognition(&recall_request, &forged_recall)
            .await
            .is_err()
    );

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

#[sqlx::test(migrations = "../../db/migrations")]
async fn supporter_reservations_are_observer_only_paid_moderated_and_birth_matched(
    pool: PgPool,
) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(404);
    store.create_world(&manifest, None).await?;
    let person = initial_person(manifest.world_id);
    let reservation_id = Uuid::new_v4();
    let request = ReservationRequest {
        reservation_id,
        world_id: manifest.world_id,
        supporter_subject: "test-account-subject".to_owned(),
        observer_label: "Ada".to_owned(),
        target: ReservationTarget::Person,
        birth_category: BirthCategory::new("female").expect("valid category"),
    };
    let pending = store.create_reservation(&request).await?;
    assert_eq!(pending.state, ReservationState::PendingPayment);
    assert_eq!(store.create_reservation(&request).await?, pending);
    let mut conflicting_request = request.clone();
    conflicting_request.observer_label = "Different label".to_owned();
    assert!(matches!(
        store.create_reservation(&conflicting_request).await,
        Err(observer_projection::ReservationStoreError::Conflict(_))
    ));

    let birth = CommittedBirth {
        world_id: manifest.world_id,
        event_id: EventId::from_uuid(Uuid::new_v4()),
        event_sequence: EventSequence::new(1),
        tick: SimTick::new(0),
        organism_id: person.organism_id,
        role: OrganismRole::Person,
        species: person.species.clone(),
        birth_category: person.birth_category.clone(),
    };
    assert!(store.match_committed_birth(&birth).await?.is_none());

    let awaiting_review = store
        .record_verified_payment(reservation_id, "stripe_webhook_event_1")
        .await?;
    assert_eq!(awaiting_review.state, ReservationState::PendingModeration);
    assert!(store.match_committed_birth(&birth).await?.is_none());

    let active = store.approve_reservation(reservation_id).await?;
    assert_eq!(active.state, ReservationState::Active);
    let matched = store
        .match_committed_birth(&birth)
        .await?
        .expect("the committed matching birth receives one observer label");
    assert_eq!(matched.state, ReservationState::Matched);
    assert_eq!(
        matched.matched_birth.expect("birth link").event_id,
        birth.event_id
    );
    assert!(store.match_committed_birth(&birth).await?.is_none());

    let pending_expiry_id = Uuid::new_v4();
    let mut pending_expiry_request = request.clone();
    pending_expiry_request.reservation_id = pending_expiry_id;
    pending_expiry_request.observer_label = "Unmatched pending label".to_owned();
    store.create_reservation(&pending_expiry_request).await?;
    let active_expiry_id = Uuid::new_v4();
    let mut active_expiry_request = request.clone();
    active_expiry_request.reservation_id = active_expiry_id;
    active_expiry_request.observer_label = "Unmatched active label".to_owned();
    store.create_reservation(&active_expiry_request).await?;
    store
        .record_verified_payment(active_expiry_id, "stripe_webhook_event_2")
        .await?;
    store.approve_reservation(active_expiry_id).await?;

    let event_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_batches WHERE world_id = $1")
            .bind(manifest.world_id.as_uuid())
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        event_count, 0,
        "observer matching cannot create canonical events"
    );

    let matched_mutation =
        sqlx::query("UPDATE supporter_reservations SET state = 'expired' WHERE id = $1")
            .bind(reservation_id)
            .execute(&pool)
            .await;
    assert!(matched_mutation.is_err());
    let deletion = sqlx::query("DELETE FROM supporter_reservations WHERE id = $1")
        .bind(reservation_id)
        .execute(&pool)
        .await;
    assert!(deletion.is_err());

    assert_eq!(store.expire_world_reservations(manifest.world_id).await?, 2);
    assert_eq!(store.expire_world_reservations(manifest.world_id).await?, 0);
    for reservation_id in [pending_expiry_id, active_expiry_id] {
        let state: String =
            sqlx::query_scalar("SELECT state FROM supporter_reservations WHERE id = $1")
                .bind(reservation_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(state, "expired");
    }
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn verified_stripe_events_are_atomic_append_only_and_idempotent(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(405);
    store.create_world(&manifest, None).await?;
    let reservation_id = Uuid::new_v4();
    store
        .create_reservation(&ReservationRequest {
            reservation_id,
            world_id: manifest.world_id,
            supporter_subject: "apple-subject-fixture".to_owned(),
            observer_label: "Grace".to_owned(),
            target: ReservationTarget::Person,
            birth_category: BirthCategory::new("female").expect("valid category"),
        })
        .await?;
    let checkout = StripeCheckoutSession {
        session_id: "cs_test_durable_fixture".to_owned(),
        checkout_url: "https://checkout.stripe.com/c/pay/cs_test_durable_fixture"
            .parse()
            .expect("Checkout URL"),
    };
    assert_eq!(
        store
            .record_checkout_session(reservation_id, &checkout)
            .await?,
        checkout
    );
    assert_eq!(
        store
            .record_checkout_session(reservation_id, &checkout)
            .await?,
        checkout
    );
    let mut conflicting_checkout = checkout.clone();
    conflicting_checkout.session_id = "cs_test_other_fixture".to_owned();
    assert!(matches!(
        store
            .record_checkout_session(reservation_id, &conflicting_checkout)
            .await,
        Err(StripeCheckoutStoreError::Conflict(_))
    ));

    let payment = VerifiedCheckoutPayment {
        event_id: "evt_atomic_fixture_1".to_owned(),
        event_type: "checkout.session.completed".to_owned(),
        checkout_session_id: "cs_atomic_fixture_1".to_owned(),
        reservation_id,
        amount_minor: 500,
        currency: "usd".to_owned(),
        live_mode: false,
        payload_hash: Digest::sha256(b"first exact raw Stripe body"),
    };
    let event = VerifiedStripeEvent::Paid(payment.clone());
    let competing_store = store.clone();
    let competing_event = event.clone();
    let (first_delivery, concurrent_delivery) = tokio::join!(
        store.record_verified_stripe_event(&event),
        competing_store.record_verified_stripe_event(&competing_event)
    );
    let mut concurrent_dispositions = [first_delivery?, concurrent_delivery?];
    concurrent_dispositions.sort_by_key(|disposition| match disposition {
        StripeWebhookDisposition::PaymentRecorded => 0,
        StripeWebhookDisposition::Duplicate => 1,
        StripeWebhookDisposition::Ignored => 2,
    });
    assert_eq!(
        concurrent_dispositions,
        [
            StripeWebhookDisposition::PaymentRecorded,
            StripeWebhookDisposition::Duplicate
        ]
    );
    assert_eq!(
        store.record_verified_stripe_event(&event).await?,
        StripeWebhookDisposition::Duplicate
    );
    let state: String = sqlx::query_scalar("SELECT state FROM supporter_reservations WHERE id=$1")
        .bind(reservation_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(state, "pending_moderation");

    let mut retried_as_new_event = payment.clone();
    retried_as_new_event.event_id = "evt_atomic_fixture_2".to_owned();
    retried_as_new_event.event_type = "checkout.session.async_payment_succeeded".to_owned();
    retried_as_new_event.payload_hash = Digest::sha256(b"second Stripe event, same checkout");
    assert_eq!(
        store
            .record_verified_stripe_event(&VerifiedStripeEvent::Paid(retried_as_new_event))
            .await?,
        StripeWebhookDisposition::Duplicate
    );

    let mut forged_reuse = payment.clone();
    forged_reuse.payload_hash = Digest::sha256(b"different body under reused event ID");
    assert!(matches!(
        store
            .record_verified_stripe_event(&VerifiedStripeEvent::Paid(forged_reuse))
            .await,
        Err(StripeWebhookStoreError::Conflict(_))
    ));

    let mut unknown = payment;
    unknown.event_id = "evt_unknown_reservation".to_owned();
    unknown.checkout_session_id = "cs_unknown_reservation".to_owned();
    unknown.reservation_id = Uuid::new_v4();
    unknown.payload_hash = Digest::sha256(b"unknown reservation body");
    assert!(matches!(
        store
            .record_verified_stripe_event(&VerifiedStripeEvent::Paid(unknown))
            .await,
        Err(StripeWebhookStoreError::ReservationNotFound(_))
    ));
    let unknown_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stripe_webhook_events WHERE event_id='evt_unknown_reservation'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        unknown_count, 0,
        "failed admission rolls back its ledger row"
    );

    let deletion =
        sqlx::query("DELETE FROM stripe_webhook_events WHERE event_id='evt_atomic_fixture_1'")
            .execute(&pool)
            .await;
    assert!(deletion.is_err());
    let canonical_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM event_batches WHERE world_id=$1")
            .bind(manifest.world_id.as_uuid())
            .fetch_one(&pool)
            .await?;
    assert_eq!(canonical_events, 0, "payments cannot write world history");
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn observer_timeline_is_idempotent_append_only_and_readable(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(505);
    let created = store.create_world(&manifest, None).await?;
    let (_, batch, snapshot) = genesis(&manifest, Vec::new())?;
    store
        .commit_transition(
            created.cursor,
            &batch,
            &snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let uncommitted_manifest = WorldManifest::new(
        WorldId::from_uuid(Uuid::new_v4()),
        WorldSeed::new(506),
        RULESET_VERSION,
    );
    let (_, uncommitted_batch, _) = genesis(&uncommitted_manifest, Vec::new())?;
    assert!(
        store
            .apply_public_timeline_batch(&uncommitted_batch)
            .await
            .is_err()
    );
    assert!(store.apply_public_timeline_batch(&batch).await?);
    assert!(!store.apply_public_timeline_batch(&batch).await?);
    assert_eq!(
        store.public_timeline_cursor(manifest.world_id).await?,
        EventSequence::new(1)
    );
    let items = store.list_public_timeline(manifest.world_id, 50).await?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source_event_id, batch.events[0].event_id);

    let row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM observer_timeline_items WHERE world_id = $1")
            .bind(manifest.world_id.as_uuid())
            .fetch_one(&pool)
            .await?;
    assert_eq!(row_count, 1);
    let mutation =
        sqlx::query("UPDATE observer_timeline_items SET title = title WHERE world_id = $1")
            .bind(manifest.world_id.as_uuid())
            .execute(&pool)
            .await;
    assert!(mutation.is_err());
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn observer_organisms_are_committed_safe_and_append_only(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(507);
    let person = initial_person(manifest.world_id);
    let person_id = person.organism_id;
    let created = store.create_world(&manifest, None).await?;
    let (running, genesis_batch, genesis_snapshot) = genesis(&manifest, vec![person])?;
    let stored = store
        .commit_transition(
            created.cursor,
            &genesis_batch,
            &genesis_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    assert!(store.apply_public_organism_batch(&genesis_batch).await?);
    assert!(!store.apply_public_organism_batch(&genesis_batch).await?);
    let organisms = store.list_public_organisms(manifest.world_id, 10).await?;
    assert_eq!(organisms.len(), 1);
    assert_eq!(organisms[0].organism_id, person_id);
    assert!(organisms[0].ended_event_id.is_none());

    let death_events = running.plan_death(
        person_id,
        DeathCause {
            mechanism: "test_only".to_owned(),
        },
    )?;
    let (after_death, death_batch) = running.commit(
        EventSequence::new(2),
        genesis_batch.batch_hash,
        death_events,
    )?;
    let death_snapshot = Snapshot::new(after_death, death_batch.sequence, death_batch.batch_hash)?;
    store
        .commit_transition(
            stored.cursor,
            &death_batch,
            &death_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    assert!(store.apply_public_organism_batch(&death_batch).await?);
    let organism = store
        .get_public_organism(manifest.world_id, person_id)
        .await?
        .expect("committed life is indexed");
    assert_eq!(
        organism.ended_event_id,
        Some(death_batch.events[0].event_id)
    );
    assert_eq!(
        store.public_organism_cursor(manifest.world_id).await?,
        EventSequence::new(2)
    );
    let mutation = sqlx::query("UPDATE observer_organisms SET role = role WHERE world_id = $1")
        .bind(manifest.world_id.as_uuid())
        .execute(&pool)
        .await;
    assert!(mutation.is_err());
    let deletion = sqlx::query("DELETE FROM observer_organism_endings WHERE world_id = $1")
        .bind(manifest.world_id.as_uuid())
        .execute(&pool)
        .await;
    assert!(deletion.is_err());
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn public_findings_are_committed_versioned_and_non_narrative(pool: PgPool) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let manifest = manifest(508);
    let created = store.create_world(&manifest, None).await?;
    let initial = EngineState::new(manifest.clone());
    let events = initial.plan_configured_genesis(
        provisional_configuration(),
        vec![provisional_initial_person(manifest.world_id)],
    )?;
    let (running, batch) = initial.commit(EventSequence::new(1), Digest::ZERO, events)?;
    let snapshot = Snapshot::new(running.clone(), batch.sequence, batch.batch_hash)?;
    let persisted = store
        .commit_transition(
            created.cursor,
            &batch,
            &snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    assert!(store.apply_public_finding_batch(&batch).await?);
    assert!(!store.apply_public_finding_batch(&batch).await?);
    let organism_id = provisional_initial_person(manifest.world_id).organism_id;
    let from_patch = running
        .organisms()
        .next()
        .and_then(sim_engine::OrganismState::embodied_patch)
        .expect("configured person has an embodied patch");
    let to_patch = world_domain::s2_edge_neighbors(from_patch)?[0];
    let movement_events = running.plan_movement(organism_id, to_patch)?;
    let (after_movement, movement_batch) =
        running.commit(EventSequence::new(2), batch.batch_hash, movement_events)?;
    let movement_snapshot = Snapshot::new(
        after_movement,
        movement_batch.sequence,
        movement_batch.batch_hash,
    )?;
    store
        .commit_transition(
            persisted.cursor,
            &movement_batch,
            &movement_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    assert!(store.apply_public_finding_batch(&movement_batch).await?);
    let findings = store.list_public_findings(manifest.world_id, 20).await?;
    let keys = findings
        .iter()
        .map(|finding| finding.finding_key.as_str())
        .collect::<Vec<_>>();
    assert!(keys.contains(&"world_began"));
    assert!(keys.contains(&"first_person_recorded"));
    assert!(keys.contains(&"people_population_record_1"));
    assert!(keys.contains(&"first_confirmed_relocation"));
    assert!(
        findings
            .iter()
            .all(|finding| !matches!(finding.kind, observer_projection::PublicFindingKind::Streak))
    );
    assert_eq!(
        store.public_finding_cursor(manifest.world_id).await?,
        EventSequence::new(2)
    );
    let mutation = sqlx::query("UPDATE observer_findings SET title = title WHERE world_id = $1")
        .bind(manifest.world_id.as_uuid())
        .execute(&pool)
        .await;
    assert!(mutation.is_err());
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cognition_selection_creates_one_immutable_leased_job_atomically(
    pool: PgPool,
) -> Result<()> {
    let store = PostgresStore::from_pool(pool.clone());
    let world_id = WorldId::from_uuid(Uuid::new_v4());
    let manifest = WorldManifest::new(world_id, WorldSeed::new(509), COGNITION_RULESET_VERSION);
    let person = cognition_initial_person(world_id);
    let person_id = person.organism_id;
    let created = store.create_world(&manifest, None).await?;
    let initial = EngineState::new(manifest.clone());
    let genesis_events =
        initial.plan_configured_genesis(cognition_configuration(), vec![person])?;
    let (running, genesis_batch) =
        initial.commit(EventSequence::new(1), Digest::ZERO, genesis_events)?;
    let genesis_snapshot = Snapshot::new(
        running.clone(),
        genesis_batch.sequence,
        genesis_batch.batch_hash,
    )?;
    let persisted = store
        .commit_transition(
            created.cursor,
            &genesis_batch,
            &genesis_snapshot,
            &TransitionEffects::default(),
        )
        .await?;

    let selection_events = running.plan_cognition_request(person_id)?;
    let (pending, selection_batch) = running.commit(
        EventSequence::new(2),
        genesis_batch.batch_hash,
        selection_events,
    )?;
    let selection_snapshot = Snapshot::new(
        pending,
        selection_batch.sequence,
        selection_batch.batch_hash,
    )?;
    store
        .commit_transition(
            persisted.cursor,
            &selection_batch,
            &selection_snapshot,
            &TransitionEffects::default(),
        )
        .await?;

    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cognition_requests WHERE world_id = $1")
            .bind(world_id.as_uuid())
            .fetch_one(&pool)
            .await?;
    assert_eq!(job_count, 1);

    let first = store
        .claim_next_cognition_request("cognition-worker-a", 60)
        .await?
        .expect("committed selection is claimable");
    assert_eq!(first.selection.world_id, world_id);
    assert_eq!(first.selection.organism_id, person_id);
    assert_eq!(first.source_sequence, selection_batch.sequence);
    assert_eq!(first.source_event_id, selection_batch.events[0].event_id);
    assert_eq!(first.source_event_index, selection_batch.events[0].index);
    assert_eq!(first.claim_count, 1);
    assert!(
        store
            .claim_next_cognition_request("cognition-worker-b", 60)
            .await?
            .is_none(),
        "an active lease excludes a second worker"
    );
    assert!(
        store
            .reschedule_cognition_request("cognition-worker-b", &first, "wrong owner", 1)
            .await
            .is_err()
    );
    store
        .reschedule_cognition_request("cognition-worker-a", &first, "temporary failure", 1)
        .await?;

    let selection_mutation = sqlx::query(
        r#"
        UPDATE cognition_requests
        SET selection = jsonb_set(selection, '{memory_query}', '"forged"'::JSONB)
        WHERE request_id = $1
        "#,
    )
    .bind(first.selection.request_id)
    .execute(&pool)
    .await;
    assert!(selection_mutation.is_err());
    let deletion = sqlx::query("DELETE FROM cognition_requests WHERE request_id = $1")
        .bind(first.selection.request_id)
        .execute(&pool)
        .await;
    assert!(deletion.is_err());

    let batches = store
        .load_event_batches(world_id, EventSequence::ZERO)
        .await?;
    assert_eq!(batches, vec![genesis_batch, selection_batch]);
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cognition_recall_is_recorded_once_and_is_immutable(pool: PgPool) -> Result<()> {
    let (store, entry) = claimed_cognition_job(&pool, 601, "recall-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;

    store
        .record_cognition_recall("recall-worker", &entry, &recall)
        .await?;
    assert_eq!(
        store.load_cognition_recall(&entry).await?,
        Some(recall.clone())
    );
    assert!(
        store
            .record_cognition_recall("recall-worker", &entry, &recall)
            .await
            .is_err(),
        "even an identical second admission must conflict rather than hide a retry"
    );
    let mutation = sqlx::query(
        "UPDATE cognition_recall_outcomes SET recall_outcome = recall_outcome WHERE request_id = $1",
    )
    .bind(entry.selection.request_id)
    .execute(&pool)
    .await;
    assert!(mutation.is_err(), "recorded recall is append-only");
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cognition_dispatch_survives_a_crash_before_the_next_route(pool: PgPool) -> Result<()> {
    let (store, entry) = claimed_cognition_job(&pool, 602, "dispatch-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;
    store
        .record_cognition_recall("dispatch-worker", &entry, &recall)
        .await?;
    let registry = CognitionRouteRegistry::production_default();

    store
        .begin_cognition_route_attempt("dispatch-worker", &entry, 0, &registry.routes[0])
        .await?;
    let persisted_state: (String, bool) = sqlx::query_as(
        "SELECT dispatch_state, network_dispatched FROM cognition_route_attempts WHERE request_id = $1 AND route_index = 0",
    )
    .bind(entry.selection.request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(persisted_state, ("dispatched".to_owned(), true));
    assert!(
        store
            .begin_cognition_route_attempt("dispatch-worker", &entry, 1, &registry.routes[1],)
            .await
            .is_err(),
        "the durable in-flight attempt must be resolved before advancing"
    );

    store
        .abandon_cognition_route_attempt("dispatch-worker", &entry, 0)
        .await?;
    store
        .begin_cognition_route_attempt("dispatch-worker", &entry, 1, &registry.routes[1])
        .await?;
    let attempts = store.list_cognition_route_attempts(&entry).await?;
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        attempts[0].persistence_state,
        CognitionAttemptPersistenceState::Abandoned
    );
    assert_eq!(
        attempts[0]
            .attempt
            .as_ref()
            .expect("abandoned attempt payload")
            .status,
        CognitionRouteAttemptStatus::Unavailable
    );
    assert_eq!(
        attempts[1].persistence_state,
        CognitionAttemptPersistenceState::Dispatched
    );
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn paid_dispatch_requires_a_durable_reservation_and_release_restores_budget(
    pool: PgPool,
) -> Result<()> {
    let (store, entry) = claimed_cognition_job(&pool, 603, "paid-release-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;
    store
        .record_cognition_recall("paid-release-worker", &entry, &recall)
        .await?;
    let paid_route = CognitionModelRoute::openrouter_deepseek_v4_flash();

    assert!(
        store
            .begin_cognition_route_attempt("paid-release-worker", &entry, 0, &paid_route)
            .await
            .is_err(),
        "a paid call cannot be marked dispatched before funds are reserved"
    );
    let authorization = match store
        .reserve_paid_cognition("paid-release-worker", &entry, &paid_route, 25_000)
        .await?
    {
        PaidCognitionReservationDecision::Authorized(authorization) => authorization,
        PaidCognitionReservationDecision::DeniedHardStop => {
            panic!("a fresh monthly cost account has budget")
        }
    };
    let reserved: i64 = sqlx::query_scalar(
        "SELECT reserved_micro_usd FROM cognition_cost_accounts WHERE billing_month = $1",
    )
    .bind(authorization.billing_month)
    .fetch_one(&pool)
    .await?;
    assert_eq!(reserved, 25_000);

    store
        .release_paid_cognition("paid-release-worker", &entry, &authorization)
        .await?;
    let account: (i64, i64) = sqlx::query_as(
        "SELECT reserved_micro_usd, spent_micro_usd FROM cognition_cost_accounts WHERE billing_month = $1",
    )
    .bind(authorization.billing_month)
    .fetch_one(&pool)
    .await?;
    assert_eq!(account, (0, 0));
    let reservation_status: String =
        sqlx::query_scalar("SELECT status FROM cognition_cost_reservations WHERE request_id = $1")
            .bind(entry.selection.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(reservation_status, "released");
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn indeterminate_paid_dispatch_keeps_budget_reserved(pool: PgPool) -> Result<()> {
    let (store, entry) = claimed_cognition_job(&pool, 604, "paid-crash-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;
    store
        .record_cognition_recall("paid-crash-worker", &entry, &recall)
        .await?;
    let paid_route = CognitionModelRoute::openrouter_deepseek_v4_flash();
    let authorization = match store
        .reserve_paid_cognition("paid-crash-worker", &entry, &paid_route, 30_000)
        .await?
    {
        PaidCognitionReservationDecision::Authorized(authorization) => authorization,
        PaidCognitionReservationDecision::DeniedHardStop => {
            panic!("a fresh monthly cost account has budget")
        }
    };
    store
        .begin_cognition_route_attempt("paid-crash-worker", &entry, 0, &paid_route)
        .await?;
    assert!(
        store
            .release_paid_cognition("paid-crash-worker", &entry, &authorization)
            .await
            .is_err(),
        "a possibly billed call cannot release its reservation"
    );
    store
        .mark_paid_cognition_indeterminate("paid-crash-worker", &entry, &authorization)
        .await?;

    let reservation: (String, Option<i64>) = sqlx::query_as(
        "SELECT status, actual_micro_usd FROM cognition_cost_reservations WHERE request_id = $1",
    )
    .bind(entry.selection.request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(reservation, ("indeterminate".to_owned(), None));
    let reserved: i64 = sqlx::query_scalar(
        "SELECT reserved_micro_usd FROM cognition_cost_accounts WHERE billing_month = $1",
    )
    .bind(authorization.billing_month)
    .fetch_one(&pool)
    .await?;
    assert_eq!(reserved, 30_000);
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cognition_result_must_equal_the_durable_attempt_prefix(pool: PgPool) -> Result<()> {
    let (store, entry) = claimed_cognition_job(&pool, 605, "result-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;
    store
        .record_cognition_recall("result-worker", &entry, &recall)
        .await?;
    let request =
        ModelCognitionRequest::from_selection(&entry.selection, recall.admitted_memories.clone())?;
    let registry = CognitionRouteRegistry::production_default();
    let first = route_attempt(
        &registry,
        0,
        CognitionRouteAttemptStatus::SkippedUnconfigured,
    );
    let second = route_attempt(&registry, 1, CognitionRouteAttemptStatus::SkippedCooldown);
    store
        .record_cognition_route_skip("result-worker", &entry, &first)
        .await?;
    store
        .record_cognition_route_skip("result-worker", &entry, &second)
        .await?;

    let result = ModelCognitionLadderResult {
        contract_version: COGNITION_MODEL_CONTRACT_VERSION,
        request_id: entry.selection.request_id,
        route_policy_version: registry.policy_version,
        route_registry_hash: registry.canonical_hash(CognitionRoutePurpose::ProductionWorld)?,
        attempts: vec![first.clone(), second.clone()],
        receipt: None,
    };
    let mut forged = result.clone();
    forged.attempts[1].status = CognitionRouteAttemptStatus::SkippedDisabled;
    assert!(
        store
            .complete_cognition_request(
                "result-worker",
                &entry,
                &registry,
                CognitionRoutePurpose::ProductionWorld,
                &request,
                &forged,
            )
            .await
            .is_err(),
        "a valid-looking result cannot differ from durable attempt history"
    );
    store
        .complete_cognition_request(
            "result-worker",
            &entry,
            &registry,
            CognitionRoutePurpose::ProductionWorld,
            &request,
            &result,
        )
        .await?;
    let persisted: serde_json::Value =
        sqlx::query_scalar("SELECT result_payload FROM cognition_results WHERE request_id = $1")
            .bind(entry.selection.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(persisted, serde_json::to_value(result)?);
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cognition_deadline_latch_is_retry_stable_and_consumed_only_by_exact_history(
    pool: PgPool,
) -> Result<()> {
    let (store, entry, state, world) =
        cognition_world_before_deadline(&pool, 606, "latch-worker").await?;
    let target_sequence = world.cursor.sequence.checked_next()?;
    let target_tick = world.cursor.tick.checked_next()?;

    let first = store
        .latch_due_cognition_inputs(entry.selection.world_id, target_sequence, target_tick)
        .await?;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].request_id, entry.selection.request_id);
    assert!(matches!(
        first[0].outcome,
        CognitionInputOutcome::Unavailable {
            reason: CognitionUnavailableReason::DeadlineNoResult
        }
    ));
    let first_bytes = serde_json::to_vec(&first)?;

    // This second call represents a process crash after the latch transaction
    // committed but before the world transition did. It must not rebuild input.
    let retry = store
        .latch_due_cognition_inputs(entry.selection.world_id, target_sequence, target_tick)
        .await?;
    assert_eq!(serde_json::to_vec(&retry)?, first_bytes);
    let latch_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cognition_deadline_latches WHERE request_id = $1")
            .bind(entry.selection.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(latch_count, 1);

    let celestial = CelestialState::new(
        TdbSecondsSinceJ2000::new(i128::from(target_tick.get()) * 300),
        CartesianMillimetres::new(1, 2, 3),
        CartesianMillimetres::new(4, 5, 6),
    );
    let exact_events = state.plan_next_tick_with_celestial_and_cognition(celestial, &first)?;
    let (next, exact_batch) = state.commit(
        target_sequence,
        world.cursor.last_event_hash,
        exact_events.clone(),
    )?;
    let exact_snapshot = Snapshot::new(next.clone(), exact_batch.sequence, exact_batch.batch_hash)?;

    let omitted_events = exact_events
        .iter()
        .filter(|event| !matches!(event, DomainEvent::CognitionInputRecorded { .. }))
        .cloned()
        .collect::<Vec<_>>();
    let omitted_batch = EventBatch::new(
        exact_batch.event_schema_version,
        exact_batch.world_id,
        exact_batch.sequence,
        exact_batch.tick,
        exact_batch.ruleset_version,
        exact_batch.previous_hash,
        omitted_events,
        next.state_hash()?,
    )?;
    let omitted_snapshot = Snapshot::new(
        next.clone(),
        omitted_batch.sequence,
        omitted_batch.batch_hash,
    )?;
    assert!(
        store
            .commit_transition(
                world.cursor,
                &omitted_batch,
                &omitted_snapshot,
                &TransitionEffects::default(),
            )
            .await
            .is_err(),
        "a due transition cannot omit its immutable cognition latch"
    );

    let mut forged_input = first[0].clone();
    forged_input.outcome = CognitionInputOutcome::Unavailable {
        reason: CognitionUnavailableReason::BudgetDenied,
    };
    forged_input.validate_against(&entry.selection)?;
    let forged_events = exact_events
        .iter()
        .map(|event| match event {
            DomainEvent::CognitionInputRecorded { .. } => DomainEvent::CognitionInputRecorded {
                input: forged_input.clone(),
            },
            event => event.clone(),
        })
        .collect::<Vec<_>>();
    let forged_batch = EventBatch::new(
        exact_batch.event_schema_version,
        exact_batch.world_id,
        exact_batch.sequence,
        exact_batch.tick,
        exact_batch.ruleset_version,
        exact_batch.previous_hash,
        forged_events,
        next.state_hash()?,
    )?;
    let forged_snapshot =
        Snapshot::new(next.clone(), forged_batch.sequence, forged_batch.batch_hash)?;
    assert!(
        store
            .commit_transition(
                world.cursor,
                &forged_batch,
                &forged_snapshot,
                &TransitionEffects::default(),
            )
            .await
            .is_err(),
        "a valid but different cognition input cannot replace the latch"
    );

    store
        .commit_transition(
            world.cursor,
            &exact_batch,
            &exact_snapshot,
            &TransitionEffects::default(),
        )
        .await?;
    let consumption: (i64, i64, Vec<u8>) = sqlx::query_as(
        "SELECT source_sequence, source_event_index, latch_checksum FROM cognition_latch_consumptions WHERE request_id = $1",
    )
    .bind(entry.selection.request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        u64::try_from(consumption.0).expect("positive sequence"),
        target_sequence.get()
    );
    let input_record = exact_batch
        .events
        .iter()
        .find(|record| matches!(record.event, DomainEvent::CognitionInputRecorded { .. }))
        .expect("exact batch records cognition input");
    assert_eq!(
        u32::try_from(consumption.1).expect("event index"),
        input_record.index
    );
    assert_eq!(consumption.2, first[0].canonical_hash()?.as_bytes());
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn late_paid_cognition_is_audited_and_billed_but_cannot_replace_the_latch(
    pool: PgPool,
) -> Result<()> {
    let (store, entry, _state, world) =
        cognition_world_before_deadline(&pool, 608, "late-paid-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;
    store
        .record_cognition_recall("late-paid-worker", &entry, &recall)
        .await?;
    let request =
        ModelCognitionRequest::from_selection(&entry.selection, recall.admitted_memories.clone())?;
    let registry = CognitionRouteRegistry::production_default();
    let paid_index = registry
        .routes
        .len()
        .checked_sub(1)
        .expect("production registry has a paid terminal route");
    for route_index in 0..paid_index {
        let skipped = route_attempt(
            &registry,
            route_index,
            CognitionRouteAttemptStatus::SkippedUnconfigured,
        );
        store
            .record_cognition_route_skip("late-paid-worker", &entry, &skipped)
            .await?;
    }
    let paid_route = &registry.routes[paid_index];
    let authorization = match store
        .reserve_paid_cognition("late-paid-worker", &entry, paid_route, 25_000)
        .await?
    {
        PaidCognitionReservationDecision::Authorized(authorization) => authorization,
        PaidCognitionReservationDecision::DeniedHardStop => {
            panic!("a fresh monthly cost account has budget")
        }
    };
    store
        .begin_cognition_route_attempt(
            "late-paid-worker",
            &entry,
            u16::try_from(paid_index)?,
            paid_route,
        )
        .await?;

    let target_sequence = world.cursor.sequence.checked_next()?;
    let target_tick = world.cursor.tick.checked_next()?;
    let latched = store
        .latch_due_cognition_inputs(entry.selection.world_id, target_sequence, target_tick)
        .await?;
    assert!(matches!(
        latched.as_slice(),
        [world_domain::CognitionDeadlineInput {
            outcome: CognitionInputOutcome::Unavailable {
                reason: CognitionUnavailableReason::DeadlineNoResult
            },
            ..
        }]
    ));
    assert!(store.cognition_deadline_is_latched(&entry).await?);

    let receipt = ModelCognitionReceipt {
        contract_version: COGNITION_MODEL_CONTRACT_VERSION,
        request_id: request.request_id,
        request_hash: request.canonical_hash()?,
        provider: paid_route.provider.clone(),
        requested_model: paid_route.requested_model.clone(),
        resolved_model: paid_route.requested_model.clone(),
        provider_response_id: "late-paid-response-1".to_owned(),
        usage: ModelTokenUsage {
            prompt_tokens: 10,
            completion_tokens: 1,
        },
        billed_micro_usd: 20_000,
        action_kind: PrimitiveActionKind::Rest,
        provider_response_hash: Digest::sha256(b"late paid cognition response"),
        adapter_version: "postgres-late-response-test-v1".to_owned(),
    };
    let succeeded = route_attempt(
        &registry,
        paid_index,
        CognitionRouteAttemptStatus::Succeeded,
    );
    store
        .finish_cognition_route_attempt(
            "late-paid-worker",
            &entry,
            &request,
            &succeeded,
            Some(&receipt),
        )
        .await?;
    store
        .settle_paid_cognition("late-paid-worker", &entry, &authorization, &receipt)
        .await?;

    let attempts = store.list_cognition_route_attempts(&entry).await?;
    let result = ModelCognitionLadderResult {
        contract_version: COGNITION_MODEL_CONTRACT_VERSION,
        request_id: request.request_id,
        route_policy_version: registry.policy_version,
        route_registry_hash: registry.canonical_hash(CognitionRoutePurpose::ProductionWorld)?,
        attempts: attempts
            .iter()
            .map(|record| record.attempt.clone().expect("terminal attempt"))
            .collect(),
        receipt: Some(receipt),
    };
    assert!(
        store
            .complete_cognition_request(
                "late-paid-worker",
                &entry,
                &registry,
                CognitionRoutePurpose::ProductionWorld,
                &request,
                &result,
            )
            .await
            .is_err(),
        "a late provider response must never replace immutable local fallback"
    );
    let retry = store
        .latch_due_cognition_inputs(entry.selection.world_id, target_sequence, target_tick)
        .await?;
    assert_eq!(serde_json::to_vec(&retry)?, serde_json::to_vec(&latched)?);
    let result_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cognition_results WHERE request_id = $1")
            .bind(entry.selection.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(result_count, 0);
    let reservation: (String, Option<i64>) = sqlx::query_as(
        "SELECT status, actual_micro_usd FROM cognition_cost_reservations WHERE request_id = $1",
    )
    .bind(entry.selection.request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(reservation, ("settled".to_owned(), Some(20_000)));
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn latched_paid_dispatch_is_recovered_without_a_second_network_call(
    pool: PgPool,
) -> Result<()> {
    let (store, entry, _state, world) =
        cognition_world_before_deadline(&pool, 609, "crashed-paid-worker").await?;
    let recall = unavailable_cognition_recall(&entry)?;
    store
        .record_cognition_recall("crashed-paid-worker", &entry, &recall)
        .await?;
    let registry = CognitionRouteRegistry::production_default();
    let paid_index = registry
        .routes
        .len()
        .checked_sub(1)
        .expect("production registry has a paid terminal route");
    for route_index in 0..paid_index {
        store
            .record_cognition_route_skip(
                "crashed-paid-worker",
                &entry,
                &route_attempt(
                    &registry,
                    route_index,
                    CognitionRouteAttemptStatus::SkippedUnconfigured,
                ),
            )
            .await?;
    }
    let paid_route = &registry.routes[paid_index];
    let authorization = match store
        .reserve_paid_cognition("crashed-paid-worker", &entry, paid_route, 25_000)
        .await?
    {
        PaidCognitionReservationDecision::Authorized(authorization) => authorization,
        PaidCognitionReservationDecision::DeniedHardStop => {
            panic!("a fresh monthly cost account has budget")
        }
    };
    store
        .begin_cognition_route_attempt(
            "crashed-paid-worker",
            &entry,
            u16::try_from(paid_index)?,
            paid_route,
        )
        .await?;
    store
        .latch_due_cognition_inputs(
            entry.selection.world_id,
            world.cursor.sequence.checked_next()?,
            world.cursor.tick.checked_next()?,
        )
        .await?;
    sqlx::query(
        "UPDATE cognition_requests SET claimed_at = NOW() - INTERVAL '2 hours' WHERE request_id = $1",
    )
    .bind(entry.selection.request_id)
    .execute(&pool)
    .await?;

    let memory = FakeCognitionMemory::default();
    let adapters = BTreeMap::<CognitionProviderId, Arc<dyn CognitionModel>>::new();
    let step = process_next_cognition_job(
        &store,
        &memory,
        &adapters,
        "recovery-worker",
        1,
        &CognitionWorkerConfiguration::production(false),
    )
    .await?;
    assert_eq!(
        step,
        CognitionWorkerStep::DeadlineElapsed {
            request_id: entry.selection.request_id,
        }
    );
    assert_eq!(memory.recall_calls.load(Ordering::SeqCst), 0);
    let attempt_state: String = sqlx::query_scalar(
        "SELECT dispatch_state FROM cognition_route_attempts WHERE request_id = $1 AND route_index = $2",
    )
    .bind(entry.selection.request_id)
    .bind(i32::try_from(paid_index)?)
    .fetch_one(&pool)
    .await?;
    assert_eq!(attempt_state, "abandoned");
    let reservation_status: String =
        sqlx::query_scalar("SELECT status FROM cognition_cost_reservations WHERE request_id = $1")
            .bind(authorization.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(reservation_status, "indeterminate");
    assert_eq!(
        process_next_cognition_job(
            &store,
            &memory,
            &adapters,
            "recovery-worker",
            1,
            &CognitionWorkerConfiguration::production(false),
        )
        .await?,
        CognitionWorkerStep::Idle
    );
    Ok(())
}

#[sqlx::test(migrations = "../../db/migrations")]
async fn cognition_worker_persists_free_route_success_and_retries_idempotently(
    pool: PgPool,
) -> Result<()> {
    let (store, entry) = claimed_cognition_job(&pool, 607, "fixture-worker").await?;
    sqlx::query(
        r#"
        UPDATE cognition_requests
        SET claimed_by = NULL, claimed_at = NULL, available_at = NOW()
        WHERE request_id = $1
        "#,
    )
    .bind(entry.selection.request_id)
    .execute(&pool)
    .await?;

    let memory = FakeCognitionMemory::default();
    let model_calls = Arc::new(AtomicUsize::new(0));
    let model = PersistedBeforeCallModel {
        pool: pool.clone(),
        calls: Arc::clone(&model_calls),
    };
    let mut adapters = BTreeMap::<CognitionProviderId, Arc<dyn CognitionModel>>::new();
    adapters.insert(CognitionProviderId::groq(), Arc::new(model));
    let configuration = CognitionWorkerConfiguration::production(false);

    let step = process_next_cognition_job(
        &store,
        &memory,
        &adapters,
        "integration-worker",
        60,
        &configuration,
    )
    .await?;
    assert_eq!(
        step,
        CognitionWorkerStep::Completed {
            request_id: entry.selection.request_id,
            used_model: true,
        }
    );
    assert_eq!(memory.recall_calls.load(Ordering::SeqCst), 1);
    assert_eq!(model_calls.load(Ordering::SeqCst), 1);

    let attempts: Vec<(i32, String, String, bool, Option<String>)> = sqlx::query_as(
        r#"
        SELECT route_index, provider_slug, dispatch_state, network_dispatched, normalized_status
        FROM cognition_route_attempts
        WHERE request_id = $1
        ORDER BY route_index ASC
        "#,
    )
    .bind(entry.selection.request_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(attempts.len(), 3);
    assert_eq!(
        attempts[0],
        (
            0,
            "cloudflare_workers_ai".to_owned(),
            "skipped".to_owned(),
            false,
            Some("skipped_unconfigured".to_owned()),
        )
    );
    assert_eq!(
        attempts[1],
        (
            1,
            "cloudflare_workers_ai".to_owned(),
            "skipped".to_owned(),
            false,
            Some("skipped_unconfigured".to_owned()),
        )
    );
    assert_eq!(
        attempts[2],
        (
            2,
            "groq".to_owned(),
            "completed".to_owned(),
            true,
            Some("succeeded".to_owned()),
        )
    );
    let result_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cognition_results WHERE request_id = $1")
            .bind(entry.selection.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(result_count, 1);
    let paid_dispatches: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM cognition_route_attempts
        WHERE request_id = $1
          AND billing_class = 'paid_approved'
          AND network_dispatched
        "#,
    )
    .bind(entry.selection.request_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(paid_dispatches, 0);

    let retry = process_next_cognition_job(
        &store,
        &memory,
        &adapters,
        "integration-worker",
        60,
        &configuration,
    )
    .await?;
    assert_eq!(retry, CognitionWorkerStep::Idle);
    assert_eq!(memory.recall_calls.load(Ordering::SeqCst), 1);
    assert_eq!(model_calls.load(Ordering::SeqCst), 1);
    let final_attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cognition_route_attempts WHERE request_id = $1")
            .bind(entry.selection.request_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(final_attempt_count, 3);
    Ok(())
}
