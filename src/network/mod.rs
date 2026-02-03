use color_eyre::eyre::Result;
use libp2p::PeerId;
use std::collections::HashMap;
use tokio::{sync::mpsc, task::JoinSet, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use message::Message;

use crate::{
    messages::{NetworkCommand, NetworkEvent},
    node::{Node, NodeResult},
};

pub mod message;

// A mock network through which the nodes communicate.
// and destination addresses when sending messages.
// The network maintains its own mpsc channel where it recieves messages
// from nodes to pass/forward to the destination node found in the message.
#[derive(Debug)]
pub struct NodeNetwork {
    nodes: HashMap<PeerId, mpsc::Sender<Message>>,
    from_nodes: mpsc::Receiver<Message>,
    tx: mpsc::Sender<Message>,

    packets: u64,
}

impl NodeNetwork {
    // Returns a new network with the specified network range set
    // by the start Ipv4 address and end Ipv4 address
    pub fn new(number_of_nodes: u8) -> Self {
        let (tx, rx) = mpsc::channel(100);

        NodeNetwork {
            nodes: HashMap::with_capacity(number_of_nodes as usize),
            from_nodes: rx,
            tx,
            packets: 0,
        }
    }

    // Builds and adds a new node into the network.
    // Returns a Node or an Err when the their is no more
    pub fn add_node(&mut self) -> Result<Node> {
        let (node, tx) = Node::new(self.tx.clone())?;

        self.nodes.insert(node.peer_id, tx);

        Ok(node)
    }

    fn connect_two_nodes(&mut self, mut node_one: Node, node_two: &mut Node) -> Node {
        node_one.add_peer(node_two.listen_address.clone());
        node_one
    }

    // Main run loop of for the node
    #[instrument(skip_all, name = "run network")]
    pub async fn run(
        &mut self,
        number_of_nodes: u8,
        network_event_tx: mpsc::Sender<NetworkEvent>,
        mut network_command_rx: mpsc::Receiver<NetworkCommand>,
        cancellation_token: CancellationToken,
    ) -> Result<()> {
        info!(target: "node_network", "node network running with {} nodes", number_of_nodes);

        // JoinSet for the aysnc tasks running the nodes
        let mut node_task_set: JoinSet<Result<NodeResult>> = JoinSet::new();

        let network_start = Instant::now();

        // Create and run the requested number of nodes
        for _ in 0..number_of_nodes {
            let mut node = self.add_node()?;

            // Every node will be able to send network events and will have
            // a cancellation token to know when to stop
            let _network_event_tx = network_event_tx.clone();
            let _canceallation_token = cancellation_token.clone();
            node_task_set.spawn(async move {
                node.run(network_start, _network_event_tx, _canceallation_token)
                    .await
            });
        }

        loop {
            tokio::select! {
                _ =  cancellation_token.cancelled() => {
                    break;
                }
                Some(command) = network_command_rx.recv() => {
                    debug!(target: "node_network", "network command recieved {:?}", command);
                },
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

    //#[test]
    //pub fn test_adding_new_nodes() {
    //    let mut network = NodeNetwork::new(5);

    //    let mut nodes = vec![];
    //    for _ in 0..5 {
    //        nodes.push(network.add_node().unwrap());
    //    }

    //    assert_eq!(nodes.len(), 5);
    //}

    //#[test]
    //pub fn test_error_out_of_ips() {
    //    let mut network = NodeNetwork::new(2);

    //    network.add_node();
    //    network.add_node();

    //    assert!(network.add_node().is_err());
    //}

    //#[tokio::test]
    //pub async fn test_message_between_two_nodes() {
    //    let (tx, rx) = build_broadacast_channel();
    //    let mut network = NodeNetwork::new(2, rx).unwrap();

    //    let (mut node_one, mut node_two) = add_two_nodes_to_network(&mut network);

    //    let node_one_stats = node_one.stats.clone();

    //    // The network needs to be running in order to route the
    //    // message between the two nodes
    //    let network_handle = tokio::spawn(async move {
    //        let _ = network.run(2).await;
    //    });

    //    // Only the node recieving the message needs to be running
    //    // in this case.
    //    // We needs its handle in order to get the node's stats to verify
    //    // the message has recieved
    //    let node_one_handle = tokio::spawn(async move {
    //        let node_stats = node_one.run().await;
    //        node_stats
    //    });

    //    let _ = node_two.send_to(node_one_ip, b"test").await.unwrap();
    //    assert_eq!(node_two.stats.lock().unwrap().sent_count, 1);

    //    // We need to add a small sleep to allow for the node to recieve and
    //    // process the message before sending the kill signal
    //    tokio::time::sleep(Duration::from_millis(100)).await;
    //    tx.send(()).unwrap();

    //    let _ = node_one_handle.await.unwrap();
    //    let _ = network_handle.await.unwrap();

    //    assert_eq!(node_one_stats.lock().unwrap().recvd_count, 1);
    //}
}
