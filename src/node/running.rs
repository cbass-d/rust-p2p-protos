use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
    time::Instant,
};

use color_eyre::eyre::Result;
use futures::StreamExt;
use libp2p::{Multiaddr, PeerId, Swarm, core::ConnectedPoint, swarm::SwarmEvent};
use tokio::sync::{mpsc, oneshot};

use crate::{
    messages::{NetworkEvent, NodeCommand, NodeResponse},
    node::{
        NodeResult, NodeStats,
        base::NodeBase,
        behaviour::NodeNetworkEvent,
        history::{MessageHistory, SwarmEventInfo, identify_event_to_string, kad_event_to_string},
        identify_handler,
        info::KBucketInfo,
        kad_handler::{self, KadQueries},
    },
};
use tracing::{debug, error, info, instrument, trace, warn};

/// A node that is actively running in the network
pub(crate) struct RunningNode {
    /// The core/base of the node
    pub(crate) base: NodeBase,

    /// Flag for stopping the node
    pub(crate) quit: bool,

    /// Instant when the node started running
    pub(crate) start: Instant,

    /// Logs for libp2p swarm events
    pub(crate) logs: Arc<RwLock<(MessageHistory, NodeStats)>>,

    /// Important kad queries that we must keep track of
    pub(crate) kad_queries: KadQueries,

    /// Flag for the status of the Kademlia bootstrap process
    pub(crate) bootstrapped: bool,

    /// The peers the node has active connections to
    pub(crate) current_peers: Arc<RwLock<HashSet<PeerId>>>,

    /// The peers the node knows at build time
    pub(crate) known_peers: Vec<Multiaddr>,
}

impl RunningNode {
    /// Main run loop of for the node
    #[instrument(skip_all, fields(id = %self.base.peer_id), name = "run node")]
    pub async fn run(&mut self) -> Result<NodeResult> {
        info!(target: "node",
            "node {} now running",
            self.base.peer_id
        );

        let _ = self
            .base
            .network_event_tx
            .send(NetworkEvent::NodeRunning {
                peer_id: self.base.peer_id,
                message_history: self.logs.clone(),
                node_connections: self.current_peers.clone(),
            })
            .await;

        self.base
            .swarm
            .listen_on(self.base.listen_address.clone())?;

        info!(target: "node", "node swarm listening on {}", self.base.listen_address);

        // Dial known peers passed by CLI and add to Kademlia routing table
        info!(target: "node", "dialing known peers");
        let mut dialed = 0;
        for addr in &self.known_peers {
            if self.base.swarm.dial(addr.clone()).is_ok() {
                debug!(target: "node", "successully dialed peer {addr}");
                dialed += 1
            } else {
                warn!(target: "node", "failed to dial peer {addr}");
            }
        }
        info!(target: "node", "dialed a total of {} peers", dialed);

        while !self.quit {
            tokio::select! {
                _ = self.base.cancellation_token.cancelled() => {
                    debug!(target: "node", "cancellation token signal received");
                    break;
                },
                Some(message) = self.base.from_network.recv() => {
                    self.handle_network_message(message);
                },

                Some(event) = self.base.swarm.next() => {
                    trace!("node swarm event {:?}", event);
                    self.logs.write().unwrap().1.recvd_count += 1;

                    self.handle_swarm_event(event, self.base.network_event_tx.clone()).await;
                },

            }
        }

        info!(target: "node", "node {} now shutting down", self.base.peer_id);

        let _ = self
            .base
            .network_event_tx
            .send(NetworkEvent::NodeStopped {
                peer_id: self.base.peer_id,
            })
            .await;

        Ok(NodeResult::Success)
    }

    /// Whether the node has completed the Kademlia bootstrap process
    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    /// Handle a message coming from the node network
    fn handle_network_message(&mut self, message: (NodeCommand, oneshot::Sender<NodeResponse>)) {
        let (command, reply_tx) = message;
        if let Some(response) = self.handle_node_command(command) {
            reply_tx.send(response).unwrap();
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
                debug!(target: "node", "dialing peer {:?}", peer_id);
            }
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
                ..
            } => {
                let local_p2p_addr = address
                    .clone()
                    .with_p2p(*self.base.swarm.local_peer_id())
                    .unwrap();
                debug!(target: "node", "listening on p2p address {:?}", local_p2p_addr);

                self.logs.write().unwrap().0.add_swarm_event(
                    SwarmEventInfo::NewListenAddr {
                        listener_id,
                        address,
                    },
                    Instant::now().duration_since(start).as_secs_f32(),
                );
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                debug!(target: "node", "new peer {} from {:?}", peer_id, endpoint);

                let peer_addr = match endpoint.clone() {
                    ConnectedPoint::Dialer { address, .. } => address,
                    ConnectedPoint::Listener { send_back_addr, .. } => send_back_addr,
                };
                self.base
                    .swarm
                    .behaviour_mut()
                    .kad
                    .add_address(&peer_id, peer_addr);

                self.current_peers.write().unwrap().insert(peer_id);

                self.logs.write().unwrap().0.add_swarm_event(
                    SwarmEventInfo::ConnectionEstablished { peer_id },
                    Instant::now().duration_since(start).as_secs_f32(),
                );

                // Send event of node connections
                network_event_tx
                    .send(NetworkEvent::NodesConnected {
                        peer_one: peer_id,
                        peer_two: self.base.peer_id,
                    })
                    .await
                    .unwrap();

                if !self.bootstrapped {
                    debug!(target: "kademlia_events", "attempting kademlia bootstrapping");
                    if let Ok(qid) = self.base.swarm.behaviour_mut().kad.bootstrap() {
                        debug!(target: "kademlia_events", "kademlia bootstrap started");
                        self.kad_queries.bootsrap_id = Some(qid);
                    } else {
                        warn!(target: "kademlia_events", "initial kademlia bootstrap failed");
                    }
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                endpoint,
                cause,
                ..
            } => {
                debug!(target: "node", "connection closed peer {} ({:?}) cause: {:?}", peer_id, endpoint, cause);

                self.logs.write().unwrap().0.add_swarm_event(
                    SwarmEventInfo::ConnectionClosed { peer_id },
                    Instant::now().duration_since(start).as_secs_f32(),
                );

                // Send event of nodes disconnecting
                network_event_tx
                    .send(NetworkEvent::NodesDisconnected {
                        peer_one: peer_id,
                        peer_two: self.base.peer_id,
                    })
                    .await
                    .unwrap();

                let mut current_peers = self.current_peers.write().unwrap();
                current_peers.remove(&peer_id);
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                ..
            } => {
                debug!(target: "node", "listener now closed");

                self.logs.write().unwrap().0.add_swarm_event(
                    SwarmEventInfo::ListenerClosed {
                        listener_id,
                        addresses,
                    },
                    Instant::now().duration_since(start).as_secs_f32(),
                );
            }
            SwarmEvent::IncomingConnectionError { peer_id, error, .. } => {
                error!(target: "node", "incoming connection failed, peer {:?}: {error}", peer_id);
            }
            SwarmEvent::Behaviour(NodeNetworkEvent::Identify(event)) => {
                {
                    debug!(target: "node", "new identify event been added");

                    let event_string = identify_event_to_string(&event);

                    self.logs.write().unwrap().0.add_identify_event(
                        event_string,
                        Instant::now().duration_since(start).as_secs_f32(),
                    );
                }

                identify_handler::handle_event(self, event).unwrap();
            }
            SwarmEvent::Behaviour(NodeNetworkEvent::Kademlia(event)) => {
                {
                    debug!(target: "node", "new kademlia event been added");
                    let event_string = kad_event_to_string(&event);

                    self.logs.write().unwrap().0.add_kademlia_event(
                        event_string,
                        Instant::now().duration_since(start).as_secs_f32(),
                    );
                }

                kad_handler::handle_event(self, event).unwrap();
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
                debug!(target: "node", "connect to command recieved: {peer}");
                if self.base.swarm.dial(peer.clone()).is_ok() {
                    debug!(target: "node", "successully dialed peer {peer}");
                    self.known_peers.push(peer);
                } else {
                    warn!(target: "node", "failed to dial peer {peer}");
                }

                None
            }
            NodeCommand::DisconnectFrom { peer } => {
                debug!(target: "node", "disconnect from command received: {peer}");
                if self.base.swarm.disconnect_peer_id(peer).is_ok() {
                    debug!(target: "node", "successully disconnected from {peer}");

                    let mut current_peers = self.current_peers.write().unwrap();
                    current_peers.remove(&peer);
                } else {
                    warn!(target: "node", "failed to disconnect from {peer}");
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
                debug!(target: "node", "stop command received");

                self.quit = true;

                None
            }
        }
    }
    /// Returns the identify messages as strings
    pub fn identify_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().unwrap().0;

        messages.identify_messages()
    }

    /// Returns the kademlia messages as strings
    pub fn kad_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().unwrap().0;

        messages.kad_messages()
    }

    /// Returns the swarm messages as strings
    pub fn swarm_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().unwrap().0;

        messages.swarm_messages()
    }

    /// Returns the all messages as strings
    pub fn all_messages(&self) -> Vec<String> {
        let messages = &self.logs.read().unwrap().0;

        messages.all_messages()
    }

    fn update_kademlia_info(&mut self) {
        // Get the closest peers
        let peer_key = self.base.peer_id.into();
        let closest = self
            .base
            .swarm
            .behaviour_mut()
            .kad
            .get_closest_local_peers(&peer_key);

        let closest: Vec<PeerId> = closest.map(|k| k.into_preimage()).collect();
        debug!(target: "node", "the closet peers: {:?}", closest);

        self.base.kad_info.set_closest_peers(closest);

        // Get the non-empty kbuckets
        let buckets = self.base.swarm.behaviour_mut().kad.kbuckets();
        let mut bucket_info: Vec<KBucketInfo> = vec![];
        buckets.for_each(|kb| {
            bucket_info.push(KBucketInfo {
                range: (kb.range().0.0, kb.range().1.0),
                num_entries: kb.num_entries(),
            });
        });

        self.base.kad_info.set_bucket_info(bucket_info);
    }
}

#[cfg(test)]
mod tests {
    use libp2p::kad::Mode;
    use tokio::{
        sync::{mpsc, oneshot},
        time::Instant,
    };
    use tokio_util::sync::CancellationToken;

    use crate::{
        messages::{NetworkEvent, NodeCommand, NodeResponse},
        node::configured::ConfiguredNode,
    };

    #[test]
    fn test_new_node() {
        let cancellation_token = CancellationToken::new();
        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
        let (configured_node, _) = ConfiguredNode::new(cancellation_token, tx);
        let node = configured_node.start();

        // Validate the new node is in the proper state
        assert!(!node.is_bootstrapped());
        assert!(!node.quit);
        assert!(node.current_peers.read().unwrap().is_empty());
        assert!(node.known_peers.is_empty());
        assert!(node.logs.read().unwrap().0.all_messages().is_empty());
        assert!(node.logs.read().unwrap().1.recvd_count == 0);
        assert!(node.logs.read().unwrap().1.sent_count == 0);

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
        let (configured_node, command_tx) = ConfiguredNode::new(cancellation_token, tx);
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
        let (configured_node, command_tx) = ConfiguredNode::new(cancellation_token, tx);
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
