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

    /// mpsc sender for Network Events
    network_event_tx: mpsc::Sender<NetworkEvent>,

    /// CancellationToken that shared by all nodes as well
    cancellation_token: CancellationToken,

    /// The Instant that the network started, used to stamp swarm events
    network_start: Instant,

    /// The number of packets the network has seen in total
    packets: u64,
}

impl NodeNetwork {
    /// Builds a new node network with the provided mpsc sender, cancellation and with the provided
    /// number of starting nodes
    pub fn new(
        network_event_tx: mpsc::Sender<NetworkEvent>,
        cancellation_token: CancellationToken,
    ) -> Self {
        debug!(target: "node_network", "node network built");

        NodeNetwork {
            nodes: HashMap::new(),
            addresses: HashMap::new(),
            network_event_tx,
            cancellation_token,
            network_start: Instant::now(),
            packets: 0,
        }
    }

    /// Builds and adds a new node into the network. Returns a Node or an Err when building the
    /// new node fails
    pub fn add_node(&mut self) -> Result<Node> {
        let (node, tx, listen_adrress) = Node::new()?;
        self.nodes.insert(node.peer_id, tx);
        self.addresses.insert(node.peer_id, listen_adrress);

        Ok(node)
    }

    /// Main run loop of for the Node Network
    #[instrument(skip_all, name = "run network")]
    pub async fn run(
        &mut self,
        number_of_nodes: u8,
        mut network_command_rx: mpsc::Receiver<NetworkCommand>,
    ) -> Result<()> {
        info!(target: "node_network", "node network running with {} nodes", self.nodes.len());

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

    /// Handle a network command received from the TUI. Takes in the network command and
    /// the task set of the running nodes. We rturn the task set with updates, if any
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

                    let response = reply_rx.await.unwrap();

                    debug!(target: "node_network", "nodes disconnected");

                    match response {
                        NodeResponse::Disconnected { peer } => {
                            self.network_event_tx
                                .send(NetworkEvent::NodesDisconnected {
                                    peer_one,
                                    peer_two: peer,
                                })
                                .await
                                .unwrap();
                        }
                        _ => {}
                    }
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
                        _ => {}
                    }
                }
            }
            NetworkCommand::GetKademliaInfo { peer_id } => {
                if let Some(node_channel) = self.nodes.get(&peer_id) {
                    let (tx, reply_rx) = oneshot::channel();

                    node_channel
                        .send((NodeCommand::GetKademliaInfo, tx))
                        .await
                        .unwrap();

                    let response = reply_rx.await.unwrap();

                    debug!(target: "node_network", "received kademlia info: {:?}", response);

                    match response {
                        NodeResponse::KademliaInfo { info } => {
                            self.network_event_tx
                                .send(NetworkEvent::KademliaInfo { info })
                                .await
                                .unwrap();
                        }
                        _ => {}
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
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::{
        messages::{NetworkCommand, NetworkEvent},
        network::NodeNetwork,
    };

    #[test]
    fn test_create_empty_network() {
        let (network_event_tx, _) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let node_network = NodeNetwork::new(network_event_tx, cancellation_token);

        assert!(node_network.nodes.is_empty());
        assert!(node_network.addresses.is_empty());
    }

    #[test]
    fn test_adding_new_node() {
        let (network_event_tx, _) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(network_event_tx, cancellation_token);

        let node = node_network.add_node().unwrap();

        assert!(node_network.nodes.contains_key(&node.peer_id));
        assert!(node_network.addresses.contains_key(&node.peer_id));
    }

    #[tokio::test]
    async fn test_connect_two_nodes() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(network_event_tx, cancellation_token.clone());

        let (network_command_tx, network_command_rx) = mpsc::channel(1);
        let _task = tokio::spawn(async move {
            let _ = node_network.run(2, network_command_rx).await;
        });

        let mut peer_ids = vec![];
        while peer_ids.len() < 2 {
            if let Some(event) = network_event_rx.recv().await {
                if let NetworkEvent::NodeRunning { peer_id, .. } = event {
                    peer_ids.push(peer_id);
                }
            }
        }

        network_command_tx
            .send(NetworkCommand::ConnectNodes {
                peer_one: peer_ids[0],
                peer_two: peer_ids[1],
            })
            .await
            .unwrap();

        while let Some(event) = network_event_rx.recv().await {
            if let NetworkEvent::NodesConnected { peer_one, peer_two } = event {
                assert!(peer_ids[0] == peer_one);
                assert!(peer_ids[1] == peer_two);
                break;
            }
        }
    }

    #[tokio::test]
    async fn test_stop_node() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(network_event_tx, cancellation_token.clone());

        let (network_command_tx, network_command_rx) = mpsc::channel(1);
        let _task = tokio::spawn(async move {
            let _ = node_network.run(1, network_command_rx).await;
        });

        let mut peer_ids = vec![];
        while peer_ids.len() < 1 {
            if let Some(event) = network_event_rx.recv().await {
                if let NetworkEvent::NodeRunning { peer_id, .. } = event {
                    peer_ids.push(peer_id);
                }
            }
        }

        network_command_tx
            .send(NetworkCommand::StopNode {
                peer_id: peer_ids[0],
            })
            .await
            .unwrap();

        while let Some(event) = network_event_rx.recv().await {
            if let NetworkEvent::NodeStopped { peer_id } = event {
                assert!(peer_ids[0] == peer_id);
                break;
            }
        }
    }

    #[tokio::test]
    async fn test_start_node() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(network_event_tx, cancellation_token.clone());

        let (network_command_tx, network_command_rx) = mpsc::channel(1);
        let _task = tokio::spawn(async move {
            let _ = node_network.run(0, network_command_rx).await;
        });

        network_command_tx
            .send(NetworkCommand::StartNode)
            .await
            .unwrap();

        while let Some(event) = network_event_rx.recv().await {
            assert!(matches!(event, NetworkEvent::NodeRunning { .. }));
            break;
        }
    }

    //#[tokio::test]
    //async fn test_disconnect_two_nodes() {
    //    let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
    //    let cancellation_token = CancellationToken::new();
    //    let mut node_network = NodeNetwork::new(network_event_tx, cancellation_token.clone());

    //    let (network_command_tx, network_command_rx) = mpsc::channel(1);
    //    let _task = tokio::spawn(async move {
    //        let _ = node_network.run(2, network_command_rx).await;
    //    });

    //    let mut peer_ids = vec![];
    //    while peer_ids.len() < 2 {
    //        if let Some(event) = network_event_rx.recv().await {
    //            if let NetworkEvent::NodeRunning { peer_id, .. } = event {
    //                peer_ids.push(peer_id);
    //            }
    //        }
    //    }

    //    network_command_tx
    //        .send(NetworkCommand::ConnectNodes {
    //            peer_one: peer_ids[0],
    //            peer_two: peer_ids[1],
    //        })
    //        .await
    //        .unwrap();

    //    network_command_tx
    //        .send(NetworkCommand::DisconectNodes {
    //            peer_one: peer_ids[0],
    //            peer_two: peer_ids[1],
    //        })
    //        .await
    //        .unwrap();

    //    while let Some(event) = network_event_rx.recv().await {
    //        if let NetworkEvent::NodesDisconnected { peer_one, peer_two } = event {
    //            break;
    //        }
    //    }
    //}
}
