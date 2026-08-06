use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use world_data::WorldDataBundle;
use world_data_filesystem::verify_release_artifacts;
use world_domain::WorldConfiguration;

#[derive(Debug, Parser)]
#[command(name = "civilization-data")]
#[command(about = "Validate canonical scientific world-data bundles")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate release completeness, canonical bytes, and optionally a world config.
    Validate {
        bundle: PathBuf,
        #[arg(long)]
        configuration: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate {
            bundle,
            configuration,
        } => validate(bundle, configuration.as_ref()),
    }
}

fn validate(bundle_path: PathBuf, configuration_path: Option<&PathBuf>) -> Result<()> {
    let bytes = fs::read(&bundle_path)
        .with_context(|| format!("failed to read bundle {}", bundle_path.display()))?;
    let bundle = WorldDataBundle::from_canonical_slice(&bytes)
        .with_context(|| format!("bundle {} is invalid", bundle_path.display()))?;
    let digest = bundle.content_digest()?;
    let artifact_root = bundle_path
        .parent()
        .context("bundle path has no parent directory")?;
    let stats = verify_release_artifacts(&bundle, artifact_root)?;

    if let Some(path) = configuration_path {
        let config_bytes = fs::read(path)
            .with_context(|| format!("failed to read configuration {}", path.display()))?;
        let configuration: WorldConfiguration = serde_json::from_slice(&config_bytes)
            .with_context(|| format!("failed to decode configuration {}", path.display()))?;
        bundle
            .validate_for_configuration(&configuration)
            .with_context(|| format!("bundle does not match configuration {}", path.display()))?;
        println!("configuration: matched {}", path.display());
    }

    println!("bundle: {}@{}", bundle.bundle_id, bundle.bundle_version);
    println!("schema: {}", bundle.bundle_schema_version);
    println!("sha256: {digest}");
    println!("sources: {}", bundle.sources.len());
    println!("entities: {}", bundle.entities.len());
    println!("parameters: {}", bundle.parameters.len());
    println!("layers: {}", bundle.layers.len());
    println!("tile indexes: {}", stats.tile_indexes);
    println!("tiles: {}", stats.tiles);
    println!(
        "artifacts: {} ({} bytes verified)",
        stats.artifacts, stats.bytes
    );
    Ok(())
}
