use anyhow::{Result, anyhow};
use core::fmt;
use rand::prelude::*;
use std::net::Ipv4Addr;
use tokio::sync::{broadcast, mpsc};

use crate::network::message::Message;

#[derive(Clone, Copy, Debug, Default)]
pub struct NodeStats {
    pub recvd_count: u64,
    pub sent_count: u64,
}

impl fmt::Display for NodeStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Packets recieved: {}\nPackets sent: {}\n",
            self.recvd_count, self.sent_count
        )
    }
}

// Node strucure representing a peer or participant in the network
pub struct Node {
    id: String,
    address: Ipv4Addr,
    to_network: mpsc::Sender<Message>,
    from_network: mpsc::Receiver<Message>,
    kill_signal: broadcast::Receiver<()>,

    pub stats: NodeStats,
}

impl Node {
    // Constructs a new node with the provided addresss.
    // Also takes in the write end for an mpsc channel used to
    // send messages to the network, as well as the reciever for
    // the broadccast channel where the kill signal will be sent at
    // program shutdown
    //
    // Returns the constructed node as well as the write end of the mpsc
    // channel where the node will be recieving messages
    pub fn new(
        address: Ipv4Addr,
        to_network: mpsc::Sender<Message>,
        kill_signal: broadcast::Receiver<()>,
    ) -> (Self, mpsc::Sender<Message>) {
        // Create a random alphanumeric ID for the node
        let rng = rand::rng();
        let id: String = rng
            .sample_iter(rand::distr::Alphanumeric)
            .take(5)
            .map(|c| c as char)
            .collect();

        // Build the mpsc channel where the channel will be recieving messages from
        let (tx, rx) = mpsc::channel(100);

        let node = Node {
            id,
            address,
            to_network,
            from_network: rx,
            kill_signal,
            stats: NodeStats::default(),
        };

        (node, tx)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn ip(&self) -> Ipv4Addr {
        self.address.clone()
    }

    pub async fn send_to(&mut self, destination: Ipv4Addr, message: &[u8]) -> Result<()> {
        let message = Message::new(self.address, destination, message);

        self.to_network.send(message).await?;
        self.stats.sent_count += 1;

        Ok(())
    }

    // Main run loop of for the node
    pub async fn run(&mut self) -> Result<NodeStats> {
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

                    self.stats.recvd_count += 1;
                },
            }
        }

        Ok(self.stats)
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
