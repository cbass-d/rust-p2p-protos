use color_eyre::eyre::Result;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use crate::{
    messages::{NetworkCommand, NetworkEvent, NodeCommand, NodeResponse},
    node::{Node, NodeResult},
};

// A mock network through which the nodes communicate.
// and destination addresses when sending messages.
// The network maintains its own mpsc channel where it recieves messages
// from nodes to pass/forward to the destination node found in the message.
#[derive(Debug)]
pub struct NodeNetwork {
    /// Mapping from peer id to mpsc channel for sending commands to node
    nodes: HashMap<PeerId, mpsc::Sender<(NodeCommand, oneshot::Sender<NodeResponse>)>>,

    /// Mapping from peer id to nodes libp2p multi-address
    addresses: HashMap<PeerId, Multiaddr>,

    network_event_tx: mpsc::Sender<NetworkEvent>,

    cancellation_token: CancellationToken,

    network_start: Instant,

    packets: u64,
}

impl NodeNetwork {
    // Returns a new network with the specified network range set
    // by the start Ipv4 address and end Ipv4 address
    pub fn new(
        number_of_nodes: u8,
        network_event_tx: mpsc::Sender<NetworkEvent>,
        cancellation_token: CancellationToken,
    ) -> Self {
        debug!(target: "node_network", "node network built with {}", number_of_nodes);

        NodeNetwork {
            nodes: HashMap::with_capacity(number_of_nodes as usize),
            addresses: HashMap::new(),
            network_event_tx,
            cancellation_token,
            network_start: Instant::now(),
            packets: 0,
        }
    }

    // Builds and adds a new node into the network.
    // Returns a Node or an Err when the their is no more
    pub fn add_node(&mut self) -> Result<Node> {
        let (node, tx, listen_adrress) = Node::new()?;
        self.nodes.insert(node.peer_id, tx);
        self.addresses.insert(node.peer_id, listen_adrress);

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
        mut network_command_rx: mpsc::Receiver<NetworkCommand>,
    ) -> Result<()> {
        info!(target: "node_network", "node network running with {} nodes", number_of_nodes);

        self.network_start = Instant::now();

        let mut node_task_set: JoinSet<Result<NodeResult>> = JoinSet::new();

        // Create and run the requested number of nodes
        for _ in 0..number_of_nodes {
            // Store the nodes Multiaddress for connecting the nodes in the future
            let mut node = self.add_node()?;

            // Every node will be able to send network events and will have
            // a cancellation token to know when to stop
            let _network_event_tx = self.network_event_tx.clone();
            let _canceallation_token = self.cancellation_token.clone();

            let network_start = self.network_start;
            node_task_set.spawn(async move {
                node.run(network_start, _network_event_tx, _canceallation_token)
                    .await
            });

            debug!(target: "node_network", "new node task spawned");
        }

        loop {
            tokio::select! {
                _ =  self.cancellation_token.cancelled() => {
                    debug!(target: "node_network", "cancellation token signal received");
                    break;
                }
                Some(command) = network_command_rx.recv() => {
                    debug!(target: "node_network", "network command recieved {:?}", command);

                    node_task_set = self.handle_network_command(command, node_task_set).await;
                },
            }
        }

        info!(target: "node_network", "network now shutting down...");

        // Wait for all the nodes to finish
        let _ = node_task_set.join_all().await;

        debug!(target: "node_network", "node tasks ended");

        Ok(())
    }

    async fn handle_network_command(
        &mut self,
        command: NetworkCommand,
        mut node_task_set: JoinSet<Result<NodeResult>>,
    ) -> JoinSet<Result<NodeResult>> {
        match command {
            NetworkCommand::ConnectNodes { peer_one, peer_two } => {
                if let Some(node_channel) = self.nodes.get(&peer_one) {
                    if let Some(address) = self.addresses.get(&peer_two) {
                        let (tx, reply_rx) = oneshot::channel();
                        node_channel
                            .send((
                                NodeCommand::ConnectTo {
                                    peer: address.to_owned(),
                                },
                                tx,
                            ))
                            .await
                            .unwrap();
                    }
                }
            }
            NetworkCommand::DisconectNodes { peer_one, peer_two } => {
                if let Some(node_channel) = self.nodes.get(&peer_one) {
                    let (tx, reply_rx) = oneshot::channel();
                    node_channel
                        .send((NodeCommand::DisconnectFrom { peer: peer_two }, tx))
                        .await
                        .unwrap();
                }
            }
            NetworkCommand::GetIdentifyInfo { peer_id } => {
                if let Some(node_channel) = self.nodes.get(&peer_id) {
                    let (tx, reply_rx) = oneshot::channel();

                    node_channel
                        .send((NodeCommand::GetIdentifyInfo, tx))
                        .await
                        .unwrap();

                    let response = reply_rx.await.unwrap();

                    debug!(target: "node_network", "received identify info: {:?}", response);

                    match response {
                        NodeResponse::IdentifyInfo { info } => {
                            self.network_event_tx
                                .send(NetworkEvent::IdentifyInfo { info })
                                .await
                                .unwrap();
                        }
                    }
                }
            }
            NetworkCommand::StopNode { peer_id } => {
                if let Some(node_channel) = self.nodes.get(&peer_id) {
                    let (tx, reply_rx) = oneshot::channel();
                    node_channel.send((NodeCommand::Stop, tx)).await.unwrap();
                }
            }
            NetworkCommand::StartNode => {
                // We limit the amount of nodes to 10
                if self.nodes.len() >= 10 {
                    return node_task_set;
                }

                // Store the nodes Multiaddress for connecting the nodes in the future
                let mut node = self.add_node().unwrap();

                // Every node will be able to send network events and will have
                // a cancellation token to know when to stop
                let _network_event_tx = self.network_event_tx.clone();
                let _canceallation_token = self.cancellation_token.clone();

                let network_start = self.network_start;
                node_task_set.spawn(async move {
                    node.run(network_start, _network_event_tx, _canceallation_token)
                        .await
                });

                debug!(target: "node_network", "new node task spawned");
            }
            _ => {}
        }

        node_task_set
    }
}

#[cfg(test)]
mod tests {
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
