#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]

pub mod bus;
pub mod cli;
pub mod error;
pub mod messages;
pub mod network;
pub mod node;
pub mod simulation;
pub mod tui;
pub mod util;

use color_eyre::eyre::{Context, Result};
use tracing::{error, info};

use crate::{
    cli::CliArgs,
    error::AppError,
    network::TransportMode,
    simulation::Simulation,
    tui::{FRAME_RATE, TICK_RATE},
};

pub const MAX_NODES: u8 = 10;

/// Build and run the simulation from parsed CLI args.
pub async fn run(args: CliArgs) -> Result<()> {
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
        return Err(AppError::MaxNodes { max: MAX_NODES }.into());
    }

    let simulation = Simulation::builder()
        .max_nodes(MAX_NODES)
        .starting_nodes(starting_nodes)
        .tick_rate(TICK_RATE)
        .frame_rate(FRAME_RATE)
        .transport(mode)
        .bind_address(bind_address)
        .build()
        .wrap_err("failed to build simulation")?;

    simulation
        .run()
        .await
        .wrap_err("failed to run simulation")?;

    info!("node network shutdown");

    Ok(())
}
