use color_eyre::eyre::Result;
use network::NodeNetwork;
use tokio::task::JoinSet;
use tracing::{info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self, EnvFilter, layer::SubscriberExt};

use crate::tui::app::App;

mod messages;
mod network;
mod node;
mod tui;
mod utils;

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

    let mut network = NodeNetwork::new(5);
    info!("node network built with 5 nodes");

    let mut task_set: JoinSet<()> = JoinSet::new();

    // Build the TUI application, this will hold the terminal and state of the TUI
    let (mut app, cancellation_token, network_event_tx, network_command_rx) = App::new();

    // Run the TUI task
    task_set.spawn(async move {
        let _ = app.run().await;
    });

    // Run the node network task
    let _cancellation_token = cancellation_token.clone();
    task_set.spawn(async move {
        let _ = network
            .run(5, network_event_tx, network_command_rx, _cancellation_token)
            .await;
    });

    // Wait for all the tasks to finish gracefully
    task_set.join_all().await;

    info!("node network shutdown");

    Ok(())
}
