mod cli;
mod messages;
mod network;
mod node;
mod tests;
mod tui;
mod utils;

use clap::Parser;
use color_eyre::eyre::Result;
use network::NodeNetwork;
use tokio::task::JoinSet;
use tracing::{info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self, EnvFilter, layer::SubscriberExt};

use crate::{cli::CliArgs, tui::app::App};

// Initialize tracing for application
// Use a rolling log file that refreshes daily
fn init_tracing() -> Result<WorkerGuard> {
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
    color_eyre::install()?;

    let _guard = init_tracing()?;

    let args = CliArgs::parse();
    let number_of_nodes = args.nodes;

    let mut network = NodeNetwork::new(number_of_nodes);

    // The task set will hold the TUI taks and the node network tasks
    // Using task set makes it easier to manage the waiting on tasks to finish
    let mut task_set: JoinSet<()> = JoinSet::new();

    // Build the TUI application, this will hold the terminal and state of the TUI
    // - network_event_tx: used to send events from the node network to the TUI module
    // - network_command_rx: used for the node network receive commands from the TUI module
    let (mut app, cancellation_token, network_event_tx, network_command_rx) = App::new();

    // Run the TUI task
    task_set.spawn(async move {
        let _ = app.run().await;
    });

    // Run the node network task
    let _cancellation_token = cancellation_token.clone();
    task_set.spawn(async move {
        let _ = network
            .run(
                number_of_nodes,
                network_event_tx,
                network_command_rx,
                _cancellation_token,
            )
            .await;
    });

    // Wait for all the tasks to finish gracefully
    task_set.join_all().await;

    info!("node network shutdown");

    Ok(())
}
