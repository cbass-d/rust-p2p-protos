use anyhow::{Result, anyhow};
use core::fmt;
use rand::prelude::*;
use std::net::Ipv4Addr;
use tokio::sync::{broadcast, mpsc};

use crate::network::message::Message;

pub struct Node {
    id: String,
    address: Ipv4Addr,
    to_network: mpsc::Sender<Message>,
    from_network: mpsc::Receiver<Message>,
    kill_signal: broadcast::Receiver<()>,

    recvd_count: u64,
    sent_count: u64,
}

impl Node {
    pub fn new(
        address: Ipv4Addr,
        to_network: mpsc::Sender<Message>,
        kill_signal: broadcast::Receiver<()>,
    ) -> (Self, mpsc::Sender<Message>) {
        let rng = rand::rng();
        let id: String = rng
            .sample_iter(rand::distr::Alphanumeric)
            .take(5)
            .map(|c| c as char)
            .collect();
        let (tx, rx) = mpsc::channel(100);

        let node = Node {
            id,
            address,
            to_network,
            from_network: rx,
            kill_signal,
            recvd_count: 0,
            sent_count: 0,
        };

        (node, tx)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub async fn send_to(&mut self, destination: Ipv4Addr, message: &[u8]) -> Result<()> {
        let message = Message::new(self.address, destination, message);

        self.to_network.send(message).await?;

        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                _ = self.kill_signal.recv() => {
                    break;
                },
                message = self.from_network.recv() => {
                    if message.is_none() {
                        continue;
                    }

                    let message = message.unwrap();

                    println!("message from {}", message.source());

                    self.recvd_count += 1;
                },
            }
        }

        Ok(())
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node: {}", self.id)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::{broadcast, mpsc};

    use crate::node::Node;

    #[test]
    pub fn test_valid_node_id() {
        let (tx, _) = mpsc::channel(1);
        let (_, rx) = broadcast::channel::<()>(1);
        let (node, _) = Node::new("10.0.0.1".parse().unwrap(), tx, rx);

        assert_eq!(node.id().len(), 5);
        assert!(node.id().chars().into_iter().all(|c| c.is_alphanumeric()));
    }
}
