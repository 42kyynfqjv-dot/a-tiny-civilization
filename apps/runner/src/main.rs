use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    process::Command as ProcessCommand,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use application::{
    AgentMemory, COGNITION_MODEL_CONTRACT_VERSION, CancerResearchEvidenceDocument,
    CancerResearchJobStore, CancerResearchLiteratureSnapshot, CancerResearchModel,
    CancerResearchModelAdapters, CancerResearchNoveltySource, CancerResearchWorkerConfiguration,
    CancerResearchWorkerOutcome, CognitionModel, CognitionModelRoute, CognitionProviderId,
    CognitionWorkerConfiguration, CognitionWorkerStep, FoundationStore, MemoryOutboxStore,
    ModelCognitionRequest, ServiceHeartbeat, WorldRuntimeError, WorldSession, WorldStore,
    advance_world, advance_world_with_celestial, advance_world_with_celestial_and_cognition,
    calculate_cancer_research_novelty, construct_configured_genesis_with_materials,
    execute_cancer_virtual_experiment, initialize_or_resume_configured_world_with_materials,
    initialize_or_resume_world, process_next_cancer_research_job, process_next_cognition_job,
    resume_world, resume_world_from_snapshot, retire_world_for_successor,
    schedule_due_cancer_research_turn, schedule_world_cognition,
};
use clap::{Parser, Subcommand};
use hindsight_adapter::HindsightMemory;
use model_adapter::OpenAiCompatibleCognition;
use postgres_store::PostgresStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sim_engine::{
    ADULT_BODY_MASS_STATE_RULESET_VERSION, BODILY_REGULATION_RULESET_VERSION,
    CANCER_BIOLOGY_RULESET_VERSION, CANCER_RESEARCH_WORLD_RULESET_VERSION,
    CELESTIAL_DRIVER_RULESET_VERSION, COGNITION_RULESET_VERSION,
    HERITABLE_DISPOSITION_RULESET_VERSION, InitialMaterialInstance, InitialOrganism,
    LOCAL_INTERACTION_RULESET_VERSION, LOCAL_WEATHER_RULESET_VERSION,
    MATERIAL_RESERVOIR_RULESET_VERSION, PartitionCapacityProbe,
    REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION, RULESET_VERSION, replay, replay_from_snapshot,
    run_partition_capacity_probe,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;
use uuid::Uuid;
use world_data::{
    DataLayerKind, FaunaEcologyPlan, FaunaMetabolicRatePlan, FaunaPhysiologyProfileSet,
    FaunaPopulationPlan, FaunaRangeCandidateSet, FaunaSeededSelection,
    ProvisionalLandOriginSelection, ProvisionalMaterialResourcePlan,
    ProvisionalOrganismBodyProfilePlan, ProvisionalOriginClimateEvidence,
    ProvisionalOriginClimateNormals, ProvisionalOriginEnvironment,
};
use world_data_filesystem::{
    load_provisional_world_composition, verify_provisional_world_artifacts,
};
use world_domain::{
    BirthCategory, BodilyNeedState, CANCER_RESEARCH_INITIAL_RESIDENTS,
    CANCER_RESEARCH_NOVELTY_METHOD_VERSION, CANCER_VIRTUAL_LAB_METHOD_VERSION,
    CancerResearchBootstrap, CapacityExhaustionPolicy, CelestialState, Digest, EntityId,
    OrganismRole, PartitionedExecution, PersonRepresentation, ProvisionalLocalEnvironmentBaseline,
    ProvisionalLocalSurfaceBaseline, ProvisionalLocalWeatherBaseline, S2CellId, SchedulerKind,
    SimTick, SpeciesIdentity, TdbSecondsSinceJ2000, WorldConfiguration, WorldExperimentCommitment,
    WorldId, WorldManifest, WorldSeed, WorldStatus,
};

/// New full-Earth worlds start with the source-backed sky and embodied-activity
/// integration driver. Older worlds retain the ruleset committed at genesis.
const DEFAULT_PROVISIONAL_RULESET_VERSION: u32 = LOCAL_INTERACTION_RULESET_VERSION;
const PROVISIONAL_HUMAN_FOUNDER_COUNT: usize = 24;
// The pinned CPU model needs more than 15 seconds to prefill a full bounded
// cognition prompt on the production-class host. Keep this below the default
// 60-second request-to-simulation-deadline window.
const DEFAULT_COGNITION_REQUEST_TIMEOUT_SECONDS: u64 = 45;
const MAX_QUALIFICATION_TICKS: u64 = 1_000_000;

#[derive(Debug, Parser)]
#[command(version, about = "A Tiny Civilization simulation runner")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: Option<String>,

    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 5)]
    database_max_connections: u32,
}

#[allow(clippy::large_enum_variant)]
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
            default_value = "data/provisional/full-earth-breadth-first-0.1.2.json"
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

        /// Canonical source evidence at the selected origin. This is optional for
        /// legacy integration worlds, but a provided file is verified against the
        /// composition and seed-derived origin before its digest is pinned.
        #[arg(long)]
        provisional_origin_environment: Option<PathBuf>,

        /// Exact 1981-2010 ERA5 monthly source bits at the selected origin. This is
        /// pinned noncausal evidence and cannot affect weather or organism state.
        #[arg(long, requires = "provisional_land_origin_selection")]
        provisional_origin_climate_evidence: Option<PathBuf>,

        /// Deterministic fixed-point monthly summaries derived from the exact ERA5
        /// evidence. These remain noncausal and cannot affect world state.
        #[arg(long, requires = "provisional_origin_climate_evidence")]
        provisional_origin_climate_normals: Option<PathBuf>,

        /// Canonical point-scoped modeled-range candidates. Must be supplied with
        /// the seeded selection, origin environment, and population plan inputs.
        #[arg(long)]
        fauna_range_candidates: Option<PathBuf>,

        /// Canonical seed-derived subset of `--fauna-range-candidates`.
        #[arg(long)]
        fauna_seeded_selection: Option<PathBuf>,

        /// Canonical evidence-only origin environment matched by the population plan.
        #[arg(long)]
        fauna_origin_environment: Option<PathBuf>,

        /// Canonical non-admitted ecological-population hand-off. It binds explicit
        /// counts to the seed-derived origin and selected source candidates.
        #[arg(long)]
        fauna_population_plan: Option<PathBuf>,

        /// Retained profile set used by the optional fauna metabolic-rate plan.
        #[arg(long)]
        fauna_metabolic_profile_set: Option<PathBuf>,

        /// One deliberately selected retained rate for every planned fauna taxon.
        #[arg(long)]
        fauna_metabolic_rate_plan: Option<PathBuf>,

        /// Retained exact diet/activity profiles used only as noncausal evidence.
        #[arg(long, requires = "fauna_ecology_plan")]
        fauna_ecology_profile_set: Option<PathBuf>,

        /// Canonical per-world selection of exact noncausal fauna ecology rows.
        #[arg(long, requires = "fauna_ecology_profile_set")]
        fauna_ecology_plan: Option<PathBuf>,

        /// Canonical provisional body profiles for every founder and planned fauna
        /// species. Required by rulesets with canonical bodily regulation.
        #[arg(long)]
        provisional_organism_profile_plan: Option<PathBuf>,

        /// Canonical provisional real-material reservoirs. Required by rulesets
        /// with shared renewable material-source mechanics.
        #[arg(long)]
        provisional_material_resource_plan: Option<PathBuf>,

        #[arg(long)]
        predecessor_world_id: Option<WorldId>,

        /// Refuse initialization when the store contains any different world.
        /// Canonical first-genesis wrappers use this to avoid publishing proof or
        /// qualification histories from a reused database.
        #[arg(long, default_value_t = false)]
        refuse_other_worlds: bool,

        #[arg(long, default_value_t = 300)]
        tick_duration_seconds: u32,

        #[arg(long, default_value_t = 10_000)]
        max_events_per_partition_transition: u32,

        /// Immutable causal ruleset for this new or resumed provisional world.
        /// Ruleset three and later require the pinned DE441 source driver at every tick.
        #[arg(long, default_value_t = DEFAULT_PROVISIONAL_RULESET_VERSION)]
        ruleset_version: u32,

        /// Construct the explicitly artificial Cancer World experiment instead of
        /// an open-ended Earth Genesis world. Requires the current Cancer World
        /// ruleset exactly; older published experiment rulesets remain replay-only.
        #[arg(long, default_value_t = false)]
        cancer_research: bool,
    },
    /// Prove canonical current-ruleset genesis construction and replay without PostgreSQL.
    VerifyProvisionalGenesis {
        #[arg(long)]
        world_id: WorldId,

        #[arg(long)]
        seed: u64,

        /// Portable artifact bundle produced by prepare-provisional-genesis.sh.
        #[arg(long)]
        genesis_directory: PathBuf,

        #[arg(
            long,
            default_value = "data/provisional/full-earth-breadth-first-0.1.2.json"
        )]
        composition: PathBuf,

        #[arg(long, default_value = ".")]
        artifact_root: PathBuf,

        #[arg(long, default_value_t = 300)]
        tick_duration_seconds: u32,

        #[arg(long, default_value_t = 10_000)]
        max_events_per_partition_transition: u32,

        #[arg(long, default_value_t = DEFAULT_PROVISIONAL_RULESET_VERSION)]
        ruleset_version: u32,

        /// Verify the Cancer World genesis variant without writing PostgreSQL.
        #[arg(long, default_value_t = false)]
        cancer_research: bool,
    },
    /// Replay one stored world from genesis and verify its snapshot, cursor, and hashes.
    VerifyWorld {
        #[arg(long)]
        world_id: WorldId,
    },
    /// Retire a populated legacy world for one disclosed successor without
    /// fabricating extinction. The successor is initialized separately.
    RetireForSuccessor {
        #[arg(long)]
        world_id: WorldId,

        #[arg(long)]
        successor_world_id: WorldId,

        #[arg(long, default_value_t = false)]
        confirm_operator_retirement: bool,
    },
    /// Advance exactly N simulation ticks for a non-production qualification world.
    AdvanceQualification {
        #[arg(long)]
        world_id: WorldId,

        #[arg(long)]
        ticks: u64,
    },
    /// Send a fixed synthetic request through OpenRouter's dynamic free route.
    ProbeOpenrouterFree {
        #[arg(long, env = "OPENROUTER_API_KEY", hide_env_values = true)]
        api_key: String,

        #[arg(long, default_value_t = 30)]
        request_timeout_seconds: u64,
    },
    /// Measure the deterministic partition kernel across population and active-fraction samples.
    CapacitySweep {
        #[arg(long, value_delimiter = ',', default_value = "66,660,6600,66000")]
        populations: Vec<u32>,

        #[arg(long, value_delimiter = ',', default_value = "1,10,100")]
        active_percents: Vec<u8>,

        #[arg(long, default_value_t = 64)]
        ticks: u32,
    },
    /// Refresh immutable, permissively licensed GBM literature snapshots from
    /// Europe PMC. This observer-side worker cannot mutate canonical history.
    CancerEvidenceWorker {
        #[arg(long, env = "CANCER_WORLD_ID")]
        world_id: WorldId,

        #[arg(
            long,
            env = "CANCER_EVIDENCE_REFRESH_SECONDS",
            default_value_t = 21_600
        )]
        refresh_seconds: u64,

        #[arg(long, env = "CANCER_EVIDENCE_PAGE_SIZE", default_value_t = 24)]
        page_size: u16,

        #[arg(
            long,
            env = "CANCER_EVIDENCE_ENDPOINT",
            default_value = "https://www.ebi.ac.uk/europepmc/webservices/rest/search"
        )]
        endpoint: String,

        /// Fetch once and exit (used by deployment checks and deterministic tests).
        #[arg(long, default_value_t = false)]
        once: bool,
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

        /// Drain every currently ready entry and exit. Any delivery or store failure is fatal.
        #[arg(long, default_value_t = false)]
        drain: bool,
    },
    /// Prepare replay-safe Hindsight recall and free-first model results. This
    /// process never writes canonical events; the simulation runner admits only
    /// immutable deadline latches.
    CognitionWorker {
        #[arg(long, env = "HINDSIGHT_BASE_URL")]
        hindsight_base_url: String,

        #[arg(long, env = "HINDSIGHT_API_KEY", hide_env_values = true)]
        hindsight_api_key: Option<String>,

        #[arg(
            long,
            env = "COGNITION_WORKER_ID",
            default_value = "local-cognition-worker"
        )]
        worker_id: String,

        #[arg(long, env = "COGNITION_POLL_MILLISECONDS", default_value_t = 250)]
        poll_milliseconds: u64,

        #[arg(long, env = "COGNITION_CLAIM_LEASE_SECONDS", default_value_t = 60)]
        claim_lease_seconds: u32,

        #[arg(
            long,
            env = "COGNITION_REQUEST_TIMEOUT_SECONDS",
            default_value_t = DEFAULT_COGNITION_REQUEST_TIMEOUT_SECONDS
        )]
        request_timeout_seconds: u64,

        /// Explicit approval to send private cognition and recalled-memory context externally.
        #[arg(
            long,
            env = "COGNITION_EXTERNAL_EXPORT_APPROVED",
            default_value_t = false
        )]
        external_export_approved: bool,

        /// Loopback-only OpenAI-compatible `/v1` base. Private context never leaves this host.
        #[arg(long, env = "LOCAL_COGNITION_BASE_URL")]
        local_cognition_base_url: Option<String>,

        /// Full OpenAI-compatible Workers AI base ending in `/ai/v1`.
        #[arg(long, env = "CLOUDFLARE_WORKERS_AI_BASE_URL")]
        cloudflare_workers_ai_base_url: Option<String>,

        #[arg(long, env = "CLOUDFLARE_WORKERS_AI_API_KEY", hide_env_values = true)]
        cloudflare_workers_ai_api_key: Option<String>,

        #[arg(long, env = "GROQ_API_KEY", hide_env_values = true)]
        groq_api_key: Option<String>,

        #[arg(long, env = "CEREBRAS_API_KEY", hide_env_values = true)]
        cerebras_api_key: Option<String>,

        #[arg(long, env = "OPENROUTER_API_KEY", hide_env_values = true)]
        openrouter_api_key: Option<String>,

        /// Dedicated Cancer World key. It is never used by the general free-first ladder.
        #[arg(long, env = "CANCER_OPENROUTER_API_KEY", hide_env_values = true)]
        cancer_openrouter_api_key: Option<String>,

        #[arg(long, env = "COGNITION_PAID_ENABLED", default_value_t = false)]
        paid_enabled: bool,
    },
    /// Process content-addressed Cancer World research turns. This worker has a
    /// dedicated provider identity, memory boundary, and monthly treasury.
    CancerResearchWorker {
        #[arg(
            long,
            env = "CANCER_RESEARCH_WORKER_ID",
            default_value = "local-cancer-research-worker"
        )]
        worker_id: String,

        #[arg(
            long,
            env = "CANCER_RESEARCH_POLL_MILLISECONDS",
            default_value_t = 1_000
        )]
        poll_milliseconds: u64,

        #[arg(
            long,
            env = "CANCER_RESEARCH_CLAIM_LEASE_SECONDS",
            default_value_t = 300
        )]
        claim_lease_seconds: u32,

        #[arg(
            long,
            env = "CANCER_RESEARCH_REQUEST_TIMEOUT_SECONDS",
            default_value_t = 120
        )]
        request_timeout_seconds: u64,

        #[arg(long, env = "CANCER_OPENROUTER_API_KEY", hide_env_values = true)]
        cancer_openrouter_api_key: String,

        /// Metered overflow for exploration after the OpenRouter free quota.
        #[arg(long, env = "CANCER_FIREWORKS_API_KEY", hide_env_values = true)]
        cancer_fireworks_api_key: Option<String>,

        #[arg(
            long,
            env = "CANCER_RESEARCH_EXTERNAL_EXPORT_APPROVED",
            default_value_t = false
        )]
        external_export_approved: bool,

        #[arg(long, env = "CANCER_RESEARCH_PAID_ENABLED", default_value_t = false)]
        paid_enabled: bool,

        #[arg(
            long,
            env = "CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD",
            default_value_t = application::MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        )]
        paid_reservation_micro_usd: u64,

        /// Drain every currently ready research turn and exit.
        #[arg(long, default_value_t = false)]
        drain: bool,
    },
}

#[derive(Serialize)]
struct CapacitySweepSample {
    #[serde(flatten)]
    probe: PartitionCapacityProbe,
    elapsed_nanoseconds: u64,
    ticks_per_second_milli: u64,
    events_per_second: u64,
}

#[derive(Serialize)]
struct CapacitySweepReport {
    report_schema_version: u16,
    status: &'static str,
    workload: &'static str,
    build_profile: &'static str,
    logical_parallelism: usize,
    samples: Vec<CapacitySweepSample>,
}

fn capacity_sweep(populations: &[u32], active_percents: &[u8], ticks: u32) -> Result<()> {
    if cfg!(debug_assertions) {
        anyhow::bail!("capacity-sweep must run from a --release build");
    }
    let mut populations = populations.to_vec();
    populations.sort_unstable();
    populations.dedup();
    if populations.len() < 2 {
        anyhow::bail!("capacity-sweep requires at least two distinct populations");
    }
    let mut active_percents = active_percents.to_vec();
    active_percents.sort_unstable();
    active_percents.dedup();
    if active_percents.len() < 2 {
        anyhow::bail!("capacity-sweep requires at least two distinct active percentages");
    }

    let mut samples = Vec::with_capacity(populations.len() * active_percents.len());
    for population in populations {
        for active_percent in &active_percents {
            let started = Instant::now();
            let probe = run_partition_capacity_probe(population, *active_percent, ticks)
                .context("execute deterministic partition capacity sample")?;
            let elapsed_nanoseconds = u64::try_from(started.elapsed().as_nanos())
                .context("capacity sample duration exceeds u64 nanoseconds")?
                .max(1);
            let ticks_per_second_milli =
                u64::from(ticks).saturating_mul(1_000_000_000_000) / elapsed_nanoseconds;
            let events_per_second =
                probe.emitted_events.saturating_mul(1_000_000_000) / elapsed_nanoseconds;
            samples.push(CapacitySweepSample {
                probe,
                elapsed_nanoseconds,
                ticks_per_second_milli,
                events_per_second,
            });
        }
    }

    let report = CapacitySweepReport {
        report_schema_version: 1,
        status: "operational-partition-kernel-capacity-evidence",
        workload: "synthetic-durable-individuals-one-event-per-active-subject-tick",
        build_profile: "release",
        logical_parallelism: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        samples,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode capacity sweep report")?
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let cli = Cli::parse();
    if let Some(Command::ProbeOpenrouterFree {
        api_key,
        request_timeout_seconds,
    }) = &cli.command
    {
        return probe_openrouter_free(api_key, *request_timeout_seconds).await;
    }
    if let Some(Command::CapacitySweep {
        populations,
        active_percents,
        ticks,
    }) = &cli.command
    {
        return capacity_sweep(populations, active_percents, *ticks);
    }
    if let Some(Command::VerifyProvisionalGenesis {
        world_id,
        seed,
        genesis_directory,
        composition,
        artifact_root,
        tick_duration_seconds,
        max_events_per_partition_transition,
        ruleset_version,
        cancer_research,
    }) = &cli.command
    {
        return verify_provisional_genesis(
            *world_id,
            *seed,
            genesis_directory,
            composition,
            artifact_root,
            *tick_duration_seconds,
            *max_events_per_partition_transition,
            *ruleset_version,
            *cancer_research,
        )
        .await;
    }
    let derived_database_url = if cli.database_url.is_none() {
        database_url_from_postgres_environment()?
    } else {
        None
    };
    let database_url = cli
        .database_url
        .as_deref()
        .or(derived_database_url.as_deref())
        .context(
            "--database-url, DATABASE_URL, or POSTGRES_USER/POSTGRES_PASSWORD/POSTGRES_DB is required except for explicitly database-free commands",
        )?;
    let store = PostgresStore::connect(database_url, cli.database_max_connections)
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
            provisional_origin_environment,
            provisional_origin_climate_evidence,
            provisional_origin_climate_normals,
            fauna_range_candidates,
            fauna_seeded_selection,
            fauna_origin_environment,
            fauna_population_plan,
            fauna_metabolic_profile_set,
            fauna_metabolic_rate_plan,
            fauna_ecology_profile_set,
            fauna_ecology_plan,
            provisional_organism_profile_plan,
            provisional_material_resource_plan,
            predecessor_world_id,
            refuse_other_worlds,
            tick_duration_seconds,
            max_events_per_partition_transition,
            ruleset_version,
            cancer_research,
        } => {
            init_provisional_full_earth_world(
                Some(&store),
                world_id,
                seed,
                &composition,
                &artifact_root,
                initial_patch,
                provisional_land_origin_selection.as_deref(),
                provisional_origin_environment.as_deref(),
                provisional_origin_climate_evidence.as_deref(),
                provisional_origin_climate_normals.as_deref(),
                fauna_range_candidates.as_deref(),
                fauna_seeded_selection.as_deref(),
                fauna_origin_environment.as_deref(),
                fauna_population_plan.as_deref(),
                fauna_metabolic_profile_set.as_deref(),
                fauna_metabolic_rate_plan.as_deref(),
                fauna_ecology_profile_set.as_deref(),
                fauna_ecology_plan.as_deref(),
                provisional_organism_profile_plan.as_deref(),
                provisional_material_resource_plan.as_deref(),
                predecessor_world_id,
                refuse_other_worlds,
                tick_duration_seconds,
                max_events_per_partition_transition,
                ruleset_version,
                cancer_research,
            )
            .await
        }
        Command::VerifyWorld { world_id } => verify_world(&store, world_id).await,
        Command::RetireForSuccessor {
            world_id,
            successor_world_id,
            confirm_operator_retirement,
        } => {
            retire_for_successor(
                &store,
                world_id,
                successor_world_id,
                confirm_operator_retirement,
            )
            .await
        }
        Command::AdvanceQualification { world_id, ticks } => {
            advance_qualification_world(&store, world_id, ticks).await
        }
        Command::ProbeOpenrouterFree { .. } => {
            unreachable!("synthetic provider probe returns before database connection")
        }
        Command::CapacitySweep { .. } => {
            unreachable!("capacity sweep returns before database connection")
        }
        Command::CancerEvidenceWorker {
            world_id,
            refresh_seconds,
            page_size,
            endpoint,
            once,
        } => {
            serve_cancer_evidence_worker(
                &store,
                world_id,
                &endpoint,
                page_size,
                refresh_seconds,
                once,
            )
            .await
        }
        Command::VerifyProvisionalGenesis { .. } => {
            unreachable!("genesis verification returns before database connection")
        }
        Command::MemoryWorker {
            hindsight_base_url,
            hindsight_api_key,
            worker_id,
            poll_milliseconds,
            claim_lease_seconds,
            request_timeout_seconds,
            drain,
        } => {
            let memory = HindsightMemory::new(
                &hindsight_base_url,
                hindsight_api_key,
                Duration::from_secs(request_timeout_seconds.max(1)),
            )
            .context("configure Hindsight memory adapter")?;
            let research_memories = store
                .backfill_cancer_research_memories()
                .await
                .context("backfill Cancer World research memories")?;
            tracing::info!(
                inserted = research_memories,
                "Cancer World research-memory mirror reconciled"
            );
            serve_memory_worker(
                &store,
                &memory,
                &worker_id,
                poll_milliseconds,
                claim_lease_seconds,
                drain,
            )
            .await
        }
        Command::CognitionWorker {
            hindsight_base_url,
            hindsight_api_key,
            worker_id,
            poll_milliseconds,
            claim_lease_seconds,
            request_timeout_seconds,
            external_export_approved,
            local_cognition_base_url,
            cloudflare_workers_ai_base_url,
            cloudflare_workers_ai_api_key,
            groq_api_key,
            cerebras_api_key,
            openrouter_api_key,
            cancer_openrouter_api_key,
            paid_enabled,
        } => {
            let timeout = Duration::from_secs(request_timeout_seconds.max(1));
            let memory = HindsightMemory::new(&hindsight_base_url, hindsight_api_key, timeout)
                .context("configure Hindsight cognition recall adapter")?;
            let adapters = cognition_adapters(
                local_cognition_base_url,
                cloudflare_workers_ai_base_url,
                cloudflare_workers_ai_api_key,
                groq_api_key,
                cerebras_api_key,
                openrouter_api_key,
                cancer_openrouter_api_key,
                timeout,
            )?;
            let external_provider_count = adapters
                .keys()
                .filter(|provider| provider.as_str() != "local_openai")
                .count();
            validate_cognition_export_approval(external_provider_count, external_export_approved)?;
            let configuration = CognitionWorkerConfiguration::production(paid_enabled);
            serve_cognition_worker(
                &store,
                &memory,
                &adapters,
                &worker_id,
                poll_milliseconds,
                claim_lease_seconds,
                &configuration,
            )
            .await
        }
        Command::CancerResearchWorker {
            worker_id,
            poll_milliseconds,
            claim_lease_seconds,
            request_timeout_seconds,
            cancer_openrouter_api_key,
            cancer_fireworks_api_key,
            external_export_approved,
            paid_enabled,
            paid_reservation_micro_usd,
            drain,
        } => {
            if !external_export_approved {
                anyhow::bail!(
                    "Cancer World research export requires CANCER_RESEARCH_EXTERNAL_EXPORT_APPROVED=true"
                );
            }
            let provider = CognitionProviderId::openrouter_cancer();
            let adapter = OpenAiCompatibleCognition::new(
                provider.clone(),
                "https://openrouter.ai/api/v1",
                cancer_openrouter_api_key,
                Duration::from_secs(request_timeout_seconds.max(1)),
            )
            .context("configure dedicated Cancer World OpenRouter adapter")?;
            let mut adapters: CancerResearchModelAdapters = BTreeMap::new();
            adapters.insert(provider, Arc::new(adapter) as Arc<dyn CancerResearchModel>);
            if let Some(api_key) = nonempty(cancer_fireworks_api_key) {
                let provider = CognitionProviderId::fireworks_cancer();
                let adapter = OpenAiCompatibleCognition::new(
                    provider.clone(),
                    "https://api.fireworks.ai/inference/v1",
                    api_key,
                    Duration::from_secs(request_timeout_seconds.max(1)),
                )
                .context("configure dedicated Cancer World Fireworks adapter")?;
                adapters.insert(provider, Arc::new(adapter) as Arc<dyn CancerResearchModel>);
            }
            let configuration = CancerResearchWorkerConfiguration {
                claim_lease_seconds,
                paid_reservation_micro_usd,
                paid_enabled,
                ..CancerResearchWorkerConfiguration::default()
            };
            serve_cancer_research_worker(
                &store,
                &adapters,
                &worker_id,
                poll_milliseconds,
                &configuration,
                drain,
            )
            .await
        }
    }
}

fn database_url_from_postgres_environment() -> Result<Option<String>> {
    let Ok(user) = std::env::var("POSTGRES_USER") else {
        return Ok(None);
    };
    let password = std::env::var("POSTGRES_PASSWORD")
        .context("POSTGRES_PASSWORD is required when deriving DATABASE_URL")?;
    let database = std::env::var("POSTGRES_DB")
        .context("POSTGRES_DB is required when deriving DATABASE_URL")?;
    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = std::env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_owned());
    let mut url = Url::parse(&format!("postgres://{host}:{port}/"))
        .context("construct PostgreSQL URL from environment")?;
    url.set_username(&user)
        .map_err(|_| anyhow::anyhow!("POSTGRES_USER cannot be encoded as a URL username"))?;
    url.set_password(Some(&password))
        .map_err(|_| anyhow::anyhow!("POSTGRES_PASSWORD cannot be encoded as a URL password"))?;
    url.set_path(&database);
    Ok(Some(url.into()))
}

const EUROPE_PMC_GBM_QUERY: &str = "(TITLE_ABS:\"glioblastoma\" OR TITLE_ABS:\"glioblastoma multiforme\") AND OPEN_ACCESS:Y AND (LICENSE:\"CC BY\" OR LICENSE:\"CC0\") sort_date:y";

async fn serve_cancer_evidence_worker(
    store: &PostgresStore,
    world_id: WorldId,
    endpoint: &str,
    page_size: u16,
    refresh_seconds: u64,
    once: bool,
) -> Result<()> {
    let endpoint = Url::parse(endpoint).context("parse Cancer World evidence endpoint")?;
    if endpoint.scheme() != "https" {
        anyhow::bail!("Cancer World evidence endpoint must use HTTPS");
    }
    let page_size = page_size.clamp(1, 100);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("a-tiny-civilization/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build Cancer World evidence client")?;
    let refresh_interval = Duration::from_secs(refresh_seconds.max(300));
    let mut next_evidence_refresh = Instant::now();
    loop {
        if Instant::now() >= next_evidence_refresh {
            match refresh_cancer_evidence(store, &client, world_id, endpoint.clone(), page_size)
                .await
            {
                Ok(count) => {
                    tracing::info!(world_id = %world_id, snapshot_count = count, "Cancer World evidence refresh completed")
                }
                Err(error) if !once => {
                    tracing::error!(world_id = %world_id, %error, "Cancer World evidence refresh failed; durable evidence remains available")
                }
                Err(error) => return Err(error),
            }
            next_evidence_refresh = Instant::now() + refresh_interval;
        }
        match refresh_cancer_novelty_audits(store, &client, world_id, endpoint.clone()).await {
            Ok(count) if count > 0 => {
                tracing::info!(world_id = %world_id, audit_count = count, "Cancer World novelty audit batch completed")
            }
            Ok(_) => {}
            Err(error) if !once => {
                tracing::error!(world_id = %world_id, %error, "Cancer World novelty audit failed; unaudited artifacts remain queued")
            }
            Err(error) => return Err(error),
        }
        match execute_pending_cancer_virtual_experiments(store, world_id).await {
            Ok(count) if count > 0 => {
                tracing::info!(world_id = %world_id, experiment_count = count, "Cancer World virtual experiment batch completed")
            }
            Ok(_) => {}
            Err(error) if !once => {
                tracing::error!(world_id = %world_id, %error, "Cancer World virtual experiment batch failed; planned experiments remain queued")
            }
            Err(error) => return Err(error),
        }
        if once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn execute_pending_cancer_virtual_experiments(
    store: &PostgresStore,
    world_id: WorldId,
) -> Result<usize> {
    let candidates = store
        .load_unexecuted_cancer_virtual_experiments(world_id, CANCER_VIRTUAL_LAB_METHOD_VERSION, 32)
        .await
        .context("load planned Cancer World virtual experiments")?;
    let mut stored = 0_usize;
    for candidate in candidates {
        let result = execute_cancer_virtual_experiment(&candidate)
            .context("execute deterministic Cancer World virtual experiment")?;
        store
            .store_cancer_virtual_experiment_result(
                &result,
                &candidate.contribution,
                candidate.ordinal,
            )
            .await
            .context("store Cancer World virtual experiment result")?;
        stored += 1;
    }
    Ok(stored)
}

async fn refresh_cancer_novelty_audits(
    store: &PostgresStore,
    client: &reqwest::Client,
    world_id: WorldId,
    endpoint: Url,
) -> Result<usize> {
    let candidates = store
        .load_unaudited_cancer_research(world_id, CANCER_RESEARCH_NOVELTY_METHOD_VERSION, 12)
        .await
        .context("load unaudited Cancer World research")?;
    let mut stored = 0_usize;
    for candidate in candidates {
        let query_terms = application::cancer_research_novelty_query_terms(&candidate.contribution);
        let mut search_endpoint = endpoint.clone();
        let mechanism_query = query_terms
            .iter()
            .map(|term| format!("TITLE_ABS:\"{term}\""))
            .collect::<Vec<_>>()
            .join(" OR ");
        let query = format!(
            "(TITLE_ABS:\"glioblastoma\" OR TITLE_ABS:\"glioblastoma multiforme\") AND ({mechanism_query})"
        );
        search_endpoint
            .query_pairs_mut()
            .append_pair("query", &query)
            .append_pair("format", "json")
            .append_pair("resultType", "core")
            .append_pair("pageSize", "12");
        let payload: serde_json::Value = client
            .get(search_endpoint)
            .send()
            .await
            .context("request Europe PMC novelty search")?
            .error_for_status()
            .context("Europe PMC novelty search returned an error")?
            .json()
            .await
            .context("decode Europe PMC novelty search")?;
        let results = payload
            .pointer("/resultList/result")
            .and_then(serde_json::Value::as_array)
            .context("Europe PMC novelty response is missing resultList.result")?;
        let sources = results
            .iter()
            .filter_map(europe_pmc_novelty_source)
            .collect::<Vec<_>>();
        let audit = calculate_cancer_research_novelty(&candidate, &sources)
            .context("calculate Cancer World novelty audit")?;
        store
            .store_cancer_research_novelty_audit(&audit)
            .await
            .context("store Cancer World novelty audit")?;
        stored += 1;
    }
    Ok(stored)
}

fn europe_pmc_novelty_source(
    source_payload: &serde_json::Value,
) -> Option<CancerResearchNoveltySource> {
    let field = |name: &str| source_payload.get(name).and_then(serde_json::Value::as_str);
    let id = field("id")?.trim();
    let source = field("source")?.trim();
    let title = field("title")?.trim();
    if id.is_empty() || source.is_empty() || title.is_empty() {
        return None;
    }
    Some(CancerResearchNoveltySource {
        source_id: format!("https://europepmc.org/article/{source}/{id}"),
        title: title.to_owned(),
        published_on: field("firstPublicationDate").map(str::to_owned),
        abstract_text: field("abstractText").unwrap_or_default().trim().to_owned(),
    })
}

async fn refresh_cancer_evidence(
    store: &PostgresStore,
    client: &reqwest::Client,
    world_id: WorldId,
    mut endpoint: Url,
    page_size: u16,
) -> Result<usize> {
    endpoint
        .query_pairs_mut()
        .append_pair("query", EUROPE_PMC_GBM_QUERY)
        .append_pair("format", "json")
        .append_pair("resultType", "core")
        .append_pair("pageSize", &page_size.to_string());
    let payload: serde_json::Value = client
        .get(endpoint)
        .send()
        .await
        .context("request Europe PMC literature search")?
        .error_for_status()
        .context("Europe PMC literature search returned an error")?
        .json()
        .await
        .context("decode Europe PMC literature search")?;
    let results = payload
        .pointer("/resultList/result")
        .and_then(serde_json::Value::as_array)
        .context("Europe PMC response is missing resultList.result")?;
    let retrieved_at = chrono::Utc::now();
    let mut stored = 0_usize;
    for source_payload in results {
        let Some(snapshot) = europe_pmc_snapshot(world_id, retrieved_at, source_payload.clone())?
        else {
            continue;
        };
        store
            .store_cancer_research_literature(&snapshot)
            .await
            .context("store Cancer World literature snapshot")?;
        stored += 1;
    }
    Ok(stored)
}

fn europe_pmc_snapshot(
    world_id: WorldId,
    retrieved_at: chrono::DateTime<chrono::Utc>,
    source_payload: serde_json::Value,
) -> Result<Option<CancerResearchLiteratureSnapshot>> {
    let field = |name: &str| source_payload.get(name).and_then(serde_json::Value::as_str);
    let Some(id) = field("id") else {
        return Ok(None);
    };
    let Some(source) = field("source") else {
        return Ok(None);
    };
    let Some(title) = field("title")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let license = field("license")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !matches!(license.as_str(), "cc by" | "cc0") {
        return Ok(None);
    }
    let Some(abstract_text) = field("abstractText")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let source_id = format!("https://europepmc.org/article/{source}/{id}");
    let published_at = field("firstPublicationDate")
        .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
    let content = serde_json::to_string(&json!({
        "evidence_schema_version": 1,
        "source": "Europe PMC",
        "source_id": source_id,
        "title": title,
        "abstract": abstract_text,
        "authors": field("authorString"),
        "doi": field("doi"),
        "pmid": field("pmid"),
        "pmcid": field("pmcid"),
        "publication_date": field("firstPublicationDate"),
        "publication_types": source_payload.pointer("/pubTypeList/pubType"),
        "license": license,
        "open_access": field("isOpenAccess") == Some("Y"),
        "cited_by_count": source_payload.get("citedByCount"),
        "warning": "Source abstract only. Claims remain unverified until independently replicated."
    }))
    .context("encode bounded Europe PMC evidence")?;
    if content.len() > application::MAX_CANCER_RESEARCH_EVIDENCE_DOCUMENT_BYTES {
        return Ok(None);
    }
    let content_hash = Digest::sha256(content.as_bytes());
    let evidence_id = Uuid::new_v5(
        &world_id.as_uuid(),
        format!("{source_id}:{content_hash}").as_bytes(),
    );
    let snapshot = CancerResearchLiteratureSnapshot {
        evidence_id,
        world_id,
        source_id: source_id.clone(),
        title: title.to_owned(),
        license,
        published_at,
        document: CancerResearchEvidenceDocument {
            reference: world_domain::CancerResearchEvidenceReference {
                kind: world_domain::CancerResearchEvidenceKind::Literature,
                source_id,
                content_hash,
            },
            content,
        },
        source_payload,
        retrieved_at,
    };
    snapshot
        .validate()
        .context("validate Europe PMC evidence snapshot")?;
    Ok(Some(snapshot))
}

async fn probe_openrouter_free(api_key: &str, request_timeout_seconds: u64) -> Result<()> {
    let adapter = OpenAiCompatibleCognition::new(
        CognitionProviderId::openrouter(),
        "https://openrouter.ai/api/v1",
        api_key.to_owned(),
        Duration::from_secs(request_timeout_seconds.max(1)),
    )
    .context("configure synthetic OpenRouter probe")?;
    let request = synthetic_cognition_probe_request();
    let route = CognitionModelRoute::openrouter_free();
    let receipt = adapter
        .infer(&route, &request)
        .await
        .context("execute synthetic OpenRouter free-route probe")?;
    println!(
        "synthetic OpenRouter free route passed: resolved_model={}, prompt_tokens={}, completion_tokens={}, billed_micro_usd={}",
        receipt.resolved_model,
        receipt.usage.prompt_tokens,
        receipt.usage.completion_tokens,
        receipt.billed_micro_usd
    );
    Ok(())
}

fn synthetic_cognition_probe_request() -> ModelCognitionRequest {
    let world_id = WorldId::from_uuid(Uuid::from_u128(0x4154_494e_5950_524f_4245));
    let agent_id = EntityId::deterministic(world_id, b"synthetic-provider-contract-probe");
    let selected_at_tick = SimTick::new(20);
    let ordinal = 0;
    ModelCognitionRequest {
        contract_version: COGNITION_MODEL_CONTRACT_VERSION,
        request_id: application::cognition_request_id(
            world_id,
            agent_id,
            selected_at_tick,
            ordinal,
        ),
        world_id,
        agent_id,
        ordinal,
        selected_at_tick,
        deadline_tick: SimTick::new(32),
        bodily_needs: BodilyNeedState::default(),
        readings: Vec::new(),
        action_values: Vec::new(),
        recalled_memories: Vec::new(),
        max_output_tokens: 32,
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

async fn retire_for_successor(
    store: &PostgresStore,
    world_id: WorldId,
    successor_world_id: WorldId,
    confirmed: bool,
) -> Result<()> {
    if !confirmed {
        anyhow::bail!(
            "successor retirement requires the literal --confirm-operator-retirement flag"
        );
    }
    if world_id == successor_world_id {
        anyhow::bail!("a world cannot be its own successor");
    }
    let _writer_lock = store
        .acquire_runner_writer_lock()
        .await
        .context("acquire the database canonical-writer lock")?;
    if store
        .list_world_ids()
        .await
        .context("list worlds before successor retirement")?
        .contains(&successor_world_id)
    {
        anyhow::bail!(
            "successor world {successor_world_id} already exists; retirement must bind an uninitialized successor ID"
        );
    }
    let current = resume_world_from_snapshot(store, world_id).await.context(
        "verify anchored snapshot and bounded immutable tail before successor retirement",
    )?;
    if current.world.manifest.experiment.is_some() {
        anyhow::bail!("experimental worlds cannot use the public-world successor cutover");
    }
    let retired = retire_world_for_successor(store, &current, successor_world_id)
        .await
        .context("commit auditable world successor retirement")?;
    print_verified_world(world_id, retired)?;
    println!(
        "retired world {world_id} for successor {successor_world_id}; no extinction was recorded"
    );
    Ok(())
}

async fn advance_qualification_world(
    store: &PostgresStore,
    world_id: WorldId,
    ticks: u64,
) -> Result<()> {
    if is_production_environment(std::env::var("APP_ENV").ok().as_deref()) {
        anyhow::bail!("advance-qualification is prohibited when APP_ENV=production");
    }
    if ticks == 0 || ticks > MAX_QUALIFICATION_TICKS {
        anyhow::bail!("qualification tick count must be between 1 and {MAX_QUALIFICATION_TICKS}");
    }
    let _writer_lock = store
        .acquire_runner_writer_lock()
        .await
        .context("acquire the database canonical-writer lock")?;
    let mut session = resume_world_from_snapshot(store, world_id)
        .await
        .context("verify qualification world before bounded advancement")?;
    if session.world.status != WorldStatus::Running {
        anyhow::bail!("qualification world is not running");
    }
    let start_tick = session.world.cursor.tick.get();
    let target_tick = start_tick
        .checked_add(ticks)
        .context("qualification target tick overflow")?;
    let start_sequence = session.world.cursor.sequence;
    while session.world.cursor.tick.get() < target_tick {
        session = advance_one_world_once(store, &session)
            .await
            .context("advance one bounded qualification cycle")?;
        if session.world.status != WorldStatus::Running
            && session.world.cursor.tick.get() < target_tick
        {
            anyhow::bail!(
                "qualification world stopped at tick {} before target {target_tick}",
                session.world.cursor.tick
            );
        }
    }
    println!(
        "advanced qualification world {world_id} by {ticks} ticks (sequence {} to {}, tick {start_tick} to {target_tick})",
        start_sequence, session.world.cursor.sequence
    );
    print_verified_world(world_id, session)
}

fn is_production_environment(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim().eq_ignore_ascii_case("production"))
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
    let _writer_lock = store
        .acquire_runner_writer_lock()
        .await
        .context("acquire the database canonical-writer lock")?;
    let instance_id = Uuid::new_v4();
    let heartbeat = ServiceHeartbeat {
        service_name: "simulation-runner".to_owned(),
        instance_id,
        metadata: json!({
            "baseline_ruleset_version": RULESET_VERSION,
            "default_provisional_ruleset_version": DEFAULT_PROVISIONAL_RULESET_VERSION,
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
        baseline_ruleset_version = RULESET_VERSION,
        default_provisional_ruleset_version = DEFAULT_PROVISIONAL_RULESET_VERSION,
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
            _ = shutdown_signal() => {
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
                let session = resume_world_from_snapshot(store, world_id).await?;
                tracing::info!(
                    %world_id,
                    sequence = %session.world.cursor.sequence,
                    tick = %session.world.cursor.tick,
                    "verified world history before resume"
                );
                session
            }
        };
        let previous_tick = current.world.cursor.tick;
        let next = advance_one_world_once(store, &current).await?;
        if next.world.cursor.tick == previous_tick {
            tracing::info!(
                %world_id,
                sequence = %next.world.cursor.sequence,
                tick = %next.world.cursor.tick,
                "committed deterministic world-selected cognition request"
            );
        } else {
            tracing::info!(
                %world_id,
                sequence = %next.world.cursor.sequence,
                tick = %next.world.cursor.tick,
                status = ?next.world.status,
                state_hash = %next.world.cursor.state_hash,
                "committed deterministic transition"
            );
        }
        if next.world.status == WorldStatus::Running {
            sessions.insert(world_id, next);
        }
    }
    Ok(())
}

async fn advance_one_world_once(
    store: &PostgresStore,
    current: &WorldSession,
) -> Result<WorldSession, WorldRuntimeError> {
    if current.state.ruleset_version() >= CANCER_RESEARCH_WORLD_RULESET_VERSION {
        schedule_due_cancer_research_turn(store, &current.state)
            .await
            .map_err(|error| {
                WorldRuntimeError::Integrity(format!(
                    "Cancer World research scheduling failed: {error}"
                ))
            })?;
    }
    if current.state.ruleset_version() >= COGNITION_RULESET_VERSION
        && let Some(selected) = schedule_world_cognition(store, current).await?
    {
        return Ok(selected);
    }
    if current.state.ruleset_version() >= COGNITION_RULESET_VERSION {
        let celestial = evaluate_pinned_de441(current)
            .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
        advance_world_with_celestial_and_cognition(store, current, celestial).await
    } else if current.state.ruleset_version() >= CELESTIAL_DRIVER_RULESET_VERSION {
        let celestial = evaluate_pinned_de441(current)
            .map_err(|error| WorldRuntimeError::Integrity(error.to_string()))?;
        advance_world_with_celestial(store, current, celestial).await
    } else {
        advance_world(store, current).await
    }
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
    let data_executable = std::env::var_os("ATINY_CIVILIZATION_DATA_EXECUTABLE")
        .unwrap_or_else(|| "/app/civilization-data".into());
    let input_directory = std::env::var_os("ATINY_JPL_DE441_INPUT_DIRECTORY")
        .unwrap_or_else(|| "/runtime/data/source-cache/jpl-de441".into());
    let output = ProcessCommand::new(data_executable)
        .args(["inspect", "jpl-de441-epoch", "--input-directory"])
        .arg(input_directory)
        .args(["--tdb-seconds-from-j2000", &tdb_seconds_text])
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
            metabolic_rate: None,
            adult_body_mass: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
        },
        InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"proof-person-male"),
            species,
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("male")?,
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: None,
            metabolic_rate: None,
            adult_body_mass: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
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

fn provisional_human_founders(
    world_id: WorldId,
    initial_patch: S2CellId,
) -> Result<Vec<InitialOrganism>> {
    let species = SpeciesIdentity::new(
        "gbif",
        "2436436",
        "Homo sapiens",
        "https://www.gbif.org/species/2436436",
    )?;
    (0..PROVISIONAL_HUMAN_FOUNDER_COUNT)
        .map(|ordinal| {
            let identity = format!("provisional-human-founder-{ordinal:02}");
            let birth_category = if ordinal % 2 == 0 { "female" } else { "male" };
            Ok(InitialOrganism {
                organism_id: EntityId::deterministic(world_id, identity.as_bytes()),
                species: species.clone(),
                role: OrganismRole::Person,
                birth_category: BirthCategory::new(birth_category)?,
                initial_age_ticks: 0,
                location_id: None,
                embodied_patch: Some(initial_patch),
                metabolic_rate: None,
                adult_body_mass: None,
                physiological_regulation: None,
                reproductive_physiology: None,
                heritable_disposition_profile: None,
            })
        })
        .collect()
}

fn cancer_research_human_founders(
    world_id: WorldId,
    initial_patch: S2CellId,
    tick_duration_seconds: u32,
) -> Result<Vec<InitialOrganism>> {
    let species = SpeciesIdentity::new(
        "gbif",
        "2436436",
        "Homo sapiens",
        "https://www.gbif.org/species/2436436",
    )?;
    let seconds_per_julian_year = 31_557_600_u64;
    let tick_duration_seconds = u64::from(tick_duration_seconds.max(1));
    (0..CANCER_RESEARCH_INITIAL_RESIDENTS)
        .map(|ordinal| {
            let identity = format!("cancer-resident-{ordinal:04}");
            let birth_category = if ordinal % 2 == 0 { "female" } else { "male" };
            let adult_age_years = 25_u64 + u64::from(ordinal % 31);
            let initial_age_ticks = adult_age_years
                .checked_mul(seconds_per_julian_year)
                .context("Cancer World founder age overflow")?
                / tick_duration_seconds;
            Ok(InitialOrganism {
                organism_id: EntityId::deterministic(world_id, identity.as_bytes()),
                species: species.clone(),
                role: OrganismRole::Person,
                birth_category: BirthCategory::new(birth_category)?,
                initial_age_ticks,
                location_id: None,
                embodied_patch: Some(initial_patch),
                metabolic_rate: None,
                adult_body_mass: None,
                physiological_regulation: None,
                reproductive_physiology: None,
                heritable_disposition_profile: None,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn verify_provisional_genesis(
    world_id: WorldId,
    seed: u64,
    genesis_directory: &std::path::Path,
    composition_path: &std::path::Path,
    artifact_root: &std::path::Path,
    tick_duration_seconds: u32,
    max_events_per_partition_transition: u32,
    ruleset_version: u32,
    cancer_research: bool,
) -> Result<()> {
    let manifest_digest = verify_portable_genesis_manifest(genesis_directory)?;
    println!("portable genesis manifest: {manifest_digest}");
    println!("verifying the complete content-addressed full-Earth artifact tree...");
    let origin_selection = genesis_directory.join("origin-selection.json");
    let origin_environment = genesis_directory.join("origin-environment.json");
    let origin_climate_evidence = genesis_directory.join("origin-climate-evidence.json");
    let origin_climate_normals = genesis_directory.join("origin-climate-normals.json");
    let fauna_candidates = genesis_directory.join("fauna-candidates.json");
    let fauna_selection = genesis_directory.join("fauna-selection.json");
    let fauna_population = genesis_directory.join("fauna-population-plan.json");
    let fauna_ecology = genesis_directory.join("fauna-ecology-plan.json");
    let organism_profiles = genesis_directory.join("organism-body-profile-plan.json");
    let material_resources = genesis_directory.join("material-resource-plan.json");
    let fauna_ecology_profiles =
        std::path::Path::new("data/derived-cache/eltontraits-ecology-v2.json");
    init_provisional_full_earth_world(
        None,
        world_id,
        seed,
        composition_path,
        artifact_root,
        None,
        Some(&origin_selection),
        Some(&origin_environment),
        Some(&origin_climate_evidence),
        Some(&origin_climate_normals),
        Some(&fauna_candidates),
        Some(&fauna_selection),
        Some(&origin_environment),
        Some(&fauna_population),
        None,
        None,
        Some(fauna_ecology_profiles),
        Some(&fauna_ecology),
        Some(&organism_profiles),
        Some(&material_resources),
        None,
        false,
        tick_duration_seconds,
        max_events_per_partition_transition,
        ruleset_version,
        cancer_research,
    )
    .await
}

fn verify_portable_genesis_manifest(genesis_directory: &std::path::Path) -> Result<Digest> {
    let manifest_path = genesis_directory.join("SHA256SUMS");
    let metadata = fs::symlink_metadata(&manifest_path)
        .with_context(|| format!("inspect genesis manifest {}", manifest_path.display()))?;
    if !metadata.file_type().is_file() {
        anyhow::bail!("genesis SHA256SUMS must be a regular, non-symlink file");
    }
    let manifest_bytes = fs::read(&manifest_path).context("read genesis SHA256SUMS")?;
    let manifest_text = std::str::from_utf8(&manifest_bytes)
        .context("genesis SHA256SUMS must contain UTF-8 text")?;
    let mut covered = BTreeSet::new();
    for line in manifest_text.lines() {
        let (expected, relative) = line
            .split_once("  ./")
            .context("genesis SHA256SUMS has a noncanonical or nonportable line")?;
        if expected.len() != 64
            || !expected
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || relative.is_empty()
            || relative.contains('/')
            || relative.contains('\\')
            || relative.chars().any(char::is_whitespace)
            || !covered.insert(relative.to_owned())
        {
            anyhow::bail!("genesis SHA256SUMS has a noncanonical or nonportable line");
        }
        let artifact = genesis_directory.join(relative);
        let metadata = fs::symlink_metadata(&artifact)
            .with_context(|| format!("inspect genesis artifact {}", artifact.display()))?;
        if !metadata.file_type().is_file() {
            anyhow::bail!("genesis artifact must be a regular, non-symlink file: {relative}");
        }
        let actual = Digest::sha256(
            &fs::read(&artifact)
                .with_context(|| format!("read genesis artifact {}", artifact.display()))?,
        );
        if actual.to_string() != expected {
            anyhow::bail!("genesis checksum mismatch: {relative}");
        }
    }
    let mut present = BTreeSet::new();
    for entry in fs::read_dir(genesis_directory).with_context(|| {
        format!(
            "enumerate provisional genesis directory {}",
            genesis_directory.display()
        )
    })? {
        let entry = entry.context("read provisional genesis directory entry")?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("genesis artifact name is not UTF-8"))?;
        if name == "SHA256SUMS" {
            continue;
        }
        if !entry
            .file_type()
            .context("inspect provisional genesis directory entry")?
            .is_file()
        {
            anyhow::bail!("genesis bundle contains a non-regular entry: {name}");
        }
        present.insert(name);
    }
    if covered.is_empty() || covered != present {
        anyhow::bail!("genesis SHA256SUMS must cover every and only genesis artifact");
    }
    Ok(Digest::sha256(&manifest_bytes))
}

#[allow(clippy::too_many_arguments)]
async fn init_provisional_full_earth_world(
    store: Option<&PostgresStore>,
    world_id: WorldId,
    seed: u64,
    composition_path: &std::path::Path,
    artifact_root: &std::path::Path,
    initial_patch: Option<S2CellId>,
    provisional_land_origin_selection_path: Option<&std::path::Path>,
    provisional_origin_environment_path: Option<&std::path::Path>,
    provisional_origin_climate_evidence_path: Option<&std::path::Path>,
    provisional_origin_climate_normals_path: Option<&std::path::Path>,
    fauna_range_candidates_path: Option<&std::path::Path>,
    fauna_seeded_selection_path: Option<&std::path::Path>,
    fauna_origin_environment_path: Option<&std::path::Path>,
    fauna_population_plan_path: Option<&std::path::Path>,
    fauna_metabolic_profile_set_path: Option<&std::path::Path>,
    fauna_metabolic_rate_plan_path: Option<&std::path::Path>,
    fauna_ecology_profile_set_path: Option<&std::path::Path>,
    fauna_ecology_plan_path: Option<&std::path::Path>,
    provisional_organism_profile_plan_path: Option<&std::path::Path>,
    provisional_material_resource_plan_path: Option<&std::path::Path>,
    predecessor_world_id: Option<WorldId>,
    refuse_other_worlds: bool,
    tick_duration_seconds: u32,
    max_events_per_partition_transition: u32,
    ruleset_version: u32,
    cancer_research: bool,
) -> Result<()> {
    if cancer_research && ruleset_version != CANCER_BIOLOGY_RULESET_VERSION {
        anyhow::bail!(
            "Cancer World genesis requires exact ruleset {CANCER_BIOLOGY_RULESET_VERSION}"
        );
    }
    if refuse_other_worlds {
        let store = store.context("exclusive initialization requires PostgreSQL")?;
        let other_worlds = store
            .list_world_ids()
            .await
            .context("list worlds before exclusive initialization")?
            .into_iter()
            .filter(|stored_world_id| *stored_world_id != world_id)
            .collect::<Vec<_>>();
        if !other_worlds.is_empty() {
            anyhow::bail!(
                "exclusive initialization refused a store containing other worlds: {}",
                other_worlds
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
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
    let origin_environment = load_provisional_origin_environment(
        provisional_origin_environment_path,
        &composition,
        &initial_origin,
    )?;
    let origin_climate_evidence = load_provisional_origin_climate_evidence(
        provisional_origin_climate_evidence_path,
        &initial_origin,
    )?;
    let origin_climate_normals = load_provisional_origin_climate_normals(
        provisional_origin_climate_normals_path,
        origin_climate_evidence.as_ref(),
        &initial_origin,
    )?;
    let partition_level = composition.full_earth_grid.levels.planetary_aggregate;
    let composition_reference = composition
        .execution_reference()
        .context("construct provisional execution reference")?;
    let execution = PartitionedExecution {
        scheduler_schema_version: 1,
        scheduler: SchedulerKind::DeterministicEventQueue,
        partition_s2_level: partition_level,
        person_representation: PersonRepresentation::DurableIndividuals,
        capacity_exhaustion: CapacityExhaustionPolicy::PauseAtCommittedBoundary,
        max_events_per_partition_transition,
    };
    let configuration = match (origin_environment.as_ref(), origin_climate_normals.as_ref()) {
        (Some(environment), Some(weather))
            if ruleset_version >= LOCAL_WEATHER_RULESET_VERSION
                && environment.surface_baseline.is_some() =>
        {
            WorldConfiguration::new_provisional_full_earth_with_surface_baseline(
                tick_duration_seconds,
                composition.full_earth_grid.clone(),
                composition_reference.clone(),
                execution,
                environment.baseline.clone(),
                weather.baseline.clone(),
                environment
                    .surface_baseline
                    .clone()
                    .expect("match guard checked surface baseline"),
            )
        }
        (Some(environment), Some(weather)) if ruleset_version >= LOCAL_WEATHER_RULESET_VERSION => {
            WorldConfiguration::new_provisional_full_earth_with_weather_baseline(
                tick_duration_seconds,
                composition.full_earth_grid.clone(),
                composition_reference.clone(),
                execution,
                environment.baseline.clone(),
                weather.baseline.clone(),
            )
        }
        (Some(environment), _) => {
            WorldConfiguration::new_provisional_full_earth_with_environment_baseline(
                tick_duration_seconds,
                composition.full_earth_grid.clone(),
                composition_reference.clone(),
                execution,
                environment.baseline.clone(),
            )
        }
        (None, _) => WorldConfiguration::new_provisional_full_earth(
            tick_duration_seconds,
            composition.full_earth_grid.clone(),
            composition_reference.clone(),
            execution,
        ),
    }
    .context("construct provisional full-Earth execution configuration")?;
    if ruleset_version >= BODILY_REGULATION_RULESET_VERSION
        && configuration.local_environment_baseline().is_none()
    {
        anyhow::bail!(
            "ruleset {ruleset_version} requires --provisional-origin-environment for canonical bodily regulation"
        );
    }
    if ruleset_version >= LOCAL_WEATHER_RULESET_VERSION
        && configuration.local_weather_baseline().is_none()
    {
        anyhow::bail!(
            "ruleset {ruleset_version} requires complete --provisional-origin-climate-evidence and --provisional-origin-climate-normals"
        );
    }

    let mut manifest = WorldManifest::new(world_id, WorldSeed::new(seed), ruleset_version);
    if cancer_research {
        manifest.experiment = Some(WorldExperimentCommitment::CancerResearch(
            CancerResearchBootstrap::english_literate_abundant_world(),
        ));
    }
    if let Some(selection_digest) = initial_origin.selection_digest {
        manifest.scientific_datasets.insert(
            "provisional_land_origin_selection".to_owned(),
            selection_digest.to_string(),
        );
    }
    if let Some(origin_environment) = origin_environment {
        manifest.scientific_datasets.insert(
            "provisional_origin_environment".to_owned(),
            origin_environment.digest.to_string(),
        );
    }
    if let Some(origin_climate_evidence) = origin_climate_evidence.as_ref() {
        manifest.scientific_datasets.insert(
            "provisional_origin_climate_evidence".to_owned(),
            origin_climate_evidence.digest.to_string(),
        );
    }
    if let Some(origin_climate_normals) = origin_climate_normals.as_ref() {
        manifest.scientific_datasets.insert(
            "provisional_origin_climate_normals".to_owned(),
            origin_climate_normals.digest.to_string(),
        );
    }
    let mut initial_organisms = if cancer_research {
        cancer_research_human_founders(world_id, initial_patch, tick_duration_seconds)?
    } else {
        provisional_human_founders(world_id, initial_patch)?
    };
    let fauna = load_provisional_fauna_initial_organisms(
        world_id,
        WorldSeed::new(seed),
        initial_patch,
        ProvisionalFaunaInputPaths {
            candidates_path: fauna_range_candidates_path,
            selection_path: fauna_seeded_selection_path,
            origin_environment_path: fauna_origin_environment_path,
            population_plan_path: fauna_population_plan_path,
            metabolic_profile_set_path: fauna_metabolic_profile_set_path,
            metabolic_rate_plan_path: fauna_metabolic_rate_plan_path,
        },
    )?;
    if let Some(fauna) = fauna {
        manifest.scientific_datasets.insert(
            "inaturalist_fauna_range_candidate_set".to_owned(),
            fauna.candidate_set_digest.to_string(),
        );
        if let Some(local_occurrence_evidence_digest) = fauna.local_occurrence_evidence_digest {
            manifest.scientific_datasets.insert(
                "local_fauna_occurrence_evidence".to_owned(),
                local_occurrence_evidence_digest.to_string(),
            );
        }
        manifest.scientific_datasets.insert(
            "provisional_fauna_seeded_selection".to_owned(),
            fauna.selection_digest.to_string(),
        );
        manifest.scientific_datasets.insert(
            "provisional_origin_environment".to_owned(),
            fauna.origin_environment_digest.to_string(),
        );
        manifest.scientific_datasets.insert(
            "provisional_fauna_population_plan".to_owned(),
            fauna.population_plan_digest.to_string(),
        );
        if let Some(metabolic_rate_plan_digest) = fauna.metabolic_rate_plan_digest {
            manifest.scientific_datasets.insert(
                "provisional_fauna_metabolic_rate_plan".to_owned(),
                metabolic_rate_plan_digest.to_string(),
            );
        }
        if let Some(metabolic_profile_set_digest) = fauna.metabolic_profile_set_digest {
            manifest.scientific_datasets.insert(
                "provisional_fauna_metabolic_profile_set".to_owned(),
                metabolic_profile_set_digest.to_string(),
            );
        }
        initial_organisms.extend(fauna.initial_organisms);
    }
    if let Some(ecology) = load_provisional_fauna_ecology_evidence(
        fauna_ecology_profile_set_path,
        fauna_ecology_plan_path,
        &initial_organisms,
    )? {
        manifest.scientific_datasets.insert(
            "provisional_fauna_ecology_plan".to_owned(),
            ecology.plan_digest.to_string(),
        );
        manifest.scientific_datasets.insert(
            "provisional_fauna_ecology_profile_set".to_owned(),
            ecology.profile_set_digest.to_string(),
        );
    }
    if let Some(evidence) = apply_provisional_organism_body_profiles(
        &mut initial_organisms,
        ruleset_version,
        tick_duration_seconds,
        provisional_organism_profile_plan_path,
    )? {
        manifest.scientific_datasets.insert(
            "provisional_organism_body_profile_plan".to_owned(),
            evidence.profile_plan_digest.to_string(),
        );
        let life_history_count = evidence.life_history_profile_set_digests.len();
        for (ordinal, digest) in evidence
            .life_history_profile_set_digests
            .into_iter()
            .enumerate()
        {
            let key = if life_history_count == 1 {
                "provisional_fauna_life_history_profile_set".to_owned()
            } else {
                format!("provisional_fauna_life_history_profile_set_{ordinal:03}")
            };
            manifest.scientific_datasets.insert(key, digest.to_string());
        }
        let body_mass_count = evidence.body_mass_profile_set_digests.len();
        for (ordinal, digest) in evidence
            .body_mass_profile_set_digests
            .into_iter()
            .enumerate()
        {
            let key = if body_mass_count == 1 {
                "provisional_fauna_body_mass_profile_set".to_owned()
            } else {
                format!("provisional_fauna_body_mass_profile_set_{ordinal:03}")
            };
            manifest.scientific_datasets.insert(key, digest.to_string());
        }
    }
    let (material_resource_plan_digest, initial_materials) = load_provisional_material_resources(
        world_id,
        WorldSeed::new(seed),
        ruleset_version,
        tick_duration_seconds,
        initial_patch,
        provisional_material_resource_plan_path,
        provisional_organism_profile_plan_path,
        fauna_population_plan_path,
        fauna_origin_environment_path,
    )?;
    if let Some(material_resource_plan_digest) = material_resource_plan_digest {
        manifest.scientific_datasets.insert(
            "provisional_material_resource_plan".to_owned(),
            material_resource_plan_digest.to_string(),
        );
    }
    println!(
        "verified {} provisional references ({} bytes)",
        verified.artifacts, verified.bytes
    );
    if let Some(store) = store {
        let session = initialize_or_resume_configured_world_with_materials(
            store,
            manifest,
            predecessor_world_id,
            configuration,
            initial_organisms,
            initial_materials,
        )
        .await
        .context("initialize provisional full-Earth world")?;
        let world_kind = if cancer_research {
            "Cancer World research experiment"
        } else {
            "provisional full-Earth world"
        };
        println!(
            "initialized {world_kind} {world_id} from {}@{}",
            composition_reference.composition_id, composition_reference.composition_version
        );
        println!("status: provisional-not-scientifically-admitted");
        println!("composition hash: {}", composition_reference.content_hash);
        println!(
            "sequence {}, tick {}, state {}",
            session.world.cursor.sequence,
            session.world.cursor.tick,
            session.world.cursor.state_hash
        );
    } else {
        let organism_count = initial_organisms.len();
        let material_count = initial_materials.len();
        let scientific_dataset_count = manifest.scientific_datasets.len();
        let genesis = construct_configured_genesis_with_materials(
            manifest.clone(),
            configuration,
            initial_organisms,
            initial_materials,
        )
        .context("construct database-free provisional full-Earth genesis")?;
        let batches = vec![genesis.batch.clone()];
        let complete = replay(manifest, &batches).context("replay genesis from event zero")?;
        let from_snapshot = replay_from_snapshot(&genesis.snapshot, &[])
            .context("replay genesis snapshot with an empty tail")?;
        if complete != from_snapshot
            || complete.state != genesis.state
            || genesis.batch.post_state_hash != genesis.snapshot.state_hash
        {
            anyhow::bail!("database-free genesis replay paths disagree");
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "mode": if cancer_research {
                    "database-free-cancer-world-genesis-proof"
                } else {
                    "database-free-canonical-genesis-proof"
                },
                "cancer_research": cancer_research,
                "world_id": world_id,
                "seed": seed,
                "ruleset_version": ruleset_version,
                "composition": {
                    "id": composition_reference.composition_id,
                    "version": composition_reference.composition_version,
                    "content_hash": composition_reference.content_hash,
                },
                "verified_artifacts": verified.artifacts,
                "verified_artifact_bytes": verified.bytes,
                "scientific_dataset_commitments": scientific_dataset_count,
                "organisms": organism_count,
                "material_instances": material_count,
                "event_count": genesis.batch.events.len(),
                "sequence": genesis.batch.sequence,
                "tick": genesis.batch.tick,
                "batch_hash": genesis.batch.batch_hash,
                "state_hash": genesis.batch.post_state_hash,
                "snapshot_schema_version": genesis.snapshot.snapshot_schema_version,
                "genesis_replay_matches_snapshot": true,
            }))
            .context("encode database-free genesis proof")?
        );
    }
    Ok(())
}

struct ResolvedInitialOrigin {
    patch: S2CellId,
    selection_digest: Option<world_domain::Digest>,
}

struct VerifiedProvisionalOriginEnvironment {
    digest: world_domain::Digest,
    baseline: ProvisionalLocalEnvironmentBaseline,
    surface_baseline: Option<ProvisionalLocalSurfaceBaseline>,
}

struct VerifiedProvisionalOriginClimateEvidence {
    digest: Digest,
    selected_patch: S2CellId,
}

struct VerifiedProvisionalOriginClimateNormals {
    digest: Digest,
    baseline: ProvisionalLocalWeatherBaseline,
}

fn load_provisional_origin_climate_evidence(
    path: Option<&std::path::Path>,
    origin: &ResolvedInitialOrigin,
) -> Result<Option<VerifiedProvisionalOriginClimateEvidence>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let selection_digest = origin.selection_digest.context(
        "--provisional-origin-climate-evidence requires --provisional-land-origin-selection",
    )?;
    let bytes = fs::read(path).with_context(|| {
        format!(
            "read provisional origin climate evidence {}",
            path.display()
        )
    })?;
    let evidence = ProvisionalOriginClimateEvidence::from_canonical_slice(&bytes)
        .context("validate provisional origin climate evidence")?;
    if evidence.origin_selection_digest != selection_digest
        || !evidence.selected_patch.contains(origin.patch)
    {
        anyhow::bail!("provisional origin climate evidence does not match the selected origin");
    }
    Ok(Some(VerifiedProvisionalOriginClimateEvidence {
        digest: Digest::sha256(&bytes),
        selected_patch: evidence.selected_patch,
    }))
}

fn load_provisional_origin_climate_normals(
    path: Option<&std::path::Path>,
    evidence: Option<&VerifiedProvisionalOriginClimateEvidence>,
    origin: &ResolvedInitialOrigin,
) -> Result<Option<VerifiedProvisionalOriginClimateNormals>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let evidence = evidence.context(
        "--provisional-origin-climate-normals requires --provisional-origin-climate-evidence",
    )?;
    let bytes = fs::read(path)
        .with_context(|| format!("read provisional origin climate normals {}", path.display()))?;
    let normals = ProvisionalOriginClimateNormals::from_canonical_slice(&bytes)
        .context("validate provisional origin climate normals")?;
    if normals.origin_climate_evidence_digest != evidence.digest {
        anyhow::bail!("provisional origin climate normals do not match the supplied evidence");
    }
    let digest = Digest::sha256(&bytes);
    let series = |variable: &str| {
        normals
            .series
            .iter()
            .find(|series| series.variable == variable)
            .with_context(|| format!("climate normals are missing {variable}"))
    };
    let values = |variable: &str,
                  select: fn(&world_data::OriginClimateNormalMonth) -> Option<i64>|
     -> Result<[i64; 12]> {
        let series = series(variable)?;
        let complete = series
            .months
            .iter()
            .map(|month| {
                select(month).with_context(|| {
                    format!(
                        "climate normals {variable} month {} is missing and cannot drive weather",
                        month.month
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        complete.try_into().map_err(|values: Vec<i64>| {
            anyhow::anyhow!(
                "climate normals {variable} require 12 months, found {}",
                values.len()
            )
        })
    };
    let t2m = series("t2m")?;
    let precipitation = series("tp")?;
    let eastward_wind = series("u10")?;
    let northward_wind = series("v10")?;
    let baseline = ProvisionalLocalWeatherBaseline {
        status: "provisional-weather-input-not-scientifically-admitted".to_owned(),
        source_normals_digest: digest,
        evidence_patch: evidence.selected_patch,
        active_patch: origin.patch,
        air_temperature_unit: t2m.unit.clone(),
        air_temperature_decimal_places: t2m.decimal_places,
        air_temperature_normal_minimum: values("t2m", |month| month.minimum)?,
        air_temperature_normal_mean: values("t2m", |month| month.mean)?,
        air_temperature_normal_maximum: values("t2m", |month| month.maximum)?,
        precipitation_unit: precipitation.unit.clone(),
        precipitation_decimal_places: precipitation.decimal_places,
        precipitation_normal_mean: values("tp", |month| month.mean)?,
        eastward_wind_unit: eastward_wind.unit.clone(),
        eastward_wind_decimal_places: eastward_wind.decimal_places,
        eastward_wind_normal_mean: values("u10", |month| month.mean)?,
        northward_wind_unit: northward_wind.unit.clone(),
        northward_wind_decimal_places: northward_wind.decimal_places,
        northward_wind_normal_mean: values("v10", |month| month.mean)?,
    };
    baseline
        .validate()
        .context("validate causal provisional local-weather baseline")?;
    Ok(Some(VerifiedProvisionalOriginClimateNormals {
        digest,
        baseline,
    }))
}

fn load_provisional_origin_environment(
    path: Option<&std::path::Path>,
    composition: &world_data::ProvisionalWorldComposition,
    origin: &ResolvedInitialOrigin,
) -> Result<Option<VerifiedProvisionalOriginEnvironment>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let selection_digest = origin.selection_digest.context(
        "--provisional-origin-environment requires --provisional-land-origin-selection rather than an explicit patch",
    )?;
    let bytes = std::fs::read(path).context("read provisional origin environment")?;
    let environment = ProvisionalOriginEnvironment::from_canonical_slice(&bytes)
        .context("validate provisional origin environment")?;
    if environment.selected_embodied_patch != origin.patch
        || environment.origin_selection_digest != selection_digest
        || environment.composition_digest
            != composition
                .content_digest()
                .context("hash provisional composition for origin environment")?
    {
        anyhow::bail!(
            "provisional origin environment does not match the selected origin and composition"
        );
    }
    let source_evidence_digest = world_domain::Digest::sha256(&bytes);
    let surface_baseline =
        environment
            .local_surface
            .as_ref()
            .map(|surface| ProvisionalLocalSurfaceBaseline {
                status: "provisional-surface-input-not-scientifically-admitted".to_owned(),
                source_evidence_digest,
                evidence_patch: environment.selected_l10_patch,
                active_patch: environment.selected_embodied_patch,
                terrain_minimum_millimetres: surface.terrain.minimum_millimetres,
                terrain_mean_millimetres: surface.terrain.mean_millimetres,
                terrain_maximum_millimetres: surface.terrain.maximum_millimetres,
                surface_water_occurrence_source_code: u8::try_from(
                    surface.surface_water.mean_value,
                )
                .expect("origin surface validation bounds source code to u8"),
                topsoil_source_quantiles: surface
                    .topsoil
                    .property_values
                    .map(|values| [values.q0_05, values.q0_5, values.q0_95]),
            });
    Ok(Some(VerifiedProvisionalOriginEnvironment {
        digest: source_evidence_digest,
        baseline: ProvisionalLocalEnvironmentBaseline {
            status: "provisional-evidence-only".to_owned(),
            source_evidence_digest,
            evidence_patch: environment.selected_l10_patch,
            active_patch: environment.selected_embodied_patch,
            air_temperature_unit: environment.air_temperature_normal_unit,
            air_temperature_decimal_places: environment.air_temperature_normal_decimal_places,
            air_temperature_normal_minimum: environment
                .air_temperature_normal
                .minimum_values
                .try_into()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "origin environment must contain twelve minimum temperature phases"
                    )
                })?,
            air_temperature_normal_mean: environment
                .air_temperature_normal
                .mean_values
                .try_into()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "origin environment must contain twelve mean temperature phases"
                    )
                })?,
            air_temperature_normal_maximum: environment
                .air_temperature_normal
                .maximum_values
                .try_into()
                .map_err(|_| {
                    anyhow::anyhow!(
                        "origin environment must contain twelve maximum temperature phases"
                    )
                })?,
        },
        surface_baseline,
    }))
}

struct AppliedFaunaEcologyEvidence {
    plan_digest: Digest,
    profile_set_digest: Digest,
}

fn load_provisional_fauna_ecology_evidence(
    profile_set_path: Option<&std::path::Path>,
    plan_path: Option<&std::path::Path>,
    initial_organisms: &[InitialOrganism],
) -> Result<Option<AppliedFaunaEcologyEvidence>> {
    let (profile_set_path, plan_path) = match (profile_set_path, plan_path) {
        (None, None) => return Ok(None),
        (Some(profile_set_path), Some(plan_path)) => (profile_set_path, plan_path),
        _ => anyhow::bail!(
            "fauna ecology profile set and ecology plan must be supplied together or both omitted"
        ),
    };
    let profile_bytes = fs::read(profile_set_path)
        .with_context(|| format!("read fauna ecology profiles {}", profile_set_path.display()))?;
    let profiles = FaunaPhysiologyProfileSet::from_canonical_slice(&profile_bytes)
        .context("validate fauna ecology profile set")?;
    let plan_bytes = fs::read(plan_path)
        .with_context(|| format!("read fauna ecology plan {}", plan_path.display()))?;
    let plan = FaunaEcologyPlan::from_canonical_slice(&plan_bytes)
        .context("validate fauna ecology plan")?;
    plan.resolve(&profiles)
        .context("resolve exact fauna ecology source rows")?;
    let fauna_species = initial_organisms
        .iter()
        .filter(|organism| organism.role == OrganismRole::Fauna)
        .map(|organism| {
            (
                organism.species.catalog.as_str(),
                organism.species.identifier.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    for entry in &plan.entries {
        if !fauna_species.contains(&(
            entry.species.catalog.as_str(),
            entry.species.identifier.as_str(),
        )) {
            anyhow::bail!(
                "fauna ecology plan contains unplanned species {}",
                entry.species.scientific_name
            );
        }
    }
    Ok(Some(AppliedFaunaEcologyEvidence {
        plan_digest: Digest::sha256(&plan_bytes),
        profile_set_digest: Digest::sha256(&profile_bytes),
    }))
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
    local_occurrence_evidence_digest: Option<world_domain::Digest>,
    selection_digest: world_domain::Digest,
    origin_environment_digest: world_domain::Digest,
    population_plan_digest: world_domain::Digest,
    metabolic_rate_plan_digest: Option<world_domain::Digest>,
    metabolic_profile_set_digest: Option<world_domain::Digest>,
    initial_organisms: Vec<InitialOrganism>,
}

#[derive(Clone, Copy)]
struct ProvisionalFaunaInputPaths<'a> {
    candidates_path: Option<&'a std::path::Path>,
    selection_path: Option<&'a std::path::Path>,
    origin_environment_path: Option<&'a std::path::Path>,
    population_plan_path: Option<&'a std::path::Path>,
    metabolic_profile_set_path: Option<&'a std::path::Path>,
    metabolic_rate_plan_path: Option<&'a std::path::Path>,
}

fn load_provisional_fauna_initial_organisms(
    world_id: WorldId,
    world_seed: WorldSeed,
    initial_patch: S2CellId,
    inputs: ProvisionalFaunaInputPaths<'_>,
) -> Result<Option<ProvisionalFaunaGenesis>> {
    let ProvisionalFaunaInputPaths {
        candidates_path,
        selection_path,
        origin_environment_path,
        population_plan_path,
        metabolic_profile_set_path,
        metabolic_rate_plan_path,
    } = inputs;
    let provided = [
        candidates_path.is_some(),
        selection_path.is_some(),
        origin_environment_path.is_some(),
        population_plan_path.is_some(),
        metabolic_profile_set_path.is_some(),
        metabolic_rate_plan_path.is_some(),
    ];
    if provided[..4].iter().all(|provided| !provided)
        && provided[4..].iter().all(|provided| !provided)
    {
        return Ok(None);
    }
    if !provided[..4].iter().all(|provided| *provided) {
        anyhow::bail!(
            "provisional fauna genesis requires --fauna-range-candidates, --fauna-seeded-selection, --fauna-origin-environment, and --fauna-population-plan together"
        );
    }
    if metabolic_profile_set_path.is_some() != metabolic_rate_plan_path.is_some() {
        anyhow::bail!(
            "provisional fauna metabolic commitments require --fauna-metabolic-profile-set and --fauna-metabolic-rate-plan together"
        );
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
    let environment_bytes =
        std::fs::read(origin_environment_path.expect("checked origin-environment path"))
            .context("read provisional fauna origin environment")?;
    let environment = ProvisionalOriginEnvironment::from_canonical_slice(&environment_bytes)
        .context("validate provisional fauna origin environment")?;
    let population_plan_bytes = std::fs::read(population_plan_path.expect("checked plan path"))
        .context("read provisional fauna population plan")?;
    let population_plan = FaunaPopulationPlan::from_canonical_slice_against_environment(
        &population_plan_bytes,
        &candidates,
        &selection,
        &environment,
    )
    .context("validate provisional fauna population plan")?;
    if population_plan.world_seed != world_seed {
        anyhow::bail!("provisional fauna population plan world seed does not match the world seed");
    }
    if population_plan.embodied_patch != initial_patch {
        anyhow::bail!("provisional fauna population plan targets another embodied patch");
    }
    let candidate_set_digest = world_domain::Digest::sha256(&candidate_bytes);
    let local_occurrence_evidence_digest = candidates.source_local_occurrence_evidence_digest;
    let selection_digest = world_domain::Digest::sha256(&selection_bytes);
    let origin_environment_digest = world_domain::Digest::sha256(&environment_bytes);
    let population_plan_digest = world_domain::Digest::sha256(&population_plan_bytes);
    let (metabolic_rate_plan, metabolic_rate_plan_digest, metabolic_profile_set_digest) =
        match (metabolic_profile_set_path, metabolic_rate_plan_path) {
            (None, None) => (None, None, None),
            (Some(profile_set_path), Some(plan_path)) => {
                let profile_set_bytes =
                    std::fs::read(profile_set_path).context("read fauna metabolic profile set")?;
                let profile_set =
                    FaunaPhysiologyProfileSet::from_canonical_slice(&profile_set_bytes)
                        .context("validate fauna metabolic profile set")?;
                let plan_bytes =
                    std::fs::read(plan_path).context("read fauna metabolic rate plan")?;
                let plan = FaunaMetabolicRatePlan::from_canonical_slice(&plan_bytes)
                    .context("validate fauna metabolic rate plan")?;
                for entry in &population_plan.entries {
                    let selection = plan.selection_for(&entry.species).ok_or_else(|| {
                        anyhow::anyhow!(
                            "fauna metabolic rate plan has no selection for {}",
                            entry.species.scientific_name
                        )
                    })?;
                    selection.resolve(&profile_set).with_context(|| {
                        format!(
                            "resolve fauna metabolic rate for {}",
                            entry.species.scientific_name
                        )
                    })?;
                }
                (
                    Some((profile_set, plan)),
                    Some(world_domain::Digest::sha256(&plan_bytes)),
                    Some(world_domain::Digest::sha256(&profile_set_bytes)),
                )
            }
            _ => unreachable!("metabolic option pair checked above"),
        };
    let capacity = population_plan
        .entries
        .iter()
        .map(|entry| usize::try_from(entry.initial_individual_count))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .try_fold(0_usize, |total, count| total.checked_add(count))
        .context("provisional fauna genesis count overflows host capacity")?;
    let mut initial_organisms = Vec::with_capacity(capacity);
    for entry in population_plan.entries {
        let metabolic_rate = metabolic_rate_plan.as_ref().map(|(profiles, plan)| {
            plan.selection_for(&entry.species)
                .expect("all planned fauna have a verified metabolic selection")
                .resolve_commitment(profiles)
                .expect("previously verified metabolic selection remains valid")
        });
        let category_counts = if entry.birth_category_counts.is_empty() {
            vec![(
                BirthCategory::new("unspecified")?,
                entry.initial_individual_count,
            )]
        } else {
            entry
                .birth_category_counts
                .iter()
                .map(|category_count| (category_count.category.clone(), category_count.count))
                .collect()
        };
        let mut ordinal = 0_u32;
        for (birth_category, count) in category_counts {
            for _ in 0..count {
                let identity =
                    format!("provisional-fauna:{}:{}", entry.species.identifier, ordinal);
                initial_organisms.push(InitialOrganism {
                    organism_id: EntityId::deterministic(world_id, identity.as_bytes()),
                    species: entry.species.clone(),
                    role: OrganismRole::Fauna,
                    birth_category: birth_category.clone(),
                    initial_age_ticks: 0,
                    location_id: None,
                    embodied_patch: Some(initial_patch),
                    metabolic_rate: metabolic_rate.clone(),
                    adult_body_mass: None,
                    physiological_regulation: None,
                    reproductive_physiology: None,
                    heritable_disposition_profile: None,
                });
                ordinal = ordinal
                    .checked_add(1)
                    .context("provisional fauna founder ordinal overflow")?;
            }
        }
    }
    Ok(Some(ProvisionalFaunaGenesis {
        candidate_set_digest,
        local_occurrence_evidence_digest,
        selection_digest,
        origin_environment_digest,
        population_plan_digest,
        metabolic_rate_plan_digest,
        metabolic_profile_set_digest,
        initial_organisms,
    }))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppliedBodyProfileEvidence {
    profile_plan_digest: Digest,
    life_history_profile_set_digests: Vec<Digest>,
    body_mass_profile_set_digests: Vec<Digest>,
}

fn apply_provisional_organism_body_profiles(
    initial_organisms: &mut [InitialOrganism],
    ruleset_version: u32,
    tick_duration_seconds: u32,
    plan_path: Option<&std::path::Path>,
) -> Result<Option<AppliedBodyProfileEvidence>> {
    if ruleset_version < BODILY_REGULATION_RULESET_VERSION {
        if plan_path.is_some() {
            anyhow::bail!(
                "--provisional-organism-profile-plan requires ruleset {BODILY_REGULATION_RULESET_VERSION} or later"
            );
        }
        return Ok(None);
    }

    let plan_path = plan_path.with_context(|| {
        format!("ruleset {ruleset_version} requires --provisional-organism-profile-plan")
    })?;
    let bytes = std::fs::read(plan_path).with_context(|| {
        format!(
            "read provisional organism body-profile plan {}",
            plan_path.display()
        )
    })?;
    let plan = ProvisionalOrganismBodyProfilePlan::from_canonical_slice(&bytes)
        .context("validate provisional organism body-profile plan")?;
    if plan.tick_duration_seconds != tick_duration_seconds {
        anyhow::bail!(
            "provisional organism body-profile plan tick duration {} does not match world tick duration {tick_duration_seconds}",
            plan.tick_duration_seconds
        );
    }

    let life_history_digests = plan
        .entries
        .iter()
        .flat_map(|entry| &entry.reproductive_physiology.category_maturity)
        .filter(|entry| {
            entry.evidence_basis != world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption
        })
        .map(|entry| entry.source_profile_set_digest)
        .collect::<BTreeSet<_>>();
    if ruleset_version < ADULT_BODY_MASS_STATE_RULESET_VERSION && life_history_digests.len() > 1 {
        anyhow::bail!("one body-profile plan cannot mix multiple life-history profile sets");
    }
    let body_mass_digests = plan
        .entries
        .iter()
        .filter_map(|entry| entry.adult_body_mass.as_ref())
        .filter(|entry| {
            entry.evidence_basis != world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption
        })
        .map(|entry| entry.profile_set_digest)
        .collect::<BTreeSet<_>>();
    if ruleset_version < ADULT_BODY_MASS_STATE_RULESET_VERSION && body_mass_digests.len() > 1 {
        anyhow::bail!("one body-profile plan cannot mix multiple body-mass profile sets");
    }

    for organism in initial_organisms {
        let profile = plan.entry_for(&organism.species).with_context(|| {
            format!(
                "provisional organism body-profile plan has no entry for {}:{} ({})",
                organism.species.catalog,
                organism.species.identifier,
                organism.species.scientific_name
            )
        })?;
        if organism
            .metabolic_rate
            .as_ref()
            .is_some_and(|existing| existing != &profile.metabolic_rate)
        {
            anyhow::bail!(
                "provisional organism body-profile plan conflicts with the retained metabolic rate for {}:{}",
                organism.species.catalog,
                organism.species.identifier
            );
        }
        if ruleset_version >= REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION
            && !profile
                .reproductive_physiology
                .supports_category(&organism.birth_category)
        {
            anyhow::bail!(
                "provisional reproductive profile for {}:{} does not support founder category {:?}",
                organism.species.catalog,
                organism.species.identifier,
                organism.birth_category
            );
        }
        organism.initial_age_ticks = profile.initial_age_ticks;
        organism.metabolic_rate = Some(profile.metabolic_rate.clone());
        organism.adult_body_mass = (ruleset_version
            >= ADULT_BODY_MASS_STATE_RULESET_VERSION)
            .then(|| {
                profile.adult_body_mass.clone().with_context(|| {
                    format!(
                        "ruleset {ruleset_version} requires an adult-body-mass commitment for {}:{}",
                        organism.species.catalog, organism.species.identifier
                    )
                })
            })
            .transpose()?;
        organism.physiological_regulation = Some(profile.physiological_regulation.clone());
        organism.reproductive_physiology = (ruleset_version
            >= REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION)
            .then(|| profile.reproductive_physiology.clone());
        organism.heritable_disposition_profile = if ruleset_version
            >= HERITABLE_DISPOSITION_RULESET_VERSION
        {
            Some(
                profile
                    .heritable_disposition_profile
                    .clone()
                    .with_context(|| {
                        format!(
                            "ruleset {ruleset_version} requires a heritable-disposition profile for {}:{}",
                            organism.species.catalog, organism.species.identifier
                        )
                    })?,
            )
        } else {
            None
        };
    }

    Ok(Some(AppliedBodyProfileEvidence {
        profile_plan_digest: Digest::sha256(&bytes),
        life_history_profile_set_digests: life_history_digests.into_iter().collect(),
        body_mass_profile_set_digests: body_mass_digests.into_iter().collect(),
    }))
}

#[allow(clippy::too_many_arguments)]
fn load_provisional_material_resources(
    world_id: WorldId,
    world_seed: WorldSeed,
    ruleset_version: u32,
    tick_duration_seconds: u32,
    initial_patch: S2CellId,
    resource_plan_path: Option<&std::path::Path>,
    body_profile_plan_path: Option<&std::path::Path>,
    fauna_population_plan_path: Option<&std::path::Path>,
    origin_environment_path: Option<&std::path::Path>,
) -> Result<(Option<Digest>, Vec<InitialMaterialInstance>)> {
    if ruleset_version < MATERIAL_RESERVOIR_RULESET_VERSION {
        if resource_plan_path.is_some() {
            anyhow::bail!(
                "--provisional-material-resource-plan requires ruleset {MATERIAL_RESERVOIR_RULESET_VERSION} or later"
            );
        }
        return Ok((None, Vec::new()));
    }
    let resource_plan_path = resource_plan_path.with_context(|| {
        format!("ruleset {ruleset_version} requires --provisional-material-resource-plan")
    })?;
    let body_profile_plan_path = body_profile_plan_path
        .context("material-resource validation requires --provisional-organism-profile-plan")?;
    let fauna_population_plan_path = fauna_population_plan_path
        .context("material-resource validation requires --fauna-population-plan")?;
    let origin_environment_path = origin_environment_path
        .context("material-resource validation requires --fauna-origin-environment")?;

    let body_bytes = std::fs::read(body_profile_plan_path)
        .context("read body-profile plan for material-resource validation")?;
    let body_profiles = ProvisionalOrganismBodyProfilePlan::from_canonical_slice(&body_bytes)
        .context("validate body-profile plan for material resources")?;
    let population_bytes = std::fs::read(fauna_population_plan_path)
        .context("read fauna population plan for material-resource validation")?;
    let environment_bytes = std::fs::read(origin_environment_path)
        .context("read origin environment for material-resource validation")?;
    let resource_bytes = std::fs::read(resource_plan_path).with_context(|| {
        format!(
            "read provisional material-resource plan {}",
            resource_plan_path.display()
        )
    })?;
    let plan =
        ProvisionalMaterialResourcePlan::from_canonical_slice(&resource_bytes, &body_profiles)
            .context("validate provisional material-resource plan")?;
    if plan.world_seed != world_seed
        || plan.tick_duration_seconds != tick_duration_seconds
        || plan.embodied_patch != initial_patch
        || plan.origin_environment_digest != Digest::sha256(&environment_bytes)
        || plan.fauna_population_plan_digest != Digest::sha256(&population_bytes)
        || plan.organism_body_profile_plan_digest != Digest::sha256(&body_bytes)
    {
        anyhow::bail!(
            "provisional material-resource plan does not match this world seed, origin, population, body plan, patch, or tick duration"
        );
    }

    let initial_materials = plan
        .sources
        .into_iter()
        .map(|source| InitialMaterialInstance {
            object_id: EntityId::deterministic(
                world_id,
                format!("provisional-material-source:{}", source.source_id).as_bytes(),
            ),
            material: source.material,
            embodied_patch: source.anchor_patch,
            initial_mass_milligrams: Some(source.initial_mass_milligrams),
            oral_transfer_profiles: source.oral_transfer_profiles,
            reservoir: source.reservoir,
        })
        .collect();
    Ok((Some(Digest::sha256(&resource_bytes)), initial_materials))
}

async fn serve_memory_worker(
    store: &PostgresStore,
    memory: &HindsightMemory,
    worker_id: &str,
    poll_milliseconds: u64,
    claim_lease_seconds: u32,
    drain: bool,
) -> Result<()> {
    let heartbeat = ServiceHeartbeat {
        service_name: "memory-worker".to_owned(),
        instance_id: Uuid::new_v4(),
        metadata: json!({
            "worker_id": worker_id,
            "worker_version": env!("CARGO_PKG_VERSION"),
            "mode": "hindsight-delivery",
        }),
    };
    let effective_poll_milliseconds = if drain { 1 } else { poll_milliseconds.max(1) };
    let mut interval = tokio::time::interval(Duration::from_millis(effective_poll_milliseconds));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(10));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tracing::info!(
        worker_id,
        poll_milliseconds = effective_poll_milliseconds,
        claim_lease_seconds = claim_lease_seconds.max(1),
        drain,
        "subjective-memory delivery worker started"
    );

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                if let Err(error) = store.record_heartbeat(&heartbeat).await {
                    tracing::warn!(%error, "memory-worker heartbeat failed; will retry");
                }
            }
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
                                if drain {
                                    anyhow::bail!(
                                        "Hindsight drain stopped after delivery failure for {operation_id}: {error}"
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) if drain => {
                        tracing::info!(worker_id, "subjective-memory outbox drain complete");
                        break;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        if drain {
                            return Err(error).context("memory outbox drain could not claim work");
                        }
                        tracing::warn!(%error, "memory outbox unavailable; will retry");
                    }
                }
            }
            _ = shutdown_signal() => {
                tracing::info!(worker_id, "subjective-memory delivery worker stopping");
                break;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)] // One explicit option per independently configured provider.
fn cognition_adapters(
    local_base_url: Option<String>,
    cloudflare_base_url: Option<String>,
    cloudflare_api_key: Option<String>,
    groq_api_key: Option<String>,
    cerebras_api_key: Option<String>,
    openrouter_api_key: Option<String>,
    cancer_openrouter_api_key: Option<String>,
    timeout: Duration,
) -> Result<BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>> {
    let mut adapters = BTreeMap::<CognitionProviderId, Arc<dyn CognitionModel>>::new();
    if let Some(base_url) = nonempty(local_base_url) {
        validate_local_cognition_base_url(&base_url)?;
        insert_cognition_adapter(
            &mut adapters,
            CognitionProviderId::local_openai(),
            &base_url,
            "loopback-only".to_owned(),
            timeout,
        )?;
    }
    match (nonempty(cloudflare_base_url), nonempty(cloudflare_api_key)) {
        (Some(base_url), Some(api_key)) => insert_cognition_adapter(
            &mut adapters,
            CognitionProviderId::cloudflare_workers_ai(),
            &base_url,
            api_key,
            timeout,
        )?,
        (None, None) => {}
        _ => anyhow::bail!(
            "Cloudflare Workers AI requires both its account-scoped base URL and API key"
        ),
    }
    if let Some(api_key) = nonempty(groq_api_key) {
        insert_cognition_adapter(
            &mut adapters,
            CognitionProviderId::groq(),
            "https://api.groq.com/openai/v1",
            api_key,
            timeout,
        )?;
    }
    if let Some(api_key) = nonempty(cerebras_api_key) {
        insert_cognition_adapter(
            &mut adapters,
            CognitionProviderId::cerebras(),
            "https://api.cerebras.ai/v1",
            api_key,
            timeout,
        )?;
    }
    if let Some(api_key) = nonempty(openrouter_api_key) {
        insert_cognition_adapter(
            &mut adapters,
            CognitionProviderId::openrouter(),
            "https://openrouter.ai/api/v1",
            api_key,
            timeout,
        )?;
    }
    if let Some(api_key) = nonempty(cancer_openrouter_api_key) {
        insert_cognition_adapter(
            &mut adapters,
            CognitionProviderId::openrouter_cancer(),
            "https://openrouter.ai/api/v1",
            api_key,
            timeout,
        )?;
    }
    Ok(adapters)
}

fn validate_local_cognition_base_url(base_url: &str) -> Result<()> {
    let url = Url::parse(base_url).context("parse local cognition base URL")?;
    let host = url.host_str();
    let loopback = matches!(host, Some("127.0.0.1" | "[::1]" | "::1" | "localhost"));
    let compose_service = host == Some("local-cognition") && url.port() == Some(11434);
    if url.scheme() != "http"
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (!loopback && !compose_service)
    {
        anyhow::bail!(
            "LOCAL_COGNITION_BASE_URL must be uncredentialed loopback HTTP or the exact private Compose service URL"
        );
    }
    Ok(())
}

fn insert_cognition_adapter(
    adapters: &mut BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>,
    provider: CognitionProviderId,
    base_url: &str,
    api_key: String,
    timeout: Duration,
) -> Result<()> {
    let adapter = OpenAiCompatibleCognition::new(provider.clone(), base_url, api_key, timeout)
        .with_context(|| format!("configure {} cognition adapter", provider.as_str()))?;
    adapters.insert(provider, Arc::new(adapter));
    Ok(())
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn validate_cognition_export_approval(
    configured_providers: usize,
    external_export_approved: bool,
) -> Result<()> {
    if configured_providers > 0 && !external_export_approved {
        anyhow::bail!(
            "configured cognition providers require COGNITION_EXTERNAL_EXPORT_APPROVED=true"
        );
    }
    Ok(())
}

async fn serve_cognition_worker(
    store: &PostgresStore,
    memory: &HindsightMemory,
    adapters: &BTreeMap<CognitionProviderId, Arc<dyn CognitionModel>>,
    worker_id: &str,
    poll_milliseconds: u64,
    claim_lease_seconds: u32,
    configuration: &CognitionWorkerConfiguration,
) -> Result<()> {
    configuration
        .validate()
        .context("validate cognition worker configuration")?;
    let heartbeat = ServiceHeartbeat {
        service_name: "cognition-worker".to_owned(),
        instance_id: Uuid::new_v4(),
        metadata: json!({
            "worker_id": worker_id,
            "worker_version": env!("CARGO_PKG_VERSION"),
            "configured_providers": adapters.len(),
            "paid_enabled": configuration.paid_enabled,
            "mode": "replay-safe-cognition",
        }),
    };
    let mut interval = tokio::time::interval(Duration::from_millis(poll_milliseconds.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(10));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tracing::info!(
        worker_id,
        configured_providers = adapters.len(),
        routes = configuration.registry.routes.len(),
        paid_enabled = configuration.paid_enabled,
        "replay-safe cognition worker started"
    );
    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                if let Err(error) = store.record_heartbeat(&heartbeat).await {
                    tracing::warn!(%error, "cognition-worker heartbeat failed; will retry");
                }
            }
            _ = interval.tick() => {
                match process_next_cognition_job(
                    store,
                    memory,
                    adapters,
                    worker_id,
                    claim_lease_seconds,
                    configuration,
                ).await {
                    Ok(CognitionWorkerStep::Idle) => {}
                    Ok(CognitionWorkerStep::Completed { request_id, used_model }) => {
                        tracing::info!(%request_id, used_model, "cognition result prepared for its fixed deadline");
                    }
                    Ok(CognitionWorkerStep::DeadlineElapsed { request_id }) => {
                        tracing::info!(%request_id, "cognition deadline elapsed; immutable local fallback retained");
                    }
                    Err(error) => {
                        tracing::warn!(%error, "cognition job unavailable; deterministic local behavior remains active");
                    }
                }
            }
            _ = shutdown_signal() => {
                tracing::info!(worker_id, "cognition worker stopping");
                break;
            }
        }
    }
    Ok(())
}

async fn serve_cancer_research_worker(
    store: &PostgresStore,
    adapters: &CancerResearchModelAdapters,
    worker_id: &str,
    poll_milliseconds: u64,
    configuration: &CancerResearchWorkerConfiguration,
    drain: bool,
) -> Result<()> {
    configuration
        .validate()
        .context("validate Cancer World research worker configuration")?;
    let heartbeat = ServiceHeartbeat {
        service_name: "cancer-research-worker".to_owned(),
        instance_id: Uuid::new_v4(),
        metadata: json!({
            "worker_id": worker_id,
            "worker_version": env!("CARGO_PKG_VERSION"),
            "configured_providers": adapters.len(),
            "paid_enabled": configuration.paid_enabled,
            "paid_reservation_micro_usd": configuration.paid_reservation_micro_usd,
            "mode": "content-addressed-open-research",
        }),
    };
    let mut interval = tokio::time::interval(Duration::from_millis(poll_milliseconds.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(10));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Keep shutdown listening independently from the research request. A model
    // call may be in flight when SIGTERM arrives; the watch value remembers the
    // signal so the worker finishes that durable turn and exits before claiming
    // another instead of losing the signal while this loop branch is awaited.
    let (shutdown_sender, mut shutdown_receiver) = tokio::sync::watch::channel(false);
    let shutdown_listener = (!drain).then(|| {
        tokio::spawn(async move {
            shutdown_signal().await;
            let _ = shutdown_sender.send(true);
        })
    });
    tracing::info!(
        worker_id,
        configured_providers = adapters.len(),
        paid_enabled = configuration.paid_enabled,
        drain,
        "Cancer World research worker started"
    );
    loop {
        if !drain && *shutdown_receiver.borrow() {
            tracing::info!(worker_id, "Cancer World research worker stopping");
            break;
        }
        tokio::select! {
            _ = heartbeat_interval.tick(), if !drain => {
                if let Err(error) = store.record_heartbeat(&heartbeat).await {
                    tracing::warn!(%error, "Cancer World research-worker heartbeat failed; will retry");
                }
            }
            _ = interval.tick() => {
                match process_next_cancer_research_job(
                    store,
                    adapters,
                    worker_id,
                    configuration,
                ).await {
                    Ok(CancerResearchWorkerOutcome::Idle) if drain => break,
                    Ok(CancerResearchWorkerOutcome::Idle) => {}
                    Ok(CancerResearchWorkerOutcome::Completed { request_id, succeeded }) => {
                        tracing::info!(%request_id, succeeded, "Cancer World research turn finalized");
                    }
                    Err(error) if drain => return Err(error).context("drain Cancer World research queue"),
                    Err(error) => {
                        tracing::warn!(%error, "Cancer World research turn failed; worker will continue");
                    }
                }
            }
            changed = shutdown_receiver.changed(), if !drain => {
                if changed.is_err() {
                    tracing::warn!(worker_id, "Cancer World research shutdown listener ended unexpectedly");
                }
                tracing::info!(worker_id, "Cancer World research worker stopping");
                break;
            }
        }
    }
    if let Some(listener) = shutdown_listener {
        listener.abort();
    }
    Ok(())
}

fn retry_delay_seconds(attempt_count: u32) -> u32 {
    let shift = attempt_count.saturating_sub(1).min(8);
    (1_u32 << shift).min(300)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut interrupt = match signal(SignalKind::interrupt()) {
            Ok(signal) => signal,
            Err(error) => {
                tracing::error!(%error, "failed to install interrupt signal handler");
                std::future::pending::<()>().await;
                unreachable!();
            }
        };
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(error) => {
                tracing::error!(%error, "failed to install termination signal handler");
                std::future::pending::<()>().await;
                unreachable!();
            }
        };
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install shutdown signal handler");
    }
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
        FAUNA_POPULATION_PLAN_SCHEMA_VERSION, FAUNA_RANGE_CANDIDATE_SET_SCHEMA_VERSION,
        FaunaBirthCategoryCount, FaunaEcologyPlan, FaunaEcologyPlanEntry,
        FaunaEcologyProfileSelection, FaunaEvidenceBasis, FaunaEvidenceSource,
        FaunaMetabolicRatePlan, FaunaMetabolicRateSelection, FaunaPhysiologyProfile,
        FaunaPhysiologyProfileSet, FaunaPopulationPlan, FaunaPopulationPlanEntry,
        FaunaRangeCandidate, FaunaRangeQueryPoint, LandCoverClassCount, LandCoverEvidenceCell,
        LandCoverSignedValueCount, OriginClimateNormalMonth, OriginClimateNormalSeries,
        OriginClimateSeries, OriginClimateSourceArtifact,
        PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_SCHEMA_VERSION,
        PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_STATUS,
        PROVISIONAL_ORIGIN_CLIMATE_NORMALS_SCHEMA_VERSION,
        PROVISIONAL_ORIGIN_CLIMATE_NORMALS_STATUS, ProvisionalOriginClimateEvidence,
        ProvisionalOriginClimateNormals, ProvisionalWorldComposition, ScaledFaunaTraitValue,
        SeasonalScalarFieldCell,
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
            source_local_occurrence_evidence_digest: None,
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

    fn provisional_body_profile_entry(
        species: SpeciesIdentity,
    ) -> world_data::ProvisionalOrganismBodyProfileEntry {
        let female = BirthCategory::new("female").expect("female category");
        let male = BirthCategory::new("male").expect("male category");
        world_data::ProvisionalOrganismBodyProfileEntry {
            species: species.clone(),
            initial_age_ticks: 20,
            metabolic_rate: world_domain::MetabolicRateCommitment {
                commitment_schema_version: world_domain::METABOLIC_RATE_COMMITMENT_SCHEMA_VERSION,
                evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
                profile_set_digest: Digest::sha256(b"runner body-profile test set"),
                observed_species: species.clone(),
                source_record_id: "runner-body-profile-test-rate".to_owned(),
                source_record_digest: Digest::sha256(b"runner body-profile test rate"),
                measured_power_value: 100,
                measured_power_decimal_places: 0,
            },
            physiological_regulation: world_domain::PhysiologicalRegulationCommitment {
                commitment_schema_version:
                    world_domain::PHYSIOLOGICAL_REGULATION_COMMITMENT_SCHEMA_VERSION,
                profile_id: "runner-body-profile-test-regulation".to_owned(),
                profile_digest: Digest::sha256(b"runner body-profile test regulation"),
                species: species.clone(),
                evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
                usable_energy_reserve_joules: 1_000_000,
                hydration_failure_seconds: 100_000,
                fatigue_failure_seconds: 10_000,
                fatigue_recovery_seconds: 10_000,
                thermoneutral_min_millicelsius: 18_000,
                thermoneutral_max_millicelsius: 26_000,
                thermal_failure_millicelsius_seconds: 100_000,
                thermal_recovery_seconds: 10_000,
            },
            reproductive_physiology: world_domain::ReproductivePhysiologyCommitment {
                commitment_schema_version:
                    world_domain::LEGACY_REPRODUCTIVE_PHYSIOLOGY_COMMITMENT_SCHEMA_VERSION,
                profile_id: "runner-body-profile-test-reproduction".to_owned(),
                profile_digest: Digest::sha256(b"runner body-profile test reproduction"),
                species: species.clone(),
                evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
                tick_duration_seconds: 300,
                maturity_age_ticks: 10,
                category_maturity: Vec::new(),
                development_ticks: 2,
                recovery_ticks: 2,
                opportunity_interval_ticks: 1,
                initiation_probability_millionths: world_domain::REPRODUCTIVE_PROBABILITY_SCALE,
                compatible_pairs: vec![world_domain::ReproductiveCategoryPair {
                    first: female.clone(),
                    second: male.clone(),
                    developing_parent: female.clone(),
                }],
                offspring_categories: vec![
                    world_domain::OffspringCategoryWeight {
                        category: female,
                        weight: 1,
                    },
                    world_domain::OffspringCategoryWeight {
                        category: male,
                        weight: 1,
                    },
                ],
            },
            adult_body_mass: Some(world_domain::AdultBodyMassCommitment {
                commitment_schema_version: world_domain::ADULT_BODY_MASS_COMMITMENT_SCHEMA_VERSION,
                species: species.clone(),
                evidence_basis: world_domain::PhysiologicalEvidenceBasis::LiteratureApproximation,
                profile_set_digest: Digest::sha256(b"runner body-mass source set"),
                source_record_id: "runner-body-mass-source-row-v1".to_owned(),
                source_record_digest: Digest::sha256(b"runner body-mass source row"),
                mass_grams_value: 70_000,
                mass_grams_decimal_places: 0,
            }),
            heritable_disposition_profile: Some(world_domain::HeritableDispositionProfile {
                profile_schema_version: world_domain::HERITABLE_DISPOSITION_PROFILE_SCHEMA_VERSION,
                profile_id: "runner-body-profile-test-heredity".to_owned(),
                profile_digest: Digest::sha256(b"runner body-profile test heredity"),
                species,
                evidence_basis: world_domain::PhysiologicalEvidenceBasis::EngineeringAssumption,
                minimum_action_weight: 4,
                neutral_action_weight: 16,
                maximum_action_weight: 28,
                founder_variation_steps: 3,
                mutation_probability_millionths: 100_000,
                mutation_max_step: 2,
            }),
        }
    }

    #[test]
    fn ruleset_fourteen_genesis_applies_one_pinned_body_profile_plan() {
        let world_id = WorldId::from_uuid(Uuid::new_v4());
        let species = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("human species");
        let plan = ProvisionalOrganismBodyProfilePlan {
            plan_schema_version: world_data::PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION,
            status: world_data::PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS.to_owned(),
            tick_duration_seconds: 300,
            entries: vec![provisional_body_profile_entry(species.clone())],
        };
        let bytes = plan.canonical_bytes().expect("canonical body-profile plan");
        let path = std::env::temp_dir().join(format!(
            "atc-runner-body-profile-plan-{}.json",
            Uuid::new_v4()
        ));
        std::fs::write(&path, &bytes).expect("write body-profile plan");
        let mut organisms = vec![InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"profiled-founder"),
            species,
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female").expect("category"),
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: Some("0000000000004000".parse().expect("L23 patch")),
            metabolic_rate: None,
            adult_body_mass: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
        }];

        let digest = apply_provisional_organism_body_profiles(
            &mut organisms,
            REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION,
            300,
            Some(&path),
        )
        .expect("apply profile plan");
        assert_eq!(
            digest,
            Some(AppliedBodyProfileEvidence {
                profile_plan_digest: Digest::sha256(&bytes),
                life_history_profile_set_digests: Vec::new(),
                body_mass_profile_set_digests: vec![Digest::sha256(b"runner body-mass source set")],
            })
        );
        assert_eq!(organisms[0].initial_age_ticks, 20);
        assert!(organisms[0].metabolic_rate.is_some());
        assert!(organisms[0].adult_body_mass.is_none());
        assert!(organisms[0].physiological_regulation.is_some());
        assert!(organisms[0].reproductive_physiology.is_some());
        assert!(organisms[0].heritable_disposition_profile.is_none());

        let mut heritable_organisms = organisms.clone();
        apply_provisional_organism_body_profiles(
            &mut heritable_organisms,
            HERITABLE_DISPOSITION_RULESET_VERSION,
            300,
            Some(&path),
        )
        .expect("ruleset fifteen applies the pinned heritable profile");
        assert_eq!(
            heritable_organisms[0].heritable_disposition_profile,
            plan.entries[0].heritable_disposition_profile
        );

        let mut mass_state_organisms = organisms.clone();
        apply_provisional_organism_body_profiles(
            &mut mass_state_organisms,
            ADULT_BODY_MASS_STATE_RULESET_VERSION,
            300,
            Some(&path),
        )
        .expect("ruleset thirty-two retains exact adult-body-mass state");
        assert_eq!(
            mass_state_organisms[0].adult_body_mass,
            plan.entries[0].adult_body_mass
        );

        let mut wrong_tick = organisms.clone();
        assert!(
            apply_provisional_organism_body_profiles(
                &mut wrong_tick,
                REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION,
                301,
                Some(&path),
            )
            .is_err()
        );
        let mut missing = organisms;
        assert!(
            apply_provisional_organism_body_profiles(
                &mut missing,
                REPRODUCTIVE_PHYSIOLOGY_RULESET_VERSION,
                300,
                None,
            )
            .is_err()
        );
        std::fs::remove_file(path).expect("remove body-profile plan");
    }

    #[test]
    fn ruleset_thirty_two_pins_every_distinct_body_mass_source_set() {
        let world_id = WorldId::from_uuid(Uuid::new_v4());
        let human = SpeciesIdentity::new(
            "gbif",
            "2436436",
            "Homo sapiens",
            "https://www.gbif.org/species/2436436",
        )
        .expect("human species");
        let fox = SpeciesIdentity::new(
            "gbif",
            "5219243",
            "Vulpes vulpes",
            "https://www.gbif.org/species/5219243",
        )
        .expect("fox species");
        let mut human_profile = provisional_body_profile_entry(human.clone());
        let mut fox_profile = provisional_body_profile_entry(fox.clone());
        let human_source = Digest::sha256(b"human mass source set");
        let fox_source = Digest::sha256(b"fox mass source set");
        human_profile
            .adult_body_mass
            .as_mut()
            .expect("human mass")
            .profile_set_digest = human_source;
        fox_profile
            .adult_body_mass
            .as_mut()
            .expect("fox mass")
            .profile_set_digest = fox_source;
        let plan = ProvisionalOrganismBodyProfilePlan {
            plan_schema_version: world_data::PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_SCHEMA_VERSION,
            status: world_data::PROVISIONAL_ORGANISM_BODY_PROFILE_PLAN_STATUS.to_owned(),
            tick_duration_seconds: 300,
            entries: vec![human_profile, fox_profile],
        };
        let bytes = plan.canonical_bytes().expect("canonical body-profile plan");
        let path = std::env::temp_dir().join(format!(
            "atc-runner-multiple-body-profile-sets-{}.json",
            Uuid::new_v4()
        ));
        std::fs::write(&path, bytes).expect("write body-profile plan");
        let mut organisms = [human, fox]
            .into_iter()
            .enumerate()
            .map(|(ordinal, species)| InitialOrganism {
                organism_id: EntityId::deterministic(
                    world_id,
                    format!("profiled-founder-{ordinal}").as_bytes(),
                ),
                species,
                role: OrganismRole::Person,
                birth_category: BirthCategory::new("female").expect("category"),
                initial_age_ticks: 0,
                location_id: None,
                embodied_patch: Some("0000000000004000".parse().expect("L23 patch")),
                metabolic_rate: None,
                adult_body_mass: None,
                physiological_regulation: None,
                reproductive_physiology: None,
                heritable_disposition_profile: None,
            })
            .collect::<Vec<_>>();
        let evidence = apply_provisional_organism_body_profiles(
            &mut organisms,
            ADULT_BODY_MASS_STATE_RULESET_VERSION,
            300,
            Some(&path),
        )
        .expect("ruleset 32 accepts multiple source sets")
        .expect("profile evidence");
        assert_eq!(
            evidence.body_mass_profile_set_digests,
            vec![human_source, fox_source]
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        );

        let error = apply_provisional_organism_body_profiles(
            &mut organisms,
            ADULT_BODY_MASS_STATE_RULESET_VERSION - 1,
            300,
            Some(&path),
        )
        .expect_err("older rulesets retain their single-set genesis contract")
        .to_string();
        assert!(error.contains("cannot mix multiple body-mass profile sets"));
        std::fs::remove_file(path).expect("remove body-profile plan");
    }

    #[test]
    fn fauna_ecology_is_source_resolved_but_never_applied_to_organisms() {
        let world_id = WorldId::from_uuid(Uuid::new_v4());
        let species = SpeciesIdentity::new(
            "gbif",
            "12",
            "Canis lupus",
            "https://www.gbif.org/species/12",
        )
        .expect("fauna species");
        let profile = FaunaPhysiologyProfile {
            species: species.clone(),
            trait_id: "diet-terrestrial-vertebrate-share-percent".to_owned(),
            value: ScaledFaunaTraitValue {
                value: 80,
                decimal_places: 0,
                unit: "percent".to_owned(),
            },
            source: FaunaEvidenceSource::EltonTraitsV1_0,
            source_field: "VertEnd".to_owned(),
            source_record_id: "elton-mammal-line-7".to_owned(),
            source_record_digest: Digest::sha256(b"ecology row"),
            evidence_basis: FaunaEvidenceBasis::SourceCompiledSpeciesAggregate,
        };
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: world_data::FAUNA_PHYSIOLOGY_PROFILE_SET_SCHEMA_VERSION,
            source_artifact_digest: Digest::sha256(b"Elton source"),
            profiles: vec![profile.clone()],
        };
        let profile_bytes = profiles.canonical_bytes().expect("canonical profiles");
        let plan = FaunaEcologyPlan {
            plan_schema_version: world_data::FAUNA_ECOLOGY_PLAN_SCHEMA_VERSION,
            profile_set_digest: Digest::sha256(&profile_bytes),
            entries: vec![FaunaEcologyPlanEntry {
                species: species.clone(),
                profiles: vec![FaunaEcologyProfileSelection {
                    trait_id: profile.trait_id,
                    source_record_id: profile.source_record_id,
                }],
            }],
        };
        let plan_bytes = plan.canonical_bytes().expect("canonical ecology plan");
        let stem = format!("atc-runner-ecology-{}", Uuid::new_v4());
        let profile_path = std::env::temp_dir().join(format!("{stem}-profiles.json"));
        let plan_path = std::env::temp_dir().join(format!("{stem}-plan.json"));
        std::fs::write(&profile_path, &profile_bytes).expect("write profiles");
        std::fs::write(&plan_path, &plan_bytes).expect("write plan");
        let organisms = vec![InitialOrganism {
            organism_id: EntityId::deterministic(world_id, b"ecology-fauna"),
            species,
            role: OrganismRole::Fauna,
            birth_category: BirthCategory::new("female").expect("category"),
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: Some("0000000000004000".parse().expect("L23 patch")),
            metabolic_rate: None,
            adult_body_mass: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
        }];
        let evidence = load_provisional_fauna_ecology_evidence(
            Some(&profile_path),
            Some(&plan_path),
            &organisms,
        )
        .expect("source-resolved evidence")
        .expect("present evidence");
        assert_eq!(evidence.plan_digest, Digest::sha256(&plan_bytes));
        assert_eq!(evidence.profile_set_digest, Digest::sha256(&profile_bytes));
        assert_eq!(organisms[0].role, OrganismRole::Fauna);
        std::fs::remove_file(profile_path).expect("remove profiles");
        std::fs::remove_file(plan_path).expect("remove plan");
    }

    fn origin_environment(
        selected_l10_patch: S2CellId,
        selected_embodied_patch: S2CellId,
    ) -> ProvisionalOriginEnvironment {
        ProvisionalOriginEnvironment {
            environment_schema_version: 1,
            status: "evidence-only-not-habitat-suitability-or-population".to_owned(),
            origin_selection_digest: world_domain::Digest::sha256(b"origin"),
            composition_digest: world_domain::Digest::sha256(b"composition"),
            selected_l10_patch,
            selected_embodied_patch,
            observed_land_cover_root_digest: world_domain::Digest::sha256(b"land root"),
            observed_land_cover_tile_digest: world_domain::Digest::sha256(b"land tile"),
            observed_land_cover: LandCoverEvidenceCell {
                s2_cell_id: selected_l10_patch,
                support_samples: 1,
                class_counts: vec![LandCoverClassCount {
                    class_value: 130,
                    samples: 1,
                }],
                processed_flag_counts: vec![LandCoverSignedValueCount {
                    value: 1,
                    samples: 1,
                }],
                current_pixel_state_counts: vec![LandCoverSignedValueCount {
                    value: 1,
                    samples: 1,
                }],
                observation_count_minimum: 1,
                observation_count_sum: 1,
                observation_count_maximum: 1,
                change_count_minimum: 0,
                change_count_sum: 0,
                change_count_maximum: 0,
            },
            air_temperature_normal_root_digest: world_domain::Digest::sha256(b"climate root"),
            air_temperature_normal_tile_digest: world_domain::Digest::sha256(b"climate tile"),
            air_temperature_normal_unit: "degC".to_owned(),
            air_temperature_normal_decimal_places: 1,
            air_temperature_normal: SeasonalScalarFieldCell {
                s2_cell_id: selected_l10_patch,
                support_samples_per_phase: 1,
                minimum_values: vec![1; 12],
                mean_values: vec![2; 12],
                maximum_values: vec![3; 12],
            },
            local_surface: None,
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
        let origin_environment_path = directory.join("origin-environment.json");
        let population_plan_path = directory.join("population-plan.json");
        let metabolic_profile_set_path = directory.join("metabolic-profile-set.json");
        let metabolic_rate_plan_path = directory.join("metabolic-rate-plan.json");
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
        let selected_l10_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let initial_patch = selected_l10_patch.descendants_at(11).expect("L11 patch")[0];
        let environment = origin_environment(selected_l10_patch, initial_patch);
        let environment_bytes = environment
            .canonical_bytes()
            .expect("canonical environment");
        std::fs::write(&origin_environment_path, &environment_bytes)
            .expect("write origin environment");
        let plan = FaunaPopulationPlan {
            population_plan_schema_version: FAUNA_POPULATION_PLAN_SCHEMA_VERSION,
            status: "provisional-not-scientifically-admitted".to_owned(),
            world_seed: seed,
            origin_environment_digest: world_domain::Digest::sha256(&environment_bytes),
            embodied_patch: initial_patch,
            candidate_set_digest: selection.candidate_set_digest,
            seeded_selection_digest: world_domain::Digest::sha256(
                &selection
                    .canonical_bytes_against(&candidates)
                    .expect("canonical selection"),
            ),
            entries: {
                let mut entries = selection
                    .selected_candidates
                    .iter()
                    .map(|candidate| FaunaPopulationPlanEntry {
                        species: candidate.species.clone(),
                        initial_individual_count: 2,
                        birth_category_counts: vec![
                            FaunaBirthCategoryCount {
                                category: BirthCategory::new("female").expect("category"),
                                count: 1,
                            },
                            FaunaBirthCategoryCount {
                                category: BirthCategory::new("male").expect("category"),
                                count: 1,
                            },
                        ],
                    })
                    .collect::<Vec<_>>();
                entries.sort_by_key(|entry| {
                    entry
                        .species
                        .identifier
                        .parse::<u64>()
                        .expect("numeric GBIF key")
                });
                entries
            },
        };
        std::fs::write(
            &population_plan_path,
            plan.canonical_bytes_against(&candidates, &selection)
                .expect("canonical population plan"),
        )
        .expect("write population plan");
        let profiles = FaunaPhysiologyProfileSet {
            profile_set_schema_version: 1,
            source_artifact_digest: world_domain::Digest::sha256(
                b"retained metabolic observations",
            ),
            profiles: plan
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| FaunaPhysiologyProfile {
                    species: entry.species.clone(),
                    trait_id: "standardized-metabolic-rate".to_owned(),
                    value: ScaledFaunaTraitValue {
                        value: i64::try_from(index + 1).expect("small fixture") * 125,
                        decimal_places: 3,
                        unit: "W".to_owned(),
                    },
                    source: FaunaEvidenceSource::AnimalTraitsV1_0_7,
                    source_field: "metabolic_rate".to_owned(),
                    source_record_id: format!("fixture-row-{}", index + 1),
                    source_record_digest: world_domain::Digest::sha256(
                        format!("fixture metabolic row {}", index + 1).as_bytes(),
                    ),
                    evidence_basis: FaunaEvidenceBasis::EmpiricalObservation,
                })
                .collect(),
        };
        let profile_bytes = profiles.canonical_bytes().expect("canonical profiles");
        std::fs::write(&metabolic_profile_set_path, &profile_bytes).expect("write profiles");
        let profile_digest = world_domain::Digest::sha256(&profile_bytes);
        let metabolic_plan = FaunaMetabolicRatePlan {
            plan_schema_version: 1,
            selections: profiles
                .profiles
                .iter()
                .map(|profile| FaunaMetabolicRateSelection {
                    selection_schema_version: 1,
                    profile_set_digest: profile_digest,
                    species: profile.species.clone(),
                    source_record_id: profile.source_record_id.clone(),
                })
                .collect(),
        };
        std::fs::write(
            &metabolic_rate_plan_path,
            metabolic_plan
                .canonical_bytes()
                .expect("canonical metabolic plan"),
        )
        .expect("write metabolic plan");
        let genesis = load_provisional_fauna_initial_organisms(
            world_id,
            seed,
            initial_patch,
            ProvisionalFaunaInputPaths {
                candidates_path: Some(&candidates_path),
                selection_path: Some(&selection_path),
                origin_environment_path: Some(&origin_environment_path),
                population_plan_path: Some(&population_plan_path),
                metabolic_profile_set_path: Some(&metabolic_profile_set_path),
                metabolic_rate_plan_path: Some(&metabolic_rate_plan_path),
            },
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
        assert!(
            genesis
                .initial_organisms
                .iter()
                .all(|organism| organism.metabolic_rate.is_some())
        );
        for species in plan.entries.iter().map(|entry| &entry.species) {
            let categories = genesis
                .initial_organisms
                .iter()
                .filter(|organism| organism.species == *species)
                .map(|organism| organism.birth_category.as_str())
                .collect::<Vec<_>>();
            assert_eq!(categories, ["female", "male"]);
        }
        assert_ne!(
            genesis.metabolic_rate_plan_digest,
            Some(world_domain::Digest::ZERO)
        );
        assert_ne!(genesis.candidate_set_digest, world_domain::Digest::ZERO);
        assert_ne!(genesis.selection_digest, world_domain::Digest::ZERO);
        assert_ne!(
            genesis.origin_environment_digest,
            world_domain::Digest::ZERO
        );
        assert_ne!(genesis.population_plan_digest, world_domain::Digest::ZERO);
        assert!(
            load_provisional_fauna_initial_organisms(
                world_id,
                WorldSeed::new(8),
                initial_patch,
                ProvisionalFaunaInputPaths {
                    candidates_path: Some(&candidates_path),
                    selection_path: Some(&selection_path),
                    origin_environment_path: Some(&origin_environment_path),
                    population_plan_path: Some(&population_plan_path),
                    metabolic_profile_set_path: None,
                    metabolic_rate_plan_path: None,
                },
            )
            .is_err()
        );
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn provisional_full_earth_defaults_to_the_source_backed_environment_ruleset() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "--database-url",
            "postgres://example",
            "init-provisional-full-earth",
            "--world-id",
            "00000000-0000-0000-0000-000000000001",
            "--seed",
            "1",
            "--composition",
            "composition.json",
            "--artifact-root",
            "artifacts",
            "--refuse-other-worlds",
        ])
        .expect("parse provisional command");
        let Some(Command::InitProvisionalFullEarth {
            ruleset_version,
            refuse_other_worlds,
            cancer_research,
            ..
        }) = cli.command
        else {
            panic!("expected provisional initialization command");
        };
        assert_eq!(ruleset_version, LOCAL_INTERACTION_RULESET_VERSION);
        assert!(refuse_other_worlds);
        assert!(!cancer_research);
    }

    #[test]
    fn provisional_genesis_starts_with_twenty_four_balanced_unrelated_people() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(0x2400));
        let patch = S2CellId::new(0x89c259b000000000).expect("fixture patch");
        let founders = provisional_human_founders(world_id, patch).expect("human founders");
        assert_eq!(founders.len(), 24);
        assert_eq!(
            founders
                .iter()
                .filter(|founder| founder.birth_category.as_str() == "female")
                .count(),
            12
        );
        assert_eq!(
            founders
                .iter()
                .filter(|founder| founder.birth_category.as_str() == "male")
                .count(),
            12
        );
        assert_eq!(
            founders
                .iter()
                .map(|founder| founder.organism_id)
                .collect::<BTreeSet<_>>()
                .len(),
            24
        );
        assert!(
            founders
                .iter()
                .all(|founder| founder.role == OrganismRole::Person)
        );
    }

    #[test]
    fn cancer_genesis_mode_commits_one_thousand_balanced_adults() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "verify-provisional-genesis",
            "--world-id",
            "00000000-0000-0000-0000-000000000001",
            "--seed",
            "38",
            "--genesis-directory",
            "genesis",
            "--ruleset-version",
            "38",
            "--cancer-research",
        ])
        .expect("parse Cancer World genesis proof");
        let Some(Command::VerifyProvisionalGenesis {
            ruleset_version,
            cancer_research,
            ..
        }) = cli.command
        else {
            panic!("expected Cancer World genesis verification command");
        };
        assert_eq!(ruleset_version, CANCER_BIOLOGY_RULESET_VERSION);
        assert!(cancer_research);

        let world_id = WorldId::from_uuid(Uuid::from_u128(0xca6ce7));
        let patch = S2CellId::new(0x89c259b000000000).expect("fixture patch");
        let founders =
            cancer_research_human_founders(world_id, patch, 300).expect("Cancer World founders");
        let resident_count =
            usize::try_from(CANCER_RESEARCH_INITIAL_RESIDENTS).expect("resident count fits usize");
        assert_eq!(founders.len(), resident_count);
        assert_eq!(
            founders
                .iter()
                .filter(|founder| founder.birth_category.as_str() == "female")
                .count(),
            resident_count / 2
        );
        assert_eq!(
            founders
                .iter()
                .filter(|founder| founder.birth_category.as_str() == "male")
                .count(),
            resident_count / 2
        );
        assert_eq!(
            founders
                .iter()
                .map(|founder| founder.organism_id)
                .collect::<BTreeSet<_>>()
                .len(),
            resident_count
        );
        assert!(founders.iter().all(|founder| founder.initial_age_ticks > 0));
    }

    #[test]
    fn canonical_genesis_proof_is_an_explicit_database_free_command() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "verify-provisional-genesis",
            "--world-id",
            "00000000-0000-0000-0000-000000000001",
            "--seed",
            "1",
            "--genesis-directory",
            "genesis",
        ])
        .expect("parse database-free genesis proof");
        let Some(Command::VerifyProvisionalGenesis {
            seed,
            genesis_directory,
            tick_duration_seconds,
            max_events_per_partition_transition,
            ruleset_version,
            ..
        }) = cli.command
        else {
            panic!("expected provisional genesis verification command");
        };
        assert_eq!(seed, 1);
        assert_eq!(genesis_directory, std::path::Path::new("genesis"));
        assert_eq!(tick_duration_seconds, 300);
        assert_eq!(max_events_per_partition_transition, 10_000);
        assert_eq!(ruleset_version, LOCAL_INTERACTION_RULESET_VERSION);
    }

    #[test]
    fn canonical_genesis_proof_requires_a_portable_complete_manifest() {
        let directory =
            std::env::temp_dir().join(format!("atc-genesis-manifest-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("test directory");
        let artifact = directory.join("seed.json");
        std::fs::write(&artifact, b"{}\n").expect("artifact");
        let artifact_digest = world_domain::Digest::sha256(b"{}\n");
        let portable_manifest = format!("{artifact_digest}  ./seed.json\n");
        std::fs::write(directory.join("SHA256SUMS"), &portable_manifest).expect("manifest");
        assert_eq!(
            verify_portable_genesis_manifest(&directory).expect("portable manifest"),
            world_domain::Digest::sha256(portable_manifest.as_bytes())
        );

        std::fs::write(
            directory.join("SHA256SUMS"),
            format!("{artifact_digest}  {}\n", artifact.display()),
        )
        .expect("absolute manifest");
        let error = verify_portable_genesis_manifest(&directory)
            .expect_err("absolute manifest must fail")
            .to_string();
        assert!(error.contains("noncanonical or nonportable"));
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn bounded_qualification_command_is_explicit_and_production_refusing() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "--database-url",
            "postgres://example",
            "advance-qualification",
            "--world-id",
            "00000000-0000-0000-0000-000000000001",
            "--ticks",
            "250",
        ])
        .expect("parse bounded qualification command");
        let Some(Command::AdvanceQualification { ticks, .. }) = cli.command else {
            panic!("expected qualification command");
        };
        assert_eq!(ticks, 250);
        assert!(is_production_environment(Some("production")));
        assert!(is_production_environment(Some(" Production ")));
        assert!(!is_production_environment(Some("development")));
        assert!(!is_production_environment(None));
    }

    #[test]
    fn memory_drain_is_explicit_and_does_not_change_continuous_defaults() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "--database-url",
            "postgres://example",
            "memory-worker",
            "--hindsight-base-url",
            "http://127.0.0.1:8888",
            "--drain",
        ])
        .expect("parse finite memory drain");
        let Some(Command::MemoryWorker {
            drain,
            poll_milliseconds,
            ..
        }) = cli.command
        else {
            panic!("expected memory worker command");
        };
        assert!(drain);
        assert_eq!(poll_milliseconds, 500);
    }

    #[test]
    fn local_cognition_default_allows_cpu_prefill_before_the_simulation_deadline() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "--database-url",
            "postgres://example",
            "cognition-worker",
            "--hindsight-base-url",
            "http://127.0.0.1:8888",
        ])
        .expect("parse cognition worker command");
        let Some(Command::CognitionWorker {
            request_timeout_seconds,
            ..
        }) = cli.command
        else {
            panic!("expected cognition worker command");
        };
        assert_eq!(
            request_timeout_seconds,
            DEFAULT_COGNITION_REQUEST_TIMEOUT_SECONDS
        );
        assert!(request_timeout_seconds < 60);
    }

    #[test]
    fn cancer_research_worker_requires_a_dedicated_key_and_defaults_paid_off() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "--database-url",
            "postgres://example",
            "cancer-research-worker",
            "--cancer-openrouter-api-key",
            "test-only-key",
            "--external-export-approved",
            "--drain",
        ])
        .expect("parse Cancer World research worker command");
        let Some(Command::CancerResearchWorker {
            paid_enabled,
            drain,
            request_timeout_seconds,
            paid_reservation_micro_usd,
            ..
        }) = cli.command
        else {
            panic!("expected Cancer World research worker command");
        };
        assert!(!paid_enabled);
        assert!(drain);
        assert_eq!(request_timeout_seconds, 120);
        assert_eq!(
            paid_reservation_micro_usd,
            application::MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        );
    }

    #[test]
    fn remote_cognition_requires_separate_export_approval() {
        validate_cognition_export_approval(0, false).expect("local fallback needs no export");
        validate_cognition_export_approval(1, true).expect("explicitly approved provider");
        assert!(validate_cognition_export_approval(1, false).is_err());
    }

    #[test]
    fn local_cognition_is_strictly_same_host_and_needs_no_export_approval() {
        for base_url in [
            "http://127.0.0.1:11434/v1",
            "http://localhost:8080/v1",
            "http://[::1]:11434/v1",
            "http://local-cognition:11434/v1",
        ] {
            validate_local_cognition_base_url(base_url).expect("same-host URL");
        }
        for base_url in [
            "https://127.0.0.1:11434/v1",
            "http://example.com/v1",
            "http://user@localhost:8080/v1",
            "http://localhost:8080/v1?forward=1",
            "http://local-cognition:11435/v1",
            "http://local-cognition.example:11434/v1",
        ] {
            assert!(validate_local_cognition_base_url(base_url).is_err());
        }
        validate_cognition_export_approval(0, false)
            .expect("loopback adapter is excluded from external providers");
    }

    #[test]
    fn provider_probe_contains_only_fixed_synthetic_state() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "probe-openrouter-free",
            "--api-key",
            "test-only-key",
        ])
        .expect("parse database-free provider probe");
        assert!(matches!(
            cli.command,
            Some(Command::ProbeOpenrouterFree { .. })
        ));

        let first = synthetic_cognition_probe_request();
        let second = synthetic_cognition_probe_request();
        assert_eq!(first, second);
        assert!(first.readings.is_empty());
        assert!(first.action_values.is_empty());
        assert!(first.recalled_memories.is_empty());
        assert_eq!(first.bodily_needs, BodilyNeedState::default());
    }

    #[test]
    fn capacity_sweep_is_database_free_and_has_an_explicit_matrix() {
        let cli = Cli::try_parse_from([
            "civilization-runner",
            "capacity-sweep",
            "--populations",
            "66,660,6600",
            "--active-percents",
            "10,100",
            "--ticks",
            "32",
        ])
        .expect("parse capacity sweep");
        let Some(Command::CapacitySweep {
            populations,
            active_percents,
            ticks,
        }) = cli.command
        else {
            panic!("expected capacity sweep command");
        };
        assert_eq!(populations, [66, 660, 6600]);
        assert_eq!(active_percents, [10, 100]);
        assert_eq!(ticks, 32);
    }

    #[test]
    fn europe_pmc_snapshot_is_provenanced_and_rejects_noncommercial_licenses() {
        let world_id = WorldId::from_uuid(Uuid::from_u128(37));
        let retrieved_at = chrono::DateTime::parse_from_rfc3339("2026-08-11T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&chrono::Utc);
        let payload = json!({
            "id": "42347750",
            "source": "MED",
            "pmid": "42347750",
            "pmcid": "PMC13296586",
            "doi": "10.1000/example",
            "title": "A bounded glioblastoma experiment",
            "abstractText": "A testable result with controls and limitations.",
            "authorString": "Researcher A, Researcher B",
            "firstPublicationDate": "2026-07-01",
            "license": "cc by",
            "isOpenAccess": "Y",
            "citedByCount": 0,
            "pubTypeList": {"pubType": ["research-article"]}
        });
        let snapshot = europe_pmc_snapshot(world_id, retrieved_at, payload.clone())
            .expect("parse evidence")
            .expect("eligible evidence");
        assert_eq!(snapshot.license, "cc by");
        assert!(snapshot.source_id.ends_with("/MED/42347750"));
        assert_eq!(
            snapshot.document.reference.content_hash,
            Digest::sha256(snapshot.document.content.as_bytes())
        );
        assert!(
            snapshot
                .document
                .content
                .contains("independently replicated")
        );

        let mut restricted = payload;
        restricted["license"] = json!("cc by-nc");
        assert!(
            europe_pmc_snapshot(world_id, retrieved_at, restricted)
                .expect("parse restricted record")
                .is_none()
        );
    }

    #[test]
    fn provisional_origin_environment_must_bind_to_the_selected_origin_and_composition() {
        let composition = ProvisionalWorldComposition::from_canonical_slice(include_bytes!(
            "../../../data/provisional/full-earth-breadth-first-0.1.2.json"
        ))
        .expect("checked-in provisional composition");
        let selected_l10_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let initial_patch = selected_l10_patch.descendants_at(11).expect("L11 patches")[0];
        let selection_digest = world_domain::Digest::sha256(b"origin");
        let origin = ResolvedInitialOrigin {
            patch: initial_patch,
            selection_digest: Some(selection_digest),
        };
        let mut environment = origin_environment(selected_l10_patch, initial_patch);
        environment.origin_selection_digest = selection_digest;
        environment.composition_digest = composition.content_digest().expect("composition hash");
        let directory =
            std::env::temp_dir().join(format!("atc-origin-environment-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("test directory");
        let path = directory.join("origin-environment.json");
        let bytes = environment
            .canonical_bytes()
            .expect("canonical environment");
        std::fs::write(&path, &bytes).expect("write origin environment");

        let verified = load_provisional_origin_environment(Some(&path), &composition, &origin)
            .expect("matching environment")
            .expect("provided environment");
        assert_eq!(verified.digest, world_domain::Digest::sha256(&bytes));

        environment.selected_embodied_patch =
            selected_l10_patch.descendants_at(11).expect("L11 patches")[1];
        std::fs::write(
            &path,
            environment
                .canonical_bytes()
                .expect("canonical changed environment"),
        )
        .expect("write changed environment");
        assert!(load_provisional_origin_environment(Some(&path), &composition, &origin).is_err());
        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn origin_climate_normals_become_a_source_bound_weather_configuration() {
        let selected_patch: S2CellId = "1000010000000000".parse().expect("L10 patch");
        let embodied_patch = selected_patch.descendants_at(11).expect("L11 patches")[0];
        let selection_digest = Digest::sha256(b"origin climate selection");
        let origin = ResolvedInitialOrigin {
            patch: embodied_patch,
            selection_digest: Some(selection_digest),
        };
        let evidence = ProvisionalOriginClimateEvidence {
            evidence_schema_version: PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_SCHEMA_VERSION,
            status: PROVISIONAL_ORIGIN_CLIMATE_EVIDENCE_STATUS.to_owned(),
            origin_selection_digest: selection_digest,
            selected_patch,
            sample_latitude_e7: 0,
            sample_longitude_e7: 0,
            source_snapshot_digest: Digest::sha256(b"ERA5 source snapshot"),
            source_grid_row: 360,
            source_grid_column: 0,
            source_grid_latitude_e7: 0,
            source_grid_longitude_e7: 0,
            source_artifacts: (world_data::ERA5_NORMAL_FIRST_YEAR
                ..=world_data::ERA5_NORMAL_LAST_YEAR)
                .map(|year| OriginClimateSourceArtifact {
                    year,
                    artifact_path: format!("era5/archive-{year}.zip"),
                    content_hash: Digest::sha256(&year.to_be_bytes()),
                    byte_length: 1,
                })
                .collect(),
            series: [
                ("siconc", "(0 - 1)", "avgua"),
                ("sst", "K", "avgua"),
                ("t2m", "K", "avgua"),
                ("tp", "m", "avgad"),
                ("u10", "m s**-1", "avgua"),
                ("v10", "m s**-1", "avgua"),
            ]
            .into_iter()
            .map(
                |(variable, source_unit, source_step_type)| OriginClimateSeries {
                    variable: variable.to_owned(),
                    source_unit: source_unit.to_owned(),
                    source_step_type: source_step_type.to_owned(),
                    values_ieee754_binary32_bits: vec![0; world_data::ERA5_NORMAL_MONTHS],
                },
            )
            .collect(),
        };
        let bytes = evidence
            .canonical_bytes()
            .expect("canonical climate evidence");
        let path = std::env::temp_dir().join(format!(
            "atc-origin-climate-evidence-{}.json",
            Uuid::new_v4()
        ));
        std::fs::write(&path, &bytes).expect("write climate evidence");

        let verified_evidence = load_provisional_origin_climate_evidence(Some(&path), &origin)
            .expect("matching climate evidence")
            .expect("verified evidence");
        assert_eq!(verified_evidence.digest, Digest::sha256(&bytes));
        assert_eq!(verified_evidence.selected_patch, selected_patch);
        let evidence_digest = Digest::sha256(&bytes);
        let normals = ProvisionalOriginClimateNormals {
            normals_schema_version: PROVISIONAL_ORIGIN_CLIMATE_NORMALS_SCHEMA_VERSION,
            status: PROVISIONAL_ORIGIN_CLIMATE_NORMALS_STATUS.to_owned(),
            origin_climate_evidence_digest: evidence_digest,
            conversion_policy:
                "binary32-nearest-even-per-observation-then-nearest-even-monthly-mean-v1".to_owned(),
            series: [
                ("siconc", "fraction", 6),
                ("sst", "degC", 3),
                ("t2m", "degC", 3),
                ("tp", "m", 6),
                ("u10", "m/s", 3),
                ("v10", "m/s", 3),
            ]
            .into_iter()
            .map(
                |(variable, unit, decimal_places)| OriginClimateNormalSeries {
                    variable: variable.to_owned(),
                    unit: unit.to_owned(),
                    decimal_places,
                    months: (1..=12)
                        .map(|month| OriginClimateNormalMonth {
                            month,
                            observed_years: 30,
                            minimum: Some(1),
                            mean: Some(2),
                            maximum: Some(3),
                        })
                        .collect(),
                },
            )
            .collect(),
        };
        let normals_bytes = normals
            .canonical_bytes()
            .expect("canonical climate normals");
        let normals_path = std::env::temp_dir().join(format!(
            "atc-origin-climate-normals-{}.json",
            Uuid::new_v4()
        ));
        std::fs::write(&normals_path, &normals_bytes).expect("write climate normals");
        let verified_normals = load_provisional_origin_climate_normals(
            Some(&normals_path),
            Some(&verified_evidence),
            &origin,
        )
        .expect("matching climate normals")
        .expect("verified normals");
        assert_eq!(verified_normals.digest, Digest::sha256(&normals_bytes));
        assert_eq!(verified_normals.baseline.evidence_patch, selected_patch);
        assert_eq!(verified_normals.baseline.active_patch, embodied_patch);
        assert_eq!(
            verified_normals.baseline.air_temperature_normal_mean,
            [2; 12]
        );
        assert!(
            load_provisional_origin_climate_normals(
                Some(&normals_path),
                Some(&VerifiedProvisionalOriginClimateEvidence {
                    digest: Digest::sha256(b"different evidence"),
                    selected_patch,
                }),
                &origin,
            )
            .is_err()
        );
        let wrong_origin = ResolvedInitialOrigin {
            patch: embodied_patch,
            selection_digest: Some(Digest::sha256(b"different origin")),
        };
        assert!(load_provisional_origin_climate_evidence(Some(&path), &wrong_origin).is_err());
        std::fs::remove_file(path).expect("remove climate evidence");
        std::fs::remove_file(normals_path).expect("remove climate normals");
    }
}
