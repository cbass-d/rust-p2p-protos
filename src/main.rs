use anyhow::Result;
use network::NodeNetwork;
use std::net::Ipv4Addr;
use tokio::sync::broadcast;

mod network;
mod node;

#[tokio::main]
async fn main() -> Result<()> {
    let (tx, kill_signal) = broadcast::channel::<()>(1);

    let mut network = NodeNetwork::new(
        "10.0.0.1".parse().unwrap(),
        "10.0.0.254".parse().unwrap(),
        kill_signal,
    );

    let _ = tokio::task::spawn(async move {
        let _ = network.run().await;
    });

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(());
    });

    Ok(())
}
