use anyhow::Result;
use log::{self, LevelFilter, info};
use network::NodeNetwork;
use std::net::Ipv4Addr;
use tokio::sync::broadcast;

mod network;
mod node;

fn init_log() {
    env_logger::builder()
        .filter(Some("node_network"), LevelFilter::Debug)
        .filter(Some("node"), LevelFilter::Debug)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_log();

    let (tx, kill_signal) = broadcast::channel::<()>(1);

    let mut network = NodeNetwork::new(
        "10.0.0.1".parse().unwrap(),
        "10.0.0.254".parse().unwrap(),
        kill_signal,
    );

    info!(target: "node_network", "node network built with address ranges 10.0.0.1 - 10.0.254");

    let _ = tokio::task::spawn(async move {
        let _ = network.run().await;
    });

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(());
    })
    .await?;

    info!(target: "node_network", "node network shutdown");

    Ok(())
}
