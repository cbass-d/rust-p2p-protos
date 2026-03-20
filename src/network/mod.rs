use color_eyre::eyre::Result;
use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    error::AppError,
    messages::{NetworkCommand, NetworkEvent, NodeCommand, NodeResponse},
    node::{NodeError, NodeResult, configured::ConfiguredNode},
};

// A mock network through which the nodes communicate.
// and destination addresses when sending messages.
// The network maintains its own mpsc channel where it receives messages
// from nodes to pass/forward to the destination node found in the message.
#[derive(Debug)]
pub(crate) struct NodeNetwork {
    /// Mapping from peer id to mpsc channel for sending commands to node
    nodes: HashMap<PeerId, mpsc::Sender<(NodeCommand, oneshot::Sender<NodeResponse>)>>,

    /// Mapping from peer id to nodes libp2p multi-address
    addresses: HashMap<PeerId, Multiaddr>,

    /// mpsc sender for Network Events
    network_event_tx: mpsc::Sender<NetworkEvent>,

    /// `CancellationToken` that shared by all nodes as well
    cancellation_token: CancellationToken,

    /// The Instant that the network started, used to stamp swarm events
    network_start: Instant,

    /// The mpsc channel where network commands will be received from the TUI
    network_command_rx: mpsc::Receiver<NetworkCommand>,

    /// The max number of nodes the network can have
    max_nodes: u8,

    /// The number of nodes the network will start with
    starting_nodes: u8,
}

impl NodeNetwork {
    /// Builds a new node network with the provided mpsc sender, cancellation and with the provided
    /// number of starting nodes
    pub fn new(
        network_event_tx: mpsc::Sender<NetworkEvent>,
        network_command_rx: mpsc::Receiver<NetworkCommand>,
        cancellation_token: CancellationToken,
        max_nodes: u8,
        starting_nodes: u8,
    ) -> Self {
        debug!(target: "simulation::network", "node network built");

        NodeNetwork {
            nodes: HashMap::new(),
            addresses: HashMap::new(),
            network_event_tx,
            cancellation_token,
            network_start: Instant::now(),
            network_command_rx,
            max_nodes,
            starting_nodes,
        }
    }

    /// Builds and adds a new node into the network. Returns a Node or an Err when building the
    /// new node fails
    pub fn add_node(&mut self) -> Result<ConfiguredNode> {
        // Every node will be able to send network events and will have
        // a cancellation token to know when to stop
        let network_event_tx = self.network_event_tx.clone();
        let cancellation_token = self.cancellation_token.clone();

        let (node, tx) = ConfiguredNode::new(cancellation_token, network_event_tx)?;
        let listen_address = node.base.listen_address.clone();
        let peer_id = node.base.peer_id;

        self.nodes.insert(peer_id, tx);
        self.addresses.insert(peer_id, listen_address);

        Ok(node)
    }

    /// Main run loop of for the Node Network
    #[instrument(skip_all, name = "run network")]
    pub async fn run(&mut self) -> Result<(), AppError> {
        info!(target: "simulation::network", "node network running with {} nodes", self.nodes.len());

        self.network_start = Instant::now();

        let mut node_task_set: JoinSet<Result<NodeResult, NodeError>> = JoinSet::new();

        self.startup_starting_nodes(&mut node_task_set);

        loop {
            tokio::select! {
                () =  self.cancellation_token.cancelled() => {
                    debug!(target: "simulation::network", "cancellation token signal received");
                    break;
                }
                Some(command) = self.network_command_rx.recv() => {
                    debug!(target: "simulation::network", "network command received {:?}", command);

                    self.handle_network_command(command, &mut node_task_set).await;
                },
            }
        }

        info!(target: "simulation::network", "network now shutting down...");

        // Wait for all the nodes to finish
        while let Some(node_result) = node_task_set.join_next().await {
            match node_result {
                Ok(result) => {
                    info!(target: "simulation::network", "node exited successfully: {result:#?}");
                }
                Err(error) => {
                    error!(target: "simulation::network", "node exited with error: {error:#?}");
                }
            }
        }

        debug!(target: "simulation::network", "all node tasks exited");

        Ok(())
    }

    fn startup_starting_nodes(
        &mut self,
        node_task_set: &mut JoinSet<Result<NodeResult, NodeError>>,
    ) {
        for _ in 0..self.starting_nodes {
            // Build the new node
            let node = match self.add_node() {
                Ok(node) => node,
                Err(e) => {
                    warn!(target: "simulation::network", "failed to build node: {e}");
                    continue;
                }
            };

            // Every node will be able to send network events and will have
            // a cancellation token to know when to stop
            let _network_event_tx = self.network_event_tx.clone();
            let _cancellation_token = self.cancellation_token.clone();

            // Run the newly built node
            node_task_set.spawn(async move {
                let mut running_node = node.start();
                running_node.run().await
            });

            debug!(target: "simulation::network", "new node task spawned");
        }
    }

    /// Handle a network command received from the TUI. Takes in the network command and
    /// the task set of the running nodes. We return the task set with updates, if any
    async fn handle_network_command(
        &mut self,
        command: NetworkCommand,
        node_task_set: &mut JoinSet<Result<NodeResult, NodeError>>,
    ) {
        match command {
            NetworkCommand::ConnectNodes { peer_one, peer_two } => {
                if let Some(node_channel) = self.nodes.get(&peer_one)
                    && let Some(address) = self.addresses.get(&peer_two)
                {
                    let (tx, reply_rx) = oneshot::channel();
                    if let Err(e) = node_channel
                        .send((
                            NodeCommand::ConnectTo {
                                peer: address.to_owned(),
                            },
                            tx,
                        ))
                        .await
                    {
                        warn!(target: "simulation::network", "failed to send connect command: {e}");
                    }

                    let response = reply_rx.await;

                    match response {
                        Ok(NodeResponse::Dialed { addr }) => {
                            debug!(target: "simulation::network", "node dialed peer address: {addr}");
                        }
                        Ok(NodeResponse::Failed) => {
                            warn!(target: "simulation::network", "node failed to dial peer {peer_two}");
                        }
                        _ => {}
                    }
                }
            }
            NetworkCommand::DisconnectNodes { peer_one, peer_two } => {
                if let Some(node_channel) = self.nodes.get(&peer_one) {
                    let (tx, reply_rx) = oneshot::channel();
                    if let Err(e) = node_channel
                        .send((NodeCommand::DisconnectFrom { peer: peer_two }, tx))
                        .await
                    {
                        warn!(target: "simulation::network", "failed to send disconnect command: {e}");
                    }

                    let response = reply_rx.await;

                    match response {
                        Ok(NodeResponse::Disconnected { peer }) => {
                            debug!(target: "simulation::network", "nodes disconnected {peer_one} =/= {peer_two}");

                            if let Err(e) = self
                                .network_event_tx
                                .send(NetworkEvent::NodesDisconnected {
                                    peer_one,
                                    peer_two: peer,
                                })
                                .await
                            {
                                warn!(target: "simulation::network", "failed to send disconnect network event: {e}");
                            }
                        }
                        Ok(NodeResponse::Failed) => {
                            warn!(target: "simulation::network", "nodes failed to disconnect {peer_one} -> {peer_two}");
                        }
                        _ => {}
                    }
                }
            }
            NetworkCommand::GetIdentifyInfo { peer_id } => {
                if let Some(node_channel) = self.nodes.get(&peer_id) {
                    let (tx, reply_rx) = oneshot::channel();

                    if let Err(e) = node_channel.send((NodeCommand::GetIdentifyInfo, tx)).await {
                        warn!(target: "simulation::network", "failed to send identify info command: {e}");
                    }

                    let response = reply_rx.await;

                    match response {
                        Ok(NodeResponse::IdentifyInfo { info }) => {
                            debug!(target: "simulation::network", "received identify info: {:?}", info);

                            if let Err(e) = self
                                .network_event_tx
                                .send(NetworkEvent::IdentifyInfo { info })
                                .await
                            {
                                warn!(target: "simulation::network", "failed to send identify info network event: {e}");
                            }
                        }
                        Ok(NodeResponse::Failed) => {
                            warn!(target: "simulation::network", "nodes failed send identify info {peer_id}");
                        }
                        _ => {}
                    }
                }
            }
            NetworkCommand::GetKademliaInfo { peer_id } => {
                if let Some(node_channel) = self.nodes.get(&peer_id) {
                    let (tx, reply_rx) = oneshot::channel();

                    if let Err(e) = node_channel.send((NodeCommand::GetKademliaInfo, tx)).await {
                        warn!(target: "simulation::network", "failed to send kad info command: {e}");
                    }

                    let response = reply_rx.await;

                    match response {
                        Ok(NodeResponse::KademliaInfo { info }) => {
                            debug!(target: "simulation::network", "received kademlia info: {:?}", info );

                            if let Err(e) = self
                                .network_event_tx
                                .send(NetworkEvent::KademliaInfo { info })
                                .await
                            {
                                warn!(target: "simulation::network", "failed to send kad info network event: {e}");
                            }
                        }
                        Ok(NodeResponse::Failed) => {
                            warn!(target: "simulation::network", "nodes failed send kad info {peer_id}");
                        }
                        _ => {}
                    }
                }
            }
            NetworkCommand::StopNode { peer_id } => {
                if let Some(node_channel) = self.nodes.get(&peer_id) {
                    let (tx, reply_rx) = oneshot::channel();

                    if let Err(e) = node_channel.send((NodeCommand::Stop, tx)).await {
                        warn!(target: "simulation::network", "failed to send stop command: {e}");
                    }

                    let response = reply_rx.await;

                    match response {
                        Ok(NodeResponse::Stopped) => {
                            debug!(target: "simulation::network", "node stopped: {peer_id}");
                        }
                        Ok(NodeResponse::Failed) => {
                            warn!(target: "simulation::network", "nodes failed to stop: {peer_id}");
                        }
                        _ => {}
                    }
                }
            }
            NetworkCommand::StartNode => {
                // We limit the amount of nodes to 10
                if self.nodes.len() >= self.max_nodes as usize {
                    if let Err(e) = self.network_event_tx.send(NetworkEvent::MaxNodes).await {
                        warn!(target: "simulation::network", "failed to send max nodes network event: {e}");
                    }

                    return;
                }

                // Build the new node
                match self.add_node() {
                    Ok(node) => {
                        // Every node will be able to send network events and will have
                        // a cancellation token to know when to stop
                        let _network_event_tx = self.network_event_tx.clone();
                        let _cancellation_token = self.cancellation_token.clone();

                        // Start the new node
                        node_task_set.spawn(async move {
                            let mut running_node = node.start();
                            running_node.run().await
                        });
                    }
                    Err(e) => {
                        warn!(target: "simulation::network", "failed to build node: {e}");
                    }
                }

                debug!(target: "simulation::network", "new node task spawned");
            }
        }
    }
}

#[cfg(test)]
mod tests {

    #![allow(clippy::unwrap_used)]

    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::{
        messages::{NetworkCommand, NetworkEvent},
        network::NodeNetwork,
    };

    #[test]
    fn test_create_empty_network() {
        let (network_event_tx, _) = mpsc::channel::<NetworkEvent>(1);
        let (_, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
        let cancellation_token = CancellationToken::new();
        let node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            1,
            0,
        );

        assert!(node_network.nodes.is_empty());
        assert!(node_network.addresses.is_empty());
    }

    #[test]
    fn test_adding_new_node() {
        let (network_event_tx, _) = mpsc::channel::<NetworkEvent>(1);
        let (_, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            1,
            0,
        );

        let node = node_network.add_node().unwrap();

        assert!(node_network.nodes.contains_key(&node.base.peer_id));
        assert!(node_network.addresses.contains_key(&node.base.peer_id));
    }

    #[tokio::test]
    async fn test_max_nodes() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            1,
            1,
        );

        let _task = tokio::spawn(async move {
            let _ = node_network.run().await;
        });

        network_command_tx
            .send(NetworkCommand::StartNode)
            .await
            .unwrap();

        if let Some(event) = network_event_rx.recv().await {
            assert!(matches!(event, NetworkEvent::MaxNodes));
        } else {
            panic!()
        }
    }

    #[tokio::test]
    async fn test_connect_two_nodes() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            2,
            2,
        );
        let _task = tokio::spawn(async move {
            let _ = node_network.run().await;
        });

        let mut peer_ids = vec![];
        while peer_ids.len() < 2 {
            if let Some(NetworkEvent::NodeRunning { peer_id, .. }) = network_event_rx.recv().await {
                peer_ids.push(peer_id);
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
        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            1,
            1,
        );

        let _task = tokio::spawn(async move {
            let _ = node_network.run().await;
        });

        let mut peer_ids = vec![];
        while peer_ids.is_empty() {
            if let Some(NetworkEvent::NodeRunning { peer_id, .. }) = network_event_rx.recv().await {
                peer_ids.push(peer_id);
            }
        }

        network_command_tx
            .send(NetworkCommand::StopNode {
                peer_id: peer_ids[0],
            })
            .await
            .unwrap();

        loop {
            if let Some(NetworkEvent::NodeStopped { peer_id }) = network_event_rx.recv().await {
                assert!(peer_ids[0] == peer_id);
                break;
            }
        }
    }

    #[tokio::test]
    async fn test_start_node() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            1,
            0,
        );

        let _task = tokio::spawn(async move {
            let _ = node_network.run().await;
        });

        network_command_tx
            .send(NetworkCommand::StartNode)
            .await
            .unwrap();

        while let Some(event) = network_event_rx.recv().await {
            if let NetworkEvent::NodeRunning { .. } = event {
                assert!(matches!(event, NetworkEvent::NodeRunning { .. }));
                break;
            }
        }
    }

    #[tokio::test]
    async fn test_disconnect_two_nodes() {
        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(2);
        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(2);
        let cancellation_token = CancellationToken::new();
        let mut node_network = NodeNetwork::new(
            network_event_tx,
            network_command_rx,
            cancellation_token,
            2,
            2,
        );

        let _task = tokio::spawn(async move {
            let _ = node_network.run().await;
        });

        let mut peer_ids = vec![];
        while peer_ids.len() < 2 {
            if let Some(NetworkEvent::NodeRunning { peer_id, .. }) = network_event_rx.recv().await {
                peer_ids.push(peer_id);
            }
        }

        network_command_tx
            .send(NetworkCommand::ConnectNodes {
                peer_one: peer_ids[0],
                peer_two: peer_ids[1],
            })
            .await
            .unwrap();

        network_command_tx
            .send(NetworkCommand::DisconnectNodes {
                peer_one: peer_ids[0],
                peer_two: peer_ids[1],
            })
            .await
            .unwrap();

        loop {
            if let Some(NetworkEvent::NodesDisconnected { peer_one, peer_two }) =
                network_event_rx.recv().await
            {
                assert!(peer_ids[1] == peer_one);
                assert!(peer_ids[0] == peer_two);
                break;
            }
        }
    }
}
