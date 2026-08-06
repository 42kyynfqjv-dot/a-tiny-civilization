use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use civilization_verify::VerificationBundle;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "Offline verifier for A Tiny Civilization histories")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write the deterministic non-production demonstration bundle.
    Demo {
        #[arg(long, default_value = "verification/demo-bundle.json")]
        output: PathBuf,
    },
    /// Verify a bundle without PostgreSQL, Hindsight, or network access.
    Verify { bundle: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Demo { output } => {
            let bundle = VerificationBundle::deterministic_demo()?;
            let encoded = bundle.to_pretty_json()?;
            fs::write(&output, encoded)
                .with_context(|| format!("write verification bundle to {}", output.display()))?;
            let report = bundle.verify()?;
            println!(
                "wrote {} (sequence {}, state {})",
                output.display(),
                report.through_sequence,
                report.state_hash
            );
        }
        Command::Verify { bundle } => {
            let bytes = fs::read(&bundle)
                .with_context(|| format!("read verification bundle {}", bundle.display()))?;
            let report = VerificationBundle::from_json(&bytes)?.verify()?;
            println!(
                "verified {} batches through sequence {} at tick {}: {:?}",
                report.event_batches, report.through_sequence, report.tick, report.status
            );
            println!("event head: {}", report.last_event_hash);
            println!("state hash: {}", report.state_hash);
            println!("genesis replay == snapshot + tail");
        }
    }
    Ok(())
}
