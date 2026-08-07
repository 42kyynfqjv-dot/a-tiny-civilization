use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::Command as ProcessCommand,
    time::Duration,
};

use anyhow::{Context, Result};
use application::{
    AgentMemory, FoundationStore, MemoryOutboxStore, ServiceHeartbeat, WorldRuntimeError,
    WorldSession, WorldStore, advance_world, advance_world_with_celestial,
    initialize_or_resume_configured_world, initialize_or_resume_world, resume_world,
};
use clap::{Parser, Subcommand};
use hindsight_adapter::HindsightMemory;
use postgres_store::PostgresStore;
use serde::Deserialize;
use serde_json::json;
use sim_engine::{CELESTIAL_DRIVER_RULESET_VERSION, InitialOrganism, RULESET_VERSION};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
use world_data::{
    DataLayerKind, FaunaRangeCandidateSet, FaunaSeededSelection, ProvisionalLandOriginSelection,
};
use world_data_filesystem::{
    load_provisional_world_composition, verify_provisional_world_artifacts,
};
use world_domain::{
    BirthCategory, CapacityExhaustionPolicy, CelestialState, EntityId, OrganismRole,
    PartitionedExecution, PersonRepresentation, S2CellId, SchedulerKind, SpeciesIdentity,
    TdbSecondsSinceJ2000, WorldConfiguration, WorldId, WorldManifest, WorldSeed, WorldStatus,
};

#[derive(Debug, Parser)]
#[command(version, about = "A Tiny Civilization simulation runner")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,

    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 5)]
    database_max_connections: u32,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Verify and advance all explicitly initialized running worlds.
    Serve {
        #[arg(long, env = "RUNNER_TICK_MILLISECONDS", default_value_t = 1_000)]
        tick_milliseconds: u64,

        #[arg(long, env = "RUNNER_HEARTBEAT_SECONDS", default_value_t = 10)]
        heartbeat_seconds: u64,
    },
    /// Create or resume a clearly non-production PostgreSQL proof world.
    InitProof {
        #[arg(long)]
        world_id: WorldId,

        #[arg(long)]
        seed: u64,

        #[arg(long)]
        predecessor_world_id: Option<WorldId>,
    },
    /// Verify all breadth-first inputs, then create or resume a provisional full-Earth world.
    InitProvisionalFullEarth {
        #[arg(long)]
        world_id: WorldId,

        #[arg(long)]
        seed: u64,

        /// Canonical schema-v1 provisional composition manifest.
        #[arg(
            long,
            default_value = "data/provisional/full-earth-breadth-first-0.1.0.json"
        )]
        composition: PathBuf,

        /// Root beneath which every content-addressed composition artifact is stored.
        #[arg(long, default_value = ".")]
        artifact_root: PathBuf,

        /// Exact lowercase 16-hex-character S2 cell at the configured embodied-patch level.
        /// Explicit integration-only starting patch. Mutually exclusive with a
        /// seed-derived `--provisional-land-origin-selection`.
        #[arg(long)]
        initial_patch: Option<S2CellId>,

        /// Canonical source-auditable origin derived from the world seed and the
        /// composition's Natural Earth land-reference root.
        #[arg(long)]
        provisional_land_origin_selection: Option<PathBuf>,

        /// Canonical point-scoped modeled-range candidates. Must be supplied with
        /// `--fauna-seeded-selection` and `--fauna-individuals-per-selected-species`.
        #[arg(long)]
        fauna_range_candidates: Option<PathBuf>,

        /// Canonical seed-derived subset of `--fauna-range-candidates`.
        #[arg(long)]
        fauna_seeded_selection: Option<PathBuf>,

        /// Explicit provisional count to initialize for every selected species.
        /// This is not inferred from modeled-range evidence or treated as abundance.
        #[arg(long)]
        fauna_individuals_per_selected_species: Option<u32>,

        #[arg(long)]
        predecessor_world_id: Option<WorldId>,

        #[arg(long, default_value_t = 300)]
        tick_duration_seconds: u32,

        #[arg(long, default_value_t = 10_000)]
        max_events_per_partition_transition: u32,

        /// Immutable causal ruleset for this new or resumed provisional world.
        /// Ruleset three requires the pinned DE441 source driver at every tick.
        #[arg(long, default_value_t = RULESET_VERSION)]
        ruleset_version: u32,
    },
    /// Replay one stored world from genesis and verify its snapshot, cursor, and hashes.
    VerifyWorld {
        #[arg(long)]
        world_id: WorldId,
    },
    /// Deliver committed subjective-memory records without blocking simulation ticks.
    MemoryWorker {
        #[arg(long, env = "HINDSIGHT_BASE_URL")]
        hindsight_base_url: String,

        #[arg(long, env = "HINDSIGHT_API_KEY", hide_env_values = true)]
        hindsight_api_key: Option<String>,

        #[arg(long, env = "MEMORY_WORKER_ID", default_value = "local-memory-worker")]
        worker_id: String,

        #[arg(long, env = "MEMORY_POLL_MILLISECONDS", default_value_t = 500)]
        poll_milliseconds: u64,

        #[arg(long, env = "MEMORY_CLAIM_LEASE_SECONDS", default_value_t = 60)]
        claim_lease_seconds: u32,

        #[arg(long, env = "HINDSIGHT_REQUEST_TIMEOUT_SECONDS", default_value_t = 15)]
        request_timeout_seconds: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let cli = Cli::parse();
    let store = PostgresStore::connect(&cli.database_url, cli.database_max_connections)
        .await
        .context("connect runner to PostgreSQL")?;

    match cli.command.unwrap_or(Command::Serve {
        tick_milliseconds: 1_000,
        heartbeat_seconds: 10,
    }) {
        Command::Serve {
            tick_milliseconds,
            heartbeat_seconds,
        } => serve(&store, tick_milliseconds, heartbeat_seconds).await,
        Command::InitProof {
            world_id,
            seed,
            predecessor_world_id,
        } => init_proof_world(&store, world_id, seed, predecessor_world_id).await,
        Command::InitProvisionalFullEarth {
            world_id,
            seed,
            composition,
            artifact_root,
            initial_patch,
            provisional_land_origin_selection,
            fauna_range_candidates,
            fauna_seeded_selection,
            fauna_individuals_per_selected_species,
            predecessor_world_id,
            tick_duration_seconds,
            max_events_per_partition_transition,
            ruleset_version,
        } => {
            init_provisional_full_earth_world(
                &store,
                world_id,
                seed,
                &composition,
                &artifact_root,
                initial_patch,
                provisional_land_origin_selection.as_deref(),
                fauna_range_candidates.as_deref(),
                fauna_seeded_selection.as_deref(),
                fauna_individuals_per_selected_species,
                predecessor_world_id,
                tick_duration_seconds,
                max_events_per_partition_transition,
                ruleset_version,
            )
            .await
        }
        Command::VerifyWorld { world_id } => verify_world(&store, world_id).await,
        Command::MemoryWorker {
            hindsight_base_url,
            hindsight_api_key,
            worker_id,
            poll_milliseconds,
            claim_lease_seconds,
            request_timeout_seconds,
        } => {
            let memory = HindsightMemory::new(
                &hindsight_base_url,
                hindsight_api_key,
                Duration::from_secs(request_timeout_seconds.max(1)),
            )
            .context("configure Hindsight memory adapter")?;
            serve_memory_worker(
                &store,
                &memory,
                &worker_id,
                poll_milliseconds,
                claim_lease_seconds,
            )
            .await
        }
    }
}

async fn verify_world(store: &PostgresStore, world_id: WorldId) -> Result<()> {
    // The verifier reads a consistent event history only when no writer commits
    // between its independently retrieved snapshot and tail. Retrying a short,
    // bounded number of times makes the operator command usable against a live
    // runner without weakening its final integrity check.
    for attempt in 1..=3 {
        match resume_world(store, world_id).await {
            Ok(session) => return print_verified_world(world_id, session),
            Err(error) => {
                if attempt == 3 {
                    return Err(error).context("replay and verify stored world after 3 attempts");
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    unreachable!("bounded verification loop returns on success or final error")
}

fn print_verified_world(world_id: WorldId, session: WorldSession) -> Result<()> {
    println!(
        "verified world {} through sequence {} at tick {}: {:?}",
        world_id, session.world.cursor.sequence, session.world.cursor.tick, session.world.status
    );
    println!("event head: {}", session.world.cursor.last_event_hash);
    println!("state hash: {}", session.world.cursor.state_hash);
    println!("genesis replay == snapshot + tail == committed cursor");
    Ok(())
}

async fn serve(
    store: &PostgresStore,
    tick_milliseconds: u64,
    heartbeat_seconds: u64,
) -> Result<()> {
    let instance_id = Uuid::new_v4();
    let heartbeat = ServiceHeartbeat {
        service_name: "simulation-runner".to_owned(),
        instance_id,
        metadata: json!({
            "ruleset_version": RULESET_VERSION,
            "runner_version": env!("CARGO_PKG_VERSION"),
            "mode": "deterministic-ticks",
        }),
    };
    let mut heartbeat_interval =
        tokio::time::interval(Duration::from_secs(heartbeat_seconds.max(1)));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut tick_interval = tokio::time::interval(Duration::from_millis(tick_milliseconds.max(1)));
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut sessions = BTreeMap::<WorldId, WorldSession>::new();

    tracing::info!(
        %instance_id,
        ruleset_version = RULESET_VERSION,
        tick_milliseconds = tick_milliseconds.max(1),
        "runner started"
    );

    loop {
        tokio::select! {
            _ = tick_interval.tick() => {
                if let Err(error) = advance_running_worlds(store, &mut sessions).await {
                    if error.is_retryable() {
                        tracing::warn!(%error, "world advancement unavailable; will reload and retry");
                        sessions.clear();
                    } else {
                        return Err(error).context("advance deterministic worlds");
                    }
                }
            }
            _ = heartbeat_interval.tick() => {
                if let Err(error) = store.record_heartbeat(&heartbeat).await {
                    tracing::warn!(%error, "runner heartbeat failed; will retry without stopping");
                }
            }
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(%error, "failed to listen for shutdown signal");
                }
                tracing::info!("runner stopping");
                break;
            }
        }
    }

    Ok(())
}

async fn advance_running_worlds(
    store: &PostgresStore,
    sessions: &mut BTreeMap<WorldId, WorldSession>,
) -> Result<(), WorldRuntimeError> {
    let running_ids = store.list_running_world_ids().await?;
    let running_set = running_ids.iter().copied().collect::<BTreeSet<_>>();
    sessions.retain(|world_id, _| running_set.contains(world_id));

    for world_id in running_ids {
        let current = match sessions.remove(&world_id) {
            Some(session) => session,
            None => {
                let session = resume_world(store, world_id).await?;
                tracing::info!(
                    %world_id,
                    sequence = %session.world.cursor.sequence,
                    tick = %session.world.cursor.tick,
                    "verified world history before resume"
                );
                session
            }
        };
        let next = if current.state.ruleset_version() >= CELESTIAL_DRIVER_RULESET_VERSION {
            let celestial = evaluate_pinned_de441(&current)
                .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
            advance_world_with_celestial(store, &current, celestial).await?
        } else {
            advance_world(store, &current).await?
        };
        tracing::info!(
            %world_id,
            sequence = %next.world.cursor.sequence,
            tick = %next.world.cursor.tick,
            status = ?next.world.status,
            state_hash = %next.world.cursor.state_hash,
            "committed deterministic transition"
        );
        if next.world.status == WorldStatus::Running {
            sessions.insert(world_id, next);
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct JplEpochInspection {
    fixed_scale_boundary: CelestialState,
}

/// Invoke the project-owned, source-verified DE441 evaluator. Its JSON result is
/// immediately committed by the caller, so replay never starts this process.
fn evaluate_pinned_de441(session: &WorldSession) -> Result<CelestialState> {
    let configuration = session
        .state
        .configuration()
        .context("celestial ruleset requires a tick-zero world configuration")?;
    let next_tick = session
        .state
        .tick()
        .checked_next()
        .context("celestial tick overflow")?;
    let tdb_seconds = i128::from(next_tick.get())
        .checked_mul(i128::from(configuration.tick_duration_seconds))
        .context("celestial epoch overflow")?;
    let tdb_seconds =
        i64::try_from(tdb_seconds).context("celestial epoch exceeds DE441 CLI range")?;
    let tdb_seconds_text = tdb_seconds.to_string();
    let output = ProcessCommand::new("/app/civilization-data")
        .args([
            "inspect",
            "jpl-de441-epoch",
            "--input-directory",
            "/runtime/data/source-cache/jpl-de441",
            "--tdb-seconds-from-j2000",
            &tdb_seconds_text,
        ])
        .output()
        .context("evaluate pinned DE441 source")?;
    if !output.status.success() {
        anyhow::bail!(
            "pinned DE441 evaluator failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let inspection: JplEpochInspection =
        serde_json::from_slice(&output.stdout).context("decode pinned DE441 evaluator output")?;
    if inspection.fixed_scale_boundary.tdb_seconds_since_j2000()
        != TdbSecondsSinceJ2000::new(i128::from(tdb_seconds))
    {
        anyhow::bail!("pinned DE441 evaluator returned a state for the wrong TDB epoch");
    }
    Ok(inspection.fixed_scale_boundary)
}

async fn init_proof_world(
    store: &PostgresStore,
    world_id: WorldId,
    seed: u64,
    predecessor_world_id: Option<WorldId>,
) -> Result<()> {
    let mut manifest = WorldManifest::new(world_id, WorldSeed::new(seed), RULESET_VERSION);
    manifest.scientific_datasets.insert(
        "gbif_taxon_2436436".to_owned(),
        "accessed_2026-08-06".to_owned(),
    );
    let species = SpeciesIdentity::new(
        "gbif",
        "2436436",
        "Homo sapiens",
        "https://www.gbif.org/species/2436436",
    )?;
    let initial_organisms = vec![
        InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"proof-person-female"),
            species: species.clone(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female")?,
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: None,
        },
        InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"proof-person-male"),
            species,
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("male")?,
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: None,
        },
    ];
    let session =
        initialize_or_resume_world(store, manifest, predecessor_world_id, initial_organisms)
            .await
            .context("initialize non-production proof world")?;

    println!("initialized non-production proof world {world_id}");
    println!(
        "sequence {}, tick {}, state {}",
        session.world.cursor.sequence, session.world.cursor.tick, session.world.cursor.state_hash
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn init_provisional_full_earth_world(
    store: &PostgresStore,
    world_id: WorldId,
    seed: u64,
    composition_path: &std::path::Path,
    artifact_root: &std::path::Path,
    initial_patch: Option<S2CellId>,
    provisional_land_origin_selection_path: Option<&std::path::Path>,
    fauna_range_candidates_path: Option<&std::path::Path>,
    fauna_seeded_selection_path: Option<&std::path::Path>,
    fauna_individuals_per_selected_species: Option<u32>,
    predecessor_world_id: Option<WorldId>,
    tick_duration_seconds: u32,
    max_events_per_partition_transition: u32,
    ruleset_version: u32,
) -> Result<()> {
    let composition = load_provisional_world_composition(composition_path)
        .context("load canonical provisional full-Earth composition")?;
    let verified = verify_provisional_world_artifacts(&composition, artifact_root)
        .context("verify every provisional full-Earth artifact")?;
    let embodied_level = composition.full_earth_grid.levels.embodied_patch;
    let initial_origin = resolve_provisional_initial_origin(
        &composition,
        WorldSeed::new(seed),
        embodied_level,
        initial_patch,
        provisional_land_origin_selection_path,
    )?;
    let initial_patch = initial_origin.patch;
    let partition_level = composition.full_earth_grid.levels.planetary_aggregate;
    let composition_reference = composition
        .execution_reference()
        .context("construct provisional execution reference")?;
    let configuration = WorldConfiguration::new_provisional_full_earth(
        tick_duration_seconds,
        composition.full_earth_grid,
        composition_reference.clone(),
        PartitionedExecution {
            scheduler_schema_version: 1,
            scheduler: SchedulerKind::DeterministicEventQueue,
            partition_s2_level: partition_level,
            person_representation: PersonRepresentation::DurableIndividuals,
            capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
            max_events_per_partition_transition,
        },
    )
    .context("construct provisional full-Earth execution configuration")?;

    let mut manifest = WorldManifest::new(world_id, WorldSeed::new(seed), ruleset_version);
    if let Some(selection_digest) = initial_origin.selection_digest {
        manifest.scientific_datasets.insert(
            "provisional_land_origin_selection".to_owned(),
            selection_digest.to_string(),
        );
    }
    let species = SpeciesIdentity::new(
        "gbif",
        "2436436",
        "Homo sapiens",
        "https://www.gbif.org/species/2436436",
    )?;
    let mut initial_organisms = [
        (b"provisional-founder-a".as_slice(), "female"),
        (b"provisional-founder-b".as_slice(), "male"),
    ]
    .into_iter()
    .map(|(identity, birth_category)| {
        Ok(InitialOrganism {
            organism_id: EntityId::deterministic(world_id, identity),
            species: species.clone(),
            role: OrganismRole::Person,
            birth_category: BirthCategory::new(birth_category)?,
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: Some(initial_patch),
        })
    })
    .collect::<Result<Vec<_>>>()?;
    let fauna = load_provisional_fauna_initial_organisms(
        world_id,
        WorldSeed::new(seed),
        initial_patch,
        fauna_range_candidates_path,
        fauna_seeded_selection_path,
        fauna_individuals_per_selected_species,
    )?;
    if let Some(fauna) = fauna {
        manifest.scientific_datasets.insert(
            "inaturalist_fauna_range_candidate_set".to_owned(),
            fauna.candidate_set_digest.to_string(),
        );
        manifest.scientific_datasets.insert(
            "provisional_fauna_seeded_selection".to_owned(),
            fauna.selection_digest.to_string(),
        );
        initial_organisms.extend(fauna.initial_organisms);
    }
    let session = initialize_or_resume_configured_world(
        store,
        manifest,
        predecessor_world_id,
        configuration,
        initial_organisms,
    )
    .await
    .context("initialize provisional full-Earth world")?;

    println!(
        "verified {} provisional references ({} bytes)",
        verified.artifacts, verified.bytes
    );
    println!(
        "initialized provisional full-Earth world {world_id} from {}@{}",
        composition_reference.composition_id, composition_reference.composition_version
    );
    println!("status: provisional-not-scientifically-admitted");
    println!("composition hash: {}", composition_reference.content_hash);
    println!(
        "sequence {}, tick {}, state {}",
        session.world.cursor.sequence, session.world.cursor.tick, session.world.cursor.state_hash
    );
    Ok(())
}

struct ResolvedInitialOrigin {
    patch: S2CellId,
    selection_digest: Option<world_domain::Digest>,
}

fn resolve_provisional_initial_origin(
    composition: &world_data::ProvisionalWorldComposition,
    world_seed: WorldSeed,
    embodied_level: u8,
    explicit_patch: Option<S2CellId>,
    selection_path: Option<&std::path::Path>,
) -> Result<ResolvedInitialOrigin> {
    match (explicit_patch, selection_path) {
        (Some(_), Some(_)) | (None, None) => anyhow::bail!(
            "provide exactly one of --initial-patch or --provisional-land-origin-selection"
        ),
        (Some(patch), None) => {
            if patch.level() != embodied_level {
                anyhow::bail!(
                    "initial patch {patch} is S2 level {}, expected configured level {embodied_level}",
                    patch.level()
                );
            }
            Ok(ResolvedInitialOrigin {
                patch,
                selection_digest: None,
            })
        }
        (None, Some(selection_path)) => {
            let bytes = std::fs::read(selection_path).with_context(|| {
                format!(
                    "read provisional land-origin selection {}",
                    selection_path.display()
                )
            })?;
            let selection = ProvisionalLandOriginSelection::from_canonical_slice(&bytes)
                .context("validate provisional land-origin selection")?;
            if selection.world_seed != world_seed {
                anyhow::bail!(
                    "provisional land-origin selection world seed does not match the world seed"
                );
            }
            if selection.selected_embodied_patch.level() != embodied_level {
                anyhow::bail!(
                    "provisional land-origin selection targets S2 level {}, expected configured level {embodied_level}",
                    selection.selected_embodied_patch.level()
                );
            }
            let coastline = composition
                .earth_layers
                .iter()
                .find(|layer| layer.kind == DataLayerKind::Coastline)
                .context("provisional composition has no coastline release")?;
            if selection.land_reference_root_digest != coastline.release.content_hash {
                anyhow::bail!(
                    "provisional land-origin selection does not match the composition coastline root"
                );
            }
            Ok(ResolvedInitialOrigin {
                patch: selection.selected_embodied_patch,
                selection_digest: Some(world_domain::Digest::sha256(&bytes)),
            })
        }
    }
}

struct ProvisionalFaunaGenesis {
    candidate_set_digest: world_domain::Digest,
    selection_digest: world_domain::Digest,
    initial_organisms: Vec<InitialOrganism>,
}

fn load_provisional_fauna_initial_organisms(
    world_id: WorldId,
    world_seed: WorldSeed,
    initial_patch: S2CellId,
    candidates_path: Option<&std::path::Path>,
    selection_path: Option<&std::path::Path>,
    individuals_per_selected_species: Option<u32>,
) -> Result<Option<ProvisionalFaunaGenesis>> {
    let provided = [
        candidates_path.is_some(),
        selection_path.is_some(),
        individuals_per_selected_species.is_some(),
    ];
    if provided.iter().all(|provided| !provided) {
        return Ok(None);
    }
    if !provided.iter().all(|provided| *provided) {
        anyhow::bail!(
            "provisional fauna genesis requires --fauna-range-candidates, --fauna-seeded-selection, and --fauna-individuals-per-selected-species together"
        );
    }
    let individuals_per_selected_species =
        individuals_per_selected_species.expect("checked complete provisional fauna arguments");
    if individuals_per_selected_species == 0 {
        anyhow::bail!("--fauna-individuals-per-selected-species must be nonzero");
    }
    let candidate_bytes = std::fs::read(candidates_path.expect("checked candidate path"))
        .context("read provisional fauna range candidate set")?;
    let candidates = FaunaRangeCandidateSet::from_canonical_slice(&candidate_bytes)
        .context("validate provisional fauna range candidate set")?;
    let selection_bytes = std::fs::read(selection_path.expect("checked selection path"))
        .context("read provisional fauna seeded selection")?;
    let selection =
        FaunaSeededSelection::from_canonical_slice_against(&selection_bytes, &candidates)
            .context("validate provisional fauna seeded selection")?;
    if selection.world_seed != world_seed {
        anyhow::bail!("provisional fauna selection world seed does not match the world seed");
    }
    if selection.selected_candidates.is_empty() {
        anyhow::bail!("provisional fauna selection must retain at least one candidate");
    }
    let candidate_set_digest = world_domain::Digest::sha256(&candidate_bytes);
    let selection_digest = world_domain::Digest::sha256(&selection_bytes);
    let capacity = selection
        .selected_candidates
        .len()
        .checked_mul(usize::try_from(individuals_per_selected_species)?)
        .context("provisional fauna genesis count overflows host capacity")?;
    let mut initial_organisms = Vec::with_capacity(capacity);
    for candidate in selection.selected_candidates {
        for ordinal in 0..individuals_per_selected_species {
            let identity = format!(
                "provisional-fauna:{}:{}",
                candidate.species.identifier, ordinal
            );
            initial_organisms.push(InitialOrganism {
                organism_id: EntityId::deterministic(world_id, identity.as_bytes()),
                species: candidate.species.clone(),
                role: OrganismRole::Fauna,
                // This engine-only category intentionally avoids an unsupported
                // demographic assertion or explicit observer presentation.
                birth_category: BirthCategory::new("unspecified")?,
                initial_age_ticks: 0,
                location_id: None,
                embodied_patch: Some(initial_patch),
            });
        }
    }
    Ok(Some(ProvisionalFaunaGenesis {
        candidate_set_digest,
        selection_digest,
        initial_organisms,
    }))
}

async fn serve_memory_worker(
    store: &PostgresStore,
    memory: &HindsightMemory,
    worker_id: &str,
    poll_milliseconds: u64,
    claim_lease_seconds: u32,
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_millis(poll_milliseconds.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tracing::info!(
        worker_id,
        poll_milliseconds = poll_milliseconds.max(1),
        claim_lease_seconds = claim_lease_seconds.max(1),
        "subjective-memory delivery worker started"
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match store.claim_next_memory(worker_id, claim_lease_seconds).await {
                    Ok(Some(entry)) => {
                        let operation_id = entry.retain.operation_id;
                        match memory.retain(&entry.retain).await {
                            Ok(receipt) => {
                                if let Err(error) = store
                                    .mark_memory_accepted(worker_id, &entry, &receipt)
                                    .await
                                {
                                    tracing::warn!(%operation_id, %error, "could not record Hindsight acknowledgement");
                                } else {
                                    tracing::info!(
                                        %operation_id,
                                        remote_operation_id = receipt.remote_operation_id,
                                        attempt = entry.attempt_count,
                                        "Hindsight accepted subjective memory"
                                    );
                                }
                            }
                            Err(error) => {
                                let retry_seconds = retry_delay_seconds(entry.attempt_count);
                                tracing::warn!(
                                    %operation_id,
                                    %error,
                                    retry_seconds,
                                    "Hindsight delivery failed; simulation history is unaffected"
                                );
                                if let Err(store_error) = store
                                    .reschedule_memory(
                                        worker_id,
                                        &entry,
                                        &error.to_string(),
                                        retry_seconds,
                                    )
                                    .await
                                {
                                    tracing::warn!(
                                        %operation_id,
                                        %store_error,
                                        "could not reschedule subjective-memory delivery"
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(%error, "memory outbox unavailable; will retry");
                    }
                }
            }
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(%error, "failed to listen for memory-worker shutdown signal");
                }
                tracing::info!(worker_id, "subjective-memory delivery worker stopping");
                break;
            }
        }
    }

    Ok(())
}

fn retry_delay_seconds(attempt_count: u32) -> u32 {
    let shift = attempt_count.saturating_sub(1).min(8);
    (1_u32 << shift).min(300)
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_data::{
        FAUNA_RANGE_CANDIDATE_SET_SCHEMA_VERSION, FaunaRangeCandidate, FaunaRangeQueryPoint,
    };

    fn candidate_set() -> FaunaRangeCandidateSet {
        FaunaRangeCandidateSet {
            candidate_set_schema_version: FAUNA_RANGE_CANDIDATE_SET_SCHEMA_VERSION,
            candidate_set_id: "inaturalist-v2-20-point-test".to_owned(),
            inaturalist_release: "2.20".to_owned(),
            query_point: FaunaRangeQueryPoint {
                latitude_e7: 446_000_000,
                longitude_e7: -1_105_000_000,
            },
            source_crosswalk_digest: world_domain::Digest::sha256(b"crosswalk"),
            source_gbif_catalog_digest: world_domain::Digest::sha256(b"catalog"),
            source_inaturalist_taxonomy_digest: world_domain::Digest::sha256(b"taxonomy"),
            candidates: vec![
                FaunaRangeCandidate {
                    species: SpeciesIdentity::new(
                        "gbif",
                        "12",
                        "Canis lupus",
                        "https://www.gbif.org/species/12",
                    )
                    .expect("species"),
                    inaturalist_taxon_id: 13,
                    range_package: "mammalia".to_owned(),
                    range_feature_fid: 14,
                },
                FaunaRangeCandidate {
                    species: SpeciesIdentity::new(
                        "gbif",
                        "20",
                        "Lynx canadensis",
                        "https://www.gbif.org/species/20",
                    )
                    .expect("species"),
                    inaturalist_taxon_id: 21,
                    range_package: "mammalia".to_owned(),
                    range_feature_fid: 22,
                },
            ],
        }
    }

    #[test]
    fn provisional_fauna_genesis_requires_and_pins_seeded_source_inputs() {
        let candidates = candidate_set();
        let seed = WorldSeed::new(7);
        let selection = candidates
            .select_seeded_candidates(seed, 2)
            .expect("selection");
        let directory = std::env::temp_dir().join(format!("atc-runner-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("test directory");
        let candidates_path = directory.join("candidates.json");
        let selection_path = directory.join("selection.json");
        std::fs::write(
            &candidates_path,
            candidates.canonical_bytes().expect("canonical candidates"),
        )
        .expect("write candidates");
        std::fs::write(
            &selection_path,
            selection
                .canonical_bytes_against(&candidates)
                .expect("canonical selection"),
        )
        .expect("write selection");
        let world_id = WorldId::from_uuid(Uuid::new_v4());
        let genesis = load_provisional_fauna_initial_organisms(
            world_id,
            seed,
            S2CellId::new(1_u64 << 60).expect("S2 cell"),
            Some(&candidates_path),
            Some(&selection_path),
            Some(2),
        )
        .expect("fauna genesis")
        .expect("provided fauna");
        assert_eq!(genesis.initial_organisms.len(), 4);
        assert!(
            genesis
                .initial_organisms
                .iter()
                .all(|organism| organism.role == OrganismRole::Fauna)
        );
        assert_ne!(genesis.candidate_set_digest, world_domain::Digest::ZERO);
        assert_ne!(genesis.selection_digest, world_domain::Digest::ZERO);
        assert!(
            load_provisional_fauna_initial_organisms(
                world_id,
                WorldSeed::new(8),
                S2CellId::new(1_u64 << 60).expect("S2 cell"),
                Some(&candidates_path),
                Some(&selection_path),
                Some(2),
            )
            .is_err()
        );
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
