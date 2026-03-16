use std::{collections::HashSet, sync::Arc, time::Instant};

use color_eyre::eyre::Result;
use libp2p::{Multiaddr, PeerId, core::ConnectedPoint, swarm::SwarmEvent};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    messages::{CommandChannel, NetworkEvent, NodeCommand, NodeResponse},
    node::{
        NodeError, NodeResult, NodeStats,
        base::NodeBase,
        behaviour::NodeNetworkEvent,
        connection_tracker::ConnectionTracker,
        history::{MessageHistory, SwarmEventInfo},
        identify_handler,
        kad_handler::{self, KadQueries},
    },
};
use tracing::{debug, error, info, instrument, trace, warn};

/// A node that is actively running in the network
pub(crate) struct RunningNode {
    /// The core/base of the node
    pub(crate) base: NodeBase,

    /// Manages the active connections to other peers in the libp2p swarm
    pub(crate) connection_tracker: ConnectionTracker,

    /// mpsc channel for receiving commands to perform from the network
    pub(crate) from_network: mpsc::Receiver<CommandChannel>,

    /// CancellationToken that shared network
    pub(crate) cancellation_token: CancellationToken,

    /// mpsc sender for Network Events
    pub(crate) network_event_tx: mpsc::Sender<NetworkEvent>,

    /// Flag for stopping the node
    pub(crate) quit: bool,

    /// Node killed by network command
    pub(crate) killed: bool,

    /// Instant when the node started running
    pub(crate) start: Instant,

    /// Logs for libp2p swarm events
    pub(crate) logs: Arc<RwLock<(MessageHistory, NodeStats)>>,

    /// Important kad queries that we must keep track of
    pub(crate) kad_queries: KadQueries,

    /// Flag for the status of the Kademlia bootstrap process
    pub(crate) bootstrapped: bool,
}

impl RunningNode {
    /// Main run loop of for the node
    #[instrument(skip_all, fields(id = %self.base.peer_id), name = "run node")]
    pub async fn run(&mut self) -> Result<NodeResult, NodeError> {
        info!(target: "simulation::node",
            "node {} now running",
            self.base.peer_id
        );

        // Send out event that the node is now running
        if let Err(e) = self
            .network_event_tx
            .send(NetworkEvent::NodeRunning {
                peer_id: self.base.peer_id,
                message_history: self.logs.clone(),
                node_connections: self.connection_tracker.connections(),
            })
            .await
        {
            warn!(target: "simulation::node", "failed to send node running event: {e}");
        }

        self.base.listen()?;
        info!(target: "simulation::node", "node swarm listening on {}", self.base.listen_address);

        self.dial_known_peers();

        while !self.quit {
            tokio::select! {
                _ = self.cancellation_token.cancelled() => {
                    debug!(target: "simulation::node", "cancellation token signal received");
                    break;
                },
                Some(message) = self.from_network.recv() => {
                    self.handle_network_message(message);
                },

                maybe_event = self.base.next_event() => {
                    if let Some(event) = maybe_event {
                        trace!("node swarm event {:?}", event);
                        self.logs.write().1.recvd_count += 1;

                        self.handle_swarm_event(event, self.network_event_tx.clone()).await;
                    } else {
                        error!(target: "simulation::node", "swarm stream ended");
                        return Err(NodeError::SwarmStreamEnded);
                    }
                },
            }
        }

        info!(target: "simulation::node", "node {} now shutting down", self.base.peer_id);

        // Send out event that the node has stopped running
        if let Err(e) = self
            .network_event_tx
            .send(NetworkEvent::NodeStopped {
                peer_id: self.base.peer_id,
            })
            .await
        {
            warn!(target: "simulation::node", "failed to send node stopped event: {e}");
        }

        if self.killed {
            Ok(NodeResult::Killed)
        } else {
            Ok(NodeResult::Success)
        }
    }

    /// Dial known peers passed by CLI and add to Kademlia routing table
    fn dial_known_peers(&mut self) {
        info!(target: "simulation::node", "dialing known peers");
        let mut dialed = 0;
        for addr in &self.connection_tracker.known() {
            if self.base.dial(addr.clone()).is_ok() {
                debug!(target: "simulation::node", "successfully dialed peer {addr}");
                dialed += 1
            } else {
                warn!(target: "simulation::node", "failed to dial peer {addr}");
            }
        }

        info!(target: "simulation::node", "dialed a total of {} peers", dialed);
    }

    /// Whether the node has completed the Kademlia bootstrap process
    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    /// Handle a message coming from the node network
    fn handle_network_message(&mut self, message: CommandChannel) {
        let (command, reply_tx) = message;
        if let Some(response) = self.handle_node_command(command) {
            if let Err(e) = reply_tx.send(response) {
                warn!(target: "simulation::node", "failed to send command response: {e:#?}");
            }
        }
    }
    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<NodeNetworkEvent>,
        network_event_tx: mpsc::Sender<NetworkEvent>,
    ) {
        let start = self.start;
        match event {
            SwarmEvent::Dialing { peer_id, .. } => {
                debug!(target: "simulation::node", "dialing peer {:?}", peer_id);
            }
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
                ..
            } => {
                self.logs.write().0.add_swarm_event(
                    SwarmEventInfo::NewListenAddr {
                        listener_id,
                        address,
                    },
                    Instant::now().duration_since(start),
                );
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                debug!(target: "simulation::node", "new peer {} from {:?}", peer_id, endpoint);

                let peer_addr = match endpoint.clone() {
                    ConnectedPoint::Dialer { address, .. } => address,
                    ConnectedPoint::Listener { send_back_addr, .. } => send_back_addr,
                };
                self.base.kad_add_address(&peer_id, peer_addr);

                self.connection_tracker.add_active_peer(peer_id);

                self.logs.write().0.add_swarm_event(
                    SwarmEventInfo::ConnectionEstablished { peer_id },
                    Instant::now().duration_since(start),
                );

                // Send event of node connections
                if let Err(e) = network_event_tx
                    .send(NetworkEvent::NodesConnected {
                        peer_one: peer_id,
                        peer_two: self.base.peer_id,
                    })
                    .await
                {
                    warn!(target: "simulation::node", "failed to send nodes connected event: {e}");
                }

                if !self.bootstrapped {
                    debug!(target: "simulation::node::kademlia_events", "attempting kademlia bootstrapping");
                    if let Ok(qid) = self.base.kad_bootstrap() {
                        debug!(target: "simulation::node::kademlia_events", "kademlia bootstrap started");
                        self.kad_queries.bootsrap_id = Some(qid);
                    } else {
                        warn!(target: "simulation::node::kademlia_events", "initial kademlia bootstrap failed");
                    }
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                endpoint,
                cause,
                ..
            } => {
                debug!(target: "simulation::node", "connection closed peer {} ({:?}) cause: {:?}", peer_id, endpoint, cause);

                self.logs.write().0.add_swarm_event(
                    SwarmEventInfo::ConnectionClosed { peer_id },
                    Instant::now().duration_since(start),
                );

                // Send event of nodes disconnecting
                if let Err(e) = network_event_tx
                    .send(NetworkEvent::NodesDisconnected {
                        peer_one: peer_id,
                        peer_two: self.base.peer_id,
                    })
                    .await
                {
                    warn!(target: "simulation::node", "failed to send nodes disconnected event: {e}");
                }

                self.connection_tracker.remove_active_peer(&peer_id);
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                ..
            } => {
                debug!(target: "simulation::node", "listener now closed");

                self.logs.write().0.add_swarm_event(
                    SwarmEventInfo::ListenerClosed {
                        listener_id,
                        addresses,
                    },
                    Instant::now().duration_since(start),
                );
            }
            SwarmEvent::IncomingConnectionError { peer_id, error, .. } => {
                error!(target: "simulation::node", "incoming connection failed, peer {:?}: {error}", peer_id);
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                error!(target: "simulation::node", "outgoing connection failed, peer {:?}: {error}", peer_id);
            }
            SwarmEvent::Behaviour(NodeNetworkEvent::Identify(event)) => {
                {
                    debug!(target: "simulation::node", "new identify event been added");
                }

                if let Err(e) = identify_handler::handle_event(self, event) {
                    warn!(target: "simulation::node", "failed to handle identify event: {e}");
                }
            }
            SwarmEvent::Behaviour(NodeNetworkEvent::Kademlia(event)) => {
                {
                    debug!(target: "simulation::node", "new kademlia event been added");
                }

                if let Err(e) = kad_handler::handle_event(self, event) {
                    warn!(target: "simulation::node", "failed to handle kad event: {e}");
                }
            }
            other => {
                debug!("new event: {:?}", other);
            }
        }
    }

    // Handles an incoming node command from the node network, returns the NodeResponse if any
    fn handle_node_command(&mut self, command: NodeCommand) -> Option<NodeResponse> {
        match command {
            NodeCommand::ConnectTo { peer } => {
                debug!(target: "simulation::node", "connect to command received: {peer}");
                match self.base.dial(peer.clone()) {
                    Ok(()) => {
                        debug!(target: "simulation::node", "successfully dialed peer {peer}");
                    }
                    Err(e) => {
                        warn!(target: "simulation::node", "failed to dial peer {peer}: {e}");
                    }
                }

                None
            }
            NodeCommand::DisconnectFrom { peer } => {
                debug!(target: "simulation::node", "disconnect from command received: {peer}");
                if self.base.disconnect_peer(peer).is_ok() {
                    debug!(target: "simulation::node", "successfully disconnected from {peer}");
                } else {
                    warn!(target: "simulation::node", "failed to disconnect from {peer}");
                }

                Some(NodeResponse::Disconnected { peer })
            }
            NodeCommand::GetIdentifyInfo => {
                let info = self.base.identify_info.clone();

                Some(NodeResponse::IdentifyInfo { info })
            }
            NodeCommand::GetKademliaInfo => {
                self.update_kademlia_info();
                let info = self.base.kad_info.clone();

                Some(NodeResponse::KademliaInfo { info })
            }
            NodeCommand::Stop => {
                debug!(target: "simulation::node", "stop command received");

                self.quit = true;
                self.killed = true;

                None
            }
        }
    }
    /// Returns the identify messages as strings
    pub fn identify_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().0;

        messages.identify_messages()
    }

    /// Returns the kademlia messages as strings
    pub fn kad_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().0;

        messages.kad_messages()
    }

    /// Returns the swarm messages as strings
    pub fn swarm_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().0;

        messages.swarm_messages()
    }

    /// Returns the all messages as strings
    pub fn all_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().0;

        messages.all_messages()
    }

    fn update_kademlia_info(&mut self) {
        // Get the closest peers
        let closest = self.base.kad_get_closest_local_peers(self.base.peer_id);

        debug!(target: "simulation::node", "the closest peers: {:?}", closest);

        self.base.kad_info.set_closest_peers(closest);

        // Get the non-empty kbuckets
        let buckets = self.base.kad_kbuckets();

        self.base.kad_info.set_bucket_info(buckets);
    }
}

#[cfg(test)]
mod tests {
    use libp2p::kad::Mode;
    use tokio::sync::{mpsc, oneshot};
    use tokio_util::sync::CancellationToken;

    use crate::{
        messages::{NetworkEvent, NodeCommand, NodeResponse},
        node::configured::ConfiguredNode,
    };

    #[test]
    fn test_new_node() {
        let cancellation_token = CancellationToken::new();
        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
        let (configured_node, _) = ConfiguredNode::new(cancellation_token, tx).unwrap();
        let node = configured_node.start();

        // Validate the new node is in the proper state
        assert!(!node.is_bootstrapped());
        assert!(!node.quit);
        assert_eq!(node.connection_tracker.connection_count(), 0);
        assert_eq!(node.connection_tracker.known_peers_count(), 0);
        assert!(node.logs.read().0.all_messages().is_empty());
        assert!(node.logs.read().1.recvd_count == 0);
        assert!(node.logs.read().1.sent_count == 0);

        assert_eq!(
            node.base.identify_info.public_key.to_peer_id(),
            node.base.peer_id
        );

        assert!(!node.base.kad_info.bootstrapped);
        assert_eq!(node.base.kad_info.mode, Mode::Client);
    }

    #[tokio::test]
    async fn test_handle_get_kad_info() {
        let cancellation_token = CancellationToken::new();
        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
        let (configured_node, command_tx) = ConfiguredNode::new(cancellation_token, tx).unwrap();
        let mut node = configured_node.start();
        let kad_info = node.base.kad_info.clone();

        let _task = tokio::task::spawn(async move {
            let _ = node.run().await;
        });

        let (tx, reply_rx) = oneshot::channel::<NodeResponse>();

        command_tx
            .send((NodeCommand::GetKademliaInfo, tx))
            .await
            .unwrap();

        let response = reply_rx.await.unwrap();

        assert_eq!(response, NodeResponse::KademliaInfo { info: kad_info });
    }

    #[tokio::test]
    async fn test_handle_get_identify_info() {
        let cancellation_token = CancellationToken::new();
        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
        let (configured_node, command_tx) = ConfiguredNode::new(cancellation_token, tx).unwrap();
        let mut node = configured_node.start();
        let identify_info = node.base.identify_info.clone();

        let _task = tokio::task::spawn(async move {
            let _ = node.run().await;
        });

        let (tx, reply_rx) = oneshot::channel::<NodeResponse>();

        command_tx
            .send((NodeCommand::GetIdentifyInfo, tx))
            .await
            .unwrap();

        let response = reply_rx.await.unwrap();

        assert_eq!(
            response,
            NodeResponse::IdentifyInfo {
                info: identify_info
            }
        );
    }
}
