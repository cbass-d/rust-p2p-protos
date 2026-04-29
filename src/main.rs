#![warn(clippy::pedantic)]
#![deny(clippy::unwrap_used)]

use clap::Parser;
use color_eyre::eyre::{Context, Result};
use tracing::instrument;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self, EnvFilter, Layer, layer::SubscriberExt};

use p2p_protos::{cli::CliArgs, error::AppError};

/// Initialize tracing for application, uses a rolling log file that refreshes daily
fn init_tracing() -> Result<WorkerGuard, AppError> {
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

    let _guard = init_tracing().wrap_err("failed to initialize tracing")?;

    let args = CliArgs::parse();
    p2p_protos::run(args).await
}
