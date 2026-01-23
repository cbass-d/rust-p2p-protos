use anyhow::{Result, anyhow};
use ipnet::Ipv4AddrRange;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};
use tokio::sync::{broadcast, mpsc};

use message::Message;

use crate::node::Node;

pub mod message;

// A mock network used that nodes uses to communicate
pub struct NodeNetwork {
    nodes: HashMap<SocketAddr, mpsc::Sender<Message>>,
    from_nodes: mpsc::Receiver<Message>,
    tx: mpsc::Sender<Message>,

    // Ip range to assign nodes IpAddresses
    ips: Ipv4AddrRange,
}

impl NodeNetwork {
    pub fn new(start: Ipv4Addr, end: Ipv4Addr) -> Self {
        let ips = Ipv4AddrRange::new(start, end);
        let (tx, rx) = mpsc::channel(100);

        NodeNetwork {
            nodes: HashMap::new(),
            ips,
            from_nodes: rx,
            tx,
        }
    }

    pub fn add_node(&mut self) -> Result<Node> {
        if let Some(ip) = self.ips.next() {
            let (_, rx) = broadcast::channel::<()>(1);
            let (node, tx) = Node::new(ip, self.tx.clone(), rx);

            self.nodes.insert(SocketAddr::new(IpAddr::V4(ip), 6000), tx);

            Ok(node)
        } else {
            Err(anyhow!("No IPs left in range"))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::network::NodeNetwork;

    #[test]
    pub fn test_adding_new_nodes() {
        let mut network =
            NodeNetwork::new("10.0.0.1".parse().unwrap(), "10.0.0.10".parse().unwrap());

        let mut nodes = vec![];
        for _ in 0..5 {
            nodes.push(network.add_node().unwrap());
        }

        assert_eq!(nodes.len(), 5);
    }

    #[test]
    pub fn test_error_out_of_ips() {
        let mut network =
            NodeNetwork::new("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());

        network.add_node().unwrap();
        network.add_node().unwrap();

        assert!(network.add_node().is_err());
    }

    #[test]
    pub fn test_message_between_two_nodes() {
        let mut network =
            NodeNetwork::new("10.0.0.1".parse().unwrap(), "10.0.0.10".parse().unwrap());
    }
}
