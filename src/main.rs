#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]

mod cli;
mod error;
mod messages;
mod network;
mod node;
mod simulation;
mod tui;
mod util;

use clap::Parser;
use color_eyre::eyre::Result;
use network::TransportMode;
use tracing::{error, info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self, EnvFilter, Layer, layer::SubscriberExt};

use crate::{cli::CliArgs, error::AppError, simulation::Simulation};

const MAX_NODES: u8 = 10;
const TICK_RATE: f64 = 4.0;
const FRAME_RATE: f64 = 60.0;

/// Initialize tracing for application, uses a rolling log file that refreshes daily
fn init_tracing() -> Result<WorkerGuard, AppError> {
    // Setup rolling logging to file
    let file_appender = tracing_appender::rolling::daily("logs", "p2p.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(console_subscriber::spawn())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(true)
                .with_filter(EnvFilter::try_from_default_env().unwrap_or(
                        EnvFilter::new("info,libp2p_mdns=warn,libp2p_swarm=warn,libp2p_kad=warn,libp2p_identify=warn,libp2p_noise=warn,libp2p_tcp=warn,libp2p_yamux=warn")
                        )),
        )
        .with(tracing_error::ErrorLayer::default());

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(guard)
}

#[tokio::main]
#[instrument]
async fn main() -> Result<()> {
    color_eyre::install().expect("failed to setup color_eyre");

    let _guard = init_tracing()?;

    let args = CliArgs::parse();
    let starting_nodes = args.nodes;
    let mode = match args.transport_mode.as_str() {
        "memory" => TransportMode::Memory,
        "tcp" => TransportMode::Tcp,
        _ => {
            info!("defaulting to memory transport mode");
            TransportMode::Memory
        }
    };

    let bind_address = args.bind_address;
    if bind_address.is_none() && mode == TransportMode::Tcp {
        error!("bind address required for tcp mode");
        return Err(AppError::NoBindAddress.into());
    }

    if starting_nodes > MAX_NODES {
        return Err(AppError::MaxNodes { max: 10 }.into());
    }

    let simulation = Simulation::builder()
        .max_nodes(MAX_NODES)
        .starting_nodes(starting_nodes)
        .tick_rate(TICK_RATE)
        .frame_rate(FRAME_RATE)
        .transport(mode)
        .bind_address(bind_address)
        .build()?;

    simulation.run().await?;

    info!("node network shutdown");

    Ok(())
}
