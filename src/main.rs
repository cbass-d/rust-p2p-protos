use anyhow::Result;
use network::NodeNetwork;
use std::{fs::File, net::Ipv4Addr};
use tokio::{sync::broadcast, task::JoinSet};
use tracing::{Level, info, instrument};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self, EnvFilter, layer::SubscriberExt};

mod network;
mod node;
mod utils;

fn init_tracing() -> Result<WorkerGuard> {
    // Setup rolling logging to file
    let file_appender = tracing_appender::rolling::daily("logs", "p2p.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(
            EnvFilter::default()
                .add_directive(("p2p_protos=debug").parse()?)
                .add_directive(("network=debug").parse()?)
                .add_directive(("node=debug").parse()?),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .pretty(),
        )
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
    let _guard = init_tracing()?;

    let (tx, kill_signal) = broadcast::channel::<()>(1);

    let mut network = NodeNetwork::new(
        "10.0.0.1".parse().unwrap(),
        "10.0.0.254".parse().unwrap(),
        kill_signal,
    );

    info!("node network built with address ranges 10.0.0.1 - 10.0.254");

    let mut task_set: JoinSet<()> = JoinSet::new();

    task_set.spawn(async move {
        let _ = network.run().await;
    });

    task_set.spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(());
    });

    // Wait for all the tasks to finish gracefully
    task_set.join_all().await;

    info!("node network shutdown");

    Ok(())
}
