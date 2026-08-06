use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use anyhow::{Context, Result};
use application::{
    AgentMemory, FoundationStore, MemoryOutboxStore, ServiceHeartbeat, WorldRuntimeError,
    WorldSession, WorldStore, advance_world, initialize_or_resume_world, resume_world,
};
use clap::{Parser, Subcommand};
use hindsight_adapter::HindsightMemory;
use postgres_store::PostgresStore;
use serde_json::json;
use sim_engine::{InitialOrganism, RULESET_VERSION};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
use world_domain::{
    BirthCategory, EntityId, OrganismRole, SpeciesIdentity, WorldId, WorldManifest, WorldSeed,
    WorldStatus,
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
        let next = advance_world(store, &current).await?;
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
