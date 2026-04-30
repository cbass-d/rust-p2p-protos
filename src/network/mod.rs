use color_eyre::eyre::Result;
use libp2p::{Multiaddr, PeerId, kad::Mode};
use std::{collections::HashMap, net::Ipv4Addr};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinSet,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    bus::EventBus,
    error::AppError,
    messages::{NetworkCommand, NetworkEvent, NodeCommand, NodeResponse},
    node::{NodeError, NodeResult, configured::ConfiguredNode},
};

mod handlers;

/// Transport the nodes operate on.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum TransportMode {
    #[default]
    Memory,
    Tcp,
}

/// Owns the node command channels and address book, and routes TUI
/// commands to the per-protocol handlers.
#[derive(Debug)]
pub(crate) struct NodeNetwork {
    /// Mapping from peer id to mpsc channel for sending commands to node
    nodes: HashMap<PeerId, mpsc::Sender<(NodeCommand, oneshot::Sender<NodeResponse>)>>,

    /// Transport mode
    pub(crate) transport: TransportMode,

    /// Bind address for the nodes
    pub(crate) bind_address: Option<Ipv4Addr>,

    /// Mapping from peer id to nodes libp2p multi-address
    addresses: HashMap<PeerId, Multiaddr>,

    /// Broadcast bus for Network Events. Each subscriber gets its own
    /// independent receiver; slow subscribers are signaled via `Lagged(n)`.
    network_event_tx: EventBus<NetworkEvent>,

    /// Our own subscription to the bus, used to observe `NodeListening`
    /// events and update `addresses` with post-bind multiaddrs.
    network_event_rx: broadcast::Receiver<NetworkEvent>,

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
    /// Builds a new node network with the provided event bus, cancellation token, and starting
    /// node count.
    pub fn new(
        network_event_tx: EventBus<NetworkEvent>,
        network_command_rx: mpsc::Receiver<NetworkCommand>,
        cancellation_token: CancellationToken,
        max_nodes: u8,
        starting_nodes: u8,
        transport: TransportMode,
        bind_address: Option<Ipv4Addr>,
    ) -> Self {
        debug!(target: "simulation::network", "node network built");

        let network_event_rx = network_event_tx.subscribe();

        NodeNetwork {
            nodes: HashMap::new(),
            addresses: HashMap::new(),
            network_event_tx,
            network_event_rx,
            cancellation_token,
            network_start: Instant::now(),
            network_command_rx,
            transport,
            bind_address,
            max_nodes,
            starting_nodes,
        }
    }

    /// Builds and adds a new node into the network. Returns a Node or an Err when building the
    /// new node fails
    pub fn add_node(&mut self, kad_mode: Mode) -> Result<ConfiguredNode> {
        // Every node will be able to send network events and will have
        // a cancellation token to know when to stop
        let network_event_tx = self.network_event_tx.clone();
        let cancellation_token = self.cancellation_token.clone();

        let (node, tx) = ConfiguredNode::new(
            cancellation_token,
            network_event_tx,
            self.transport,
            self.bind_address,
            kad_mode,
        )?;
        let listen_address = node.base.listen_address.clone();
        let peer_id = node.base.peer_id;

        self.nodes.insert(peer_id, tx);
        self.addresses.insert(peer_id, listen_address);

        Ok(node)
    }

    /// Main run loop of for the Node Network
    #[instrument(skip_all, name = "run network")]
    pub async fn run(&mut self) -> Result<(), AppError> {
        info!(target: "simulation::network", node_count = self.nodes.len(), "node network running");

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
                    debug!(target: "simulation::network", ?command, "network command received");

                    self.handle_network_command(command, &mut node_task_set).await;
                },
                event = self.network_event_rx.recv() => {
                    match event {
                        Ok(NetworkEvent::NodeListening { peer_id, address }) => {
                            debug!(target: "simulation::network", %peer_id, %address, "updating node listen address");
                            self.addresses.insert(peer_id, address);
                        }
                        Ok(_) => {},
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: "simulation::network", skipped = n, "event bus lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                Some(node_result) = node_task_set.join_next() => {
                    match node_result {
                        Ok(result) => {
                            info!(target: "simulation::network", ?result, "node exited");
                        }
                        Err(error) => {
                            error!(target: "simulation::network", %error, "node task panicked");
                        }
                    }

                    if node_task_set.is_empty() {
                        return Err(AppError::AllNodesExited);
                    }
                }
            }
        }

        info!(target: "simulation::network", "network now shutting down...");

        // Wait for all the nodes to finish
        while let Some(node_result) = node_task_set.join_next().await {
            match node_result {
                Ok(result) => {
                    info!(target: "simulation::network", ?result, "node exited");
                }
                Err(error) => {
                    error!(target: "simulation::network", %error, "node task panicked");
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
            let node = match self.add_node(Mode::Server) {
                Ok(node) => node,
                Err(e) => {
                    warn!(target: "simulation::network", error = %e, "failed to build node");
                    continue;
                }
            };

            // Every node will be able to send network events and will have
            // a cancellation token to know when to stop
            let _network_event_tx = self.network_event_tx.clone();
            let _cancellation_token = self.cancellation_token.clone();

            // Run the newly built node
            node_task_set.spawn(async move {
                let mut running_node = node.build();
                running_node.run().await
            });

            debug!(target: "simulation::network", "new node task spawned");
        }
    }

    /// Handle a network command received from the TUI. Delegates to a
    /// per-protocol handler module in `src/network/handlers/`.
    #[instrument(skip_all, name = "network_command")]
    async fn handle_network_command(
        &mut self,
        command: NetworkCommand,
        node_task_set: &mut JoinSet<Result<NodeResult, NodeError>>,
    ) {
        let mut ctx = handlers::ControlCtx {
            nodes: &mut self.nodes,
            addresses: &mut self.addresses,
            network_event_tx: &self.network_event_tx,
            max_nodes: self.max_nodes,
            transport: self.transport,
            bind_address: self.bind_address,
            cancellation_token: &self.cancellation_token,
        };

        match command {
            NetworkCommand::Swarm(c) => handlers::handle_swarm(c, &mut ctx).await,
            NetworkCommand::Identify(c) => handlers::handle_identify(c, &mut ctx).await,
            NetworkCommand::Kademlia(c) => handlers::handle_kademlia(c, &mut ctx).await,
            NetworkCommand::Lifecycle(c) => {
                handlers::handle_lifecycle(c, &mut ctx, node_task_set).await;
            }
        }
    }
}

//#[cfg(test)]
//mod tests {
//
//    #![allow(clippy::unwrap_used)]
//
//    use tokio::sync::mpsc;
//    use tokio_util::sync::CancellationToken;
//
//    use crate::{
//        messages::{NetworkCommand, NetworkEvent},
//        network::NodeNetwork,
//    };
//
//    #[test]
//    fn test_create_empty_network() {
//        let (network_event_tx, _) = mpsc::channel::<NetworkEvent>(1);
//        let (_, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
//        let cancellation_token = CancellationToken::new();
//        let node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            1,
//            0,
//        );
//
//        assert!(node_network.nodes.is_empty());
//        assert!(node_network.addresses.is_empty());
//    }
//
//    #[test]
//    fn test_adding_new_node() {
//        let (network_event_tx, _) = mpsc::channel::<NetworkEvent>(1);
//        let (_, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
//        let cancellation_token = CancellationToken::new();
//        let mut node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            1,
//            0,
//        );
//
//        let node = node_network.add_node().unwrap();
//
//        assert!(node_network.nodes.contains_key(&node.base.peer_id));
//        assert!(node_network.addresses.contains_key(&node.base.peer_id));
//    }
//
//    #[tokio::test]
//    async fn test_max_nodes() {
//        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
//        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
//        let cancellation_token = CancellationToken::new();
//        let mut node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            1,
//            1,
//        );
//
//        let _task = tokio::spawn(async move {
//            let _ = node_network.run().await;
//        });
//
//        network_command_tx
//            .send(NetworkCommand::StartNode)
//            .await
//            .unwrap();
//
//        if let Some(event) = network_event_rx.recv().await {
//            assert!(matches!(event, NetworkEvent::MaxNodes));
//        } else {
//            panic!()
//        }
//    }
//
//    #[tokio::test]
//    async fn test_connect_two_nodes() {
//        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
//        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
//        let cancellation_token = CancellationToken::new();
//        let mut node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            2,
//            2,
//        );
//        let _task = tokio::spawn(async move {
//            let _ = node_network.run().await;
//        });
//
//        let mut peer_ids = vec![];
//        while peer_ids.len() < 2 {
//            if let Some(NetworkEvent::NodeRunning { peer_id, .. }) = network_event_rx.recv().await {
//                peer_ids.push(peer_id);
//            }
//        }
//
//        network_command_tx
//            .send(NetworkCommand::ConnectNodes {
//                peer_one: peer_ids[0],
//                peer_two: peer_ids[1],
//            })
//            .await
//            .unwrap();
//
//        while let Some(event) = network_event_rx.recv().await {
//            if let NetworkEvent::NodesConnected { peer_one, peer_two } = event {
//                assert!(peer_ids[0] == peer_one);
//                assert!(peer_ids[1] == peer_two);
//                break;
//            }
//        }
//    }
//
//    #[tokio::test]
//    async fn test_stop_node() {
//        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
//        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
//        let cancellation_token = CancellationToken::new();
//        let mut node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            1,
//            1,
//        );
//
//        let _task = tokio::spawn(async move {
//            let _ = node_network.run().await;
//        });
//
//        let mut peer_ids = vec![];
//        while peer_ids.is_empty() {
//            if let Some(NetworkEvent::NodeRunning { peer_id, .. }) = network_event_rx.recv().await {
//                peer_ids.push(peer_id);
//            }
//        }
//
//        network_command_tx
//            .send(NetworkCommand::StopNode {
//                peer_id: peer_ids[0],
//            })
//            .await
//            .unwrap();
//
//        loop {
//            if let Some(NetworkEvent::NodeStopped { peer_id }) = network_event_rx.recv().await {
//                assert!(peer_ids[0] == peer_id);
//                break;
//            }
//        }
//    }
//
//    #[tokio::test]
//    async fn test_start_node() {
//        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(1);
//        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(1);
//        let cancellation_token = CancellationToken::new();
//        let mut node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            1,
//            0,
//        );
//
//        let _task = tokio::spawn(async move {
//            let _ = node_network.run().await;
//        });
//
//        network_command_tx
//            .send(NetworkCommand::StartNode)
//            .await
//            .unwrap();
//
//        while let Some(event) = network_event_rx.recv().await {
//            if let NetworkEvent::NodeRunning { .. } = event {
//                assert!(matches!(event, NetworkEvent::NodeRunning { .. }));
//                break;
//            }
//        }
//    }
//
//    #[tokio::test]
//    async fn test_disconnect_two_nodes() {
//        let (network_event_tx, mut network_event_rx) = mpsc::channel::<NetworkEvent>(2);
//        let (network_command_tx, network_command_rx) = mpsc::channel::<NetworkCommand>(2);
//        let cancellation_token = CancellationToken::new();
//        let mut node_network = NodeNetwork::new(
//            network_event_tx,
//            network_command_rx,
//            cancellation_token,
//            2,
//            2,
//        );
//
//        let _task = tokio::spawn(async move {
//            let _ = node_network.run().await;
//        });
//
//        let mut peer_ids = vec![];
//        while peer_ids.len() < 2 {
//            if let Some(NetworkEvent::NodeRunning { peer_id, .. }) = network_event_rx.recv().await {
//                peer_ids.push(peer_id);
//            }
//        }
//
//        network_command_tx
//            .send(NetworkCommand::ConnectNodes {
//                peer_one: peer_ids[0],
//                peer_two: peer_ids[1],
//            })
//            .await
//            .unwrap();
//
//        network_command_tx
//            .send(NetworkCommand::DisconnectNodes {
//                peer_one: peer_ids[0],
//                peer_two: peer_ids[1],
//            })
//            .await
//            .unwrap();
//
//        loop {
//            if let Some(NetworkEvent::NodesDisconnected { peer_one, peer_two }) =
//                network_event_rx.recv().await
//            {
//                assert!(peer_ids[1] == peer_one);
//                assert!(peer_ids[0] == peer_two);
//                break;
//            }
//        }
//    }
//}
