use anyhow::{Result, anyhow};
use ipnet::Ipv4AddrRange;
use libp2p::{
    Multiaddr, PeerId, Transport,
    core::{
        muxing::StreamMuxerBox,
        transport::{MemoryTransport, upgrade},
    },
    identify, identity,
    multiaddr::Protocol,
    noise, yamux,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinSet,
};
use tracing::{info, instrument};

use message::Message;

use crate::node::Node;

pub mod message;

// A mock network through which the nodes communicate.
// Nodes are given an mock IPv4 address which are used as source
// and destination addresses when sending messages.
// A mapping is kept of IPv4 addresses to the write end of a nodes
// mpsc channel where messages will be sent.
// The network maintains its own mpsc channel where it recieves messages
// from nodes to pass/forward to the destination node found in the message.
#[derive(Debug)]
pub struct NodeNetwork {
    nodes: HashMap<Ipv4Addr, mpsc::Sender<Message>>,
    from_nodes: mpsc::Receiver<Message>,
    kill_signal: broadcast::Receiver<()>,
    tx: mpsc::Sender<Message>,

    // Ip range to assign nodes IpAddresses
    ips: Ipv4AddrRange,

    packets: u64,
}

impl NodeNetwork {
    // Returns a new network with the specified network range set
    // by the start Ipv4 address and end Ipv4 address
    pub fn new(start: Ipv4Addr, end: Ipv4Addr, kill_signal: broadcast::Receiver<()>) -> Self {
        let ips = Ipv4AddrRange::new(start, end);
        let (tx, rx) = mpsc::channel(100);

        NodeNetwork {
            nodes: HashMap::new(),
            from_nodes: rx,
            kill_signal,
            tx,
            ips,
            packets: 0,
        }
    }

    // Builds and adds a new node into the network.
    // Returns a Node or an Err when the their is no more
    // IP addresses left to assign
    pub fn add_node(&mut self) -> Result<Node> {
        if let Some(ip) = self.ips.next() {
            let (node, tx) =
                Node::new(ip, self.tx.clone(), self.kill_signal.resubscribe()).unwrap();

            self.nodes.insert(ip, tx);

            Ok(node)
        } else {
            Err(anyhow!("No IPs left in range"))
        }
    }

    fn connect_two_nodes(&mut self, mut node_one: Node, node_two: &mut Node) -> Node {
        node_one.add_peer(node_two.listen_address.clone());

        node_one
    }

    // Main run loop of for the node
    #[instrument(skip(self), name = "run network")]
    pub async fn run(&mut self) -> Result<()> {
        info!(target: "node_network", "node network running");

        // JoinSet for the aysnc tasks running the nodes
        let mut node_task_set: JoinSet<()> = JoinSet::new();

        // Segment the nodes into two sections
        // section #1 [1, 2]
        // secion #2 [3, 4, 5]
        // node 2 and 3 will know each other and act as
        // the bridge between the two
        let node_one = self.add_node().unwrap();
        let mut node_two = self.add_node().unwrap();
        let mut node_three = self.add_node().unwrap();
        let node_four = self.add_node().unwrap();
        let node_five = self.add_node().unwrap();

        let mut node_one = self.connect_two_nodes(node_one, &mut node_two);
        let mut node_two = self.connect_two_nodes(node_two, &mut node_three);

        let mut node_three = self.connect_two_nodes(node_three, &mut node_two);
        let mut node_four = self.connect_two_nodes(node_four, &mut node_three);
        let mut node_five = self.connect_two_nodes(node_five, &mut node_four);

        node_task_set.spawn(async move {
            let _ = node_one.run().await;
        });

        node_task_set.spawn(async move {
            let _ = node_two.run().await;
        });

        node_task_set.spawn(async move {
            let _ = node_three.run().await;
        });

        node_task_set.spawn(async move {
            let _ = node_four.run().await;
        });

        node_task_set.spawn(async move {
            let _ = node_five.run().await;
        });

        loop {
            tokio::select! {
                _ = self.kill_signal.recv() => {
                    break;
                }

                message = self.from_nodes.recv() => {
                    if message.is_none() {
                        continue;
                    }

                    let message = message.unwrap();

                    if let Some(node_tx) = self.nodes.get(&message.destination()) {
                        let _ = node_tx.send(message).await;

                        self.packets += 1;
                    }
                }
            }
        }

        info!(target: "node_network", "network now shutting down...");

        // Wait for all the nodes to finish
        let _ = node_task_set.join_all().await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use tokio::sync::broadcast;

    use crate::{
        network::{NodeNetwork, message::Message},
        node::Node,
    };

    fn build_broadacast_channel() -> (broadcast::Sender<()>, broadcast::Receiver<()>) {
        broadcast::channel(1)
    }

    fn bulid_mpsc_channel() -> (mpsc::Sender<Message>, mpsc::Receiver<Message>) {
        mpsc::channel()
    }

    fn add_two_nodes_to_network(network: &mut NodeNetwork) -> (Node, Node) {
        let node_one = network.add_node().unwrap();
        let node_two = network.add_node().unwrap();

        (node_one, node_two)
    }

    #[test]
    pub fn test_adding_new_nodes() {
        let (_, rx) = build_broadacast_channel();
        let mut network = NodeNetwork::new(
            "10.0.0.1".parse().unwrap(),
            "10.0.0.10".parse().unwrap(),
            rx,
        );

        let mut nodes = vec![];
        for _ in 0..5 {
            nodes.push(network.add_node().unwrap());
        }

        assert_eq!(nodes.len(), 5);
    }

    #[test]
    pub fn test_error_out_of_ips() {
        let (_, rx) = build_broadacast_channel();
        let mut network =
            NodeNetwork::new("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap(), rx);

        network.add_node().unwrap();
        network.add_node().unwrap();

        assert!(network.add_node().is_err());
    }

    #[tokio::test]
    pub async fn test_message_between_two_nodes() {
        let (tx, rx) = build_broadacast_channel();
        let mut network = NodeNetwork::new(
            "10.0.0.1".parse().unwrap(),
            "10.0.0.10".parse().unwrap(),
            rx,
        );

        let (mut node_one, mut node_two) = add_two_nodes_to_network(&mut network);

        let node_one_ip = node_one.ip_address;
        let node_one_stats = node_one.stats.clone();

        // The network needs to be running in order to route the
        // message between the two nodes
        let network_handle = tokio::spawn(async move {
            let _ = network.run().await;
        });

        // Only the node recieving the message needs to be running
        // in this case.
        // We needs its handle in order to get the node's stats to verify
        // the message has recieved
        let node_one_handle = tokio::spawn(async move {
            let node_stats = node_one.run().await;
            node_stats
        });

        let _ = node_two.send_to(node_one_ip, b"test").await.unwrap();
        assert_eq!(node_two.stats.lock().unwrap().sent_count, 1);

        // We need to add a small sleep to allow for the node to recieve and
        // process the message before sending the kill signal
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx.send(()).unwrap();

        let _ = node_one_handle.await.unwrap();
        let _ = network_handle.await.unwrap();

        assert_eq!(node_one_stats.lock().unwrap().recvd_count, 1);
    }
}
