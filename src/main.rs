#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]

mod cli;
mod error;
mod messages;
mod network;
mod node;
mod simulation;
mod tui;

use clap::Parser;
use color_eyre::eyre::Result;
use tracing::{info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self, EnvFilter, layer::SubscriberExt};

use crate::{cli::CliArgs, error::AppError, simulation::Simulation};

const MAX_NODES: u8 = 10;
const TICK_RATE: f64 = 4.0;
const FRAME_RATE: f64 = 60.0;

/// Initialize tracing for application, uses a rolling log file that refreshes daily
fn init_tracing() -> Result<WorkerGuard, AppError> {
    // Setup rolling logging to file
    let file_appender = tracing_appender::rolling::daily("logs", "p2p.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new("info")))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        );

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(_guard)
}

#[tokio::main]
#[instrument]
async fn main() -> Result<()> {
    color_eyre::install().expect("failed to setup color_eyre");

    let _guard = init_tracing()?;

    let args = CliArgs::parse();
    let starting_nodes = args.nodes;

    if starting_nodes > MAX_NODES {
        return Err(AppError::MaxNodes { max: 10 }.into());
    }

    let simulation = Simulation::builder()
        .max_nodes(MAX_NODES)
        .starting_nodes(starting_nodes)
        .tick_rate(TICK_RATE)
        .frame_rate(FRAME_RATE)
        .build()?;

    simulation.run().await?;

    info!("node network shutdown");

    Ok(())
}
