mod behaviour;
pub mod history;
mod identify_handler;
pub mod info;
mod kad_handler;

use color_eyre::eyre::Result;
use core::fmt;
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, StreamProtocol, Swarm, Transport,
    core::{
        ConnectedPoint,
        transport::{MemoryTransport, upgrade},
    },
    identify::{Behaviour as Identify, Config as IdentifyConfig},
    identity,
    kad::{Behaviour as Kademlia, store::MemoryStore},
    multiaddr::Protocol,
    noise,
    swarm::{self, SwarmEvent},
    yamux,
};
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    messages::{NetworkEvent, NodeCommand, NodeResponse},
    node::{
        behaviour::{NodeBehaviour, NodeNetworkEvent},
        history::{MessageHistory, SwarmEventInfo, identify_event_to_string, kad_event_to_string},
        info::{IdentifyInfo, KBucketInfo, KademliaInfo},
        kad_handler::KadQueries,
    },
};

const IPFS_KAD_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");
const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/id/1.0.0");
const NODE_NETWORK_AGENT: &str = "node-network/0.1";

/// The result after a node has stopped running
#[derive(Debug, Clone)]
pub enum NodeResult {
    Success,
    Error(String),
}

/// The number of messages/swarm events the node has sent and received
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

/// Node structure representing a peer participant in the network
pub struct Node {
    /// PeerId of the node in the libp2p swarm network
    pub peer_id: PeerId,

    /// mpsc channel for receiving commands to perform from the network
    from_network: mpsc::Receiver<(NodeCommand, oneshot::Sender<NodeResponse>)>,

    /// The peers the node has active connections to
    current_peers: Arc<RwLock<HashSet<PeerId>>>,

    /// The peers the node knows or has previously seen
    known_peers: Vec<Multiaddr>,

    /// Logs for libp2p swarm events
    logs: Arc<RwLock<(MessageHistory, NodeStats)>>,
    kad_queries: KadQueries,

    /// libp2p swarm listen address
    pub listen_address: Multiaddr,

    /// Flag for stopping the node
    quit: bool,

    /// Instant when the node started running
    start: Option<Instant>,

    /// Flag for the status of the Kademlia bootstrap process
    bootstrapped: bool,

    /// The custom libp2p swarm behaviour
    /// - identify
    /// - kademlia
    swarm: Swarm<NodeBehaviour>,

    /// Struct to hold local identify info (info that is pushed to other peers)
    identify_info: IdentifyInfo,

    /// Struct to hold local kademlia info
    kad_info: KademliaInfo,
}

impl Node {
    /// Constructs a new node with the provided addresss.
    /// Also takes in the write end for an mpsc channel used to
    /// send messages to the network, as well as the reciever for
    /// the broadccast channel where the kill signal will be sent at
    /// program shutdown
    ///
    /// Returns the constructed node as well as the write end of the mpsc
    /// channel where the node will be recieving messages
    pub fn new() -> Result<(
        Self,
        mpsc::Sender<(NodeCommand, oneshot::Sender<NodeResponse>)>,
        Multiaddr,
    )> {
        // Build the mpsc channel where the channel will be recieving messages from
        let (tx, rx) = mpsc::channel(100);

        let node_keys = identity::Keypair::generate_ed25519();
        let peer_id = node_keys.public().to_peer_id();
        let node_transport = MemoryTransport::default()
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::Config::new(&node_keys)?)
            .multiplex(yamux::Config::default())
            .boxed();

        let listen_address = Multiaddr::from(Protocol::Memory(rand::random::<u64>()));

        let store = MemoryStore::new(peer_id);
        let kad = Kademlia::new(peer_id, store);
        let identify = {
            let cfg = IdentifyConfig::new(IPFS_PROTO_NAME.to_string(), node_keys.public())
                .with_agent_version(NODE_NETWORK_AGENT.to_string());

            Identify::new(cfg)
        };
        let behaviour = NodeBehaviour { kad, identify };

        let swarm = Swarm::new(
            node_transport,
            behaviour,
            peer_id,
            swarm::Config::with_tokio_executor(),
        );

        let logs = Arc::new(RwLock::new((
            MessageHistory::default(),
            NodeStats::default(),
        )));

        let identify_info = IdentifyInfo::new(
            node_keys.public(),
            IPFS_PROTO_NAME.to_string(),
            NODE_NETWORK_AGENT.to_string(),
            listen_address.clone(),
        );

        let kad_info = KademliaInfo::new(swarm.behaviour().kad.mode(), false);

        let node = Node {
            peer_id,
            from_network: rx,
            current_peers: Arc::new(RwLock::new(HashSet::new())),
            known_peers: vec![],
            quit: false,
            logs: logs.clone(),
            kad_queries: KadQueries::default(),
            start: None,
            bootstrapped: false,
            swarm,
            identify_info,
            kad_info,
            listen_address: listen_address.clone(),
        };

        Ok((node, tx, listen_address))
    }

    /// Whether the node has completed the Kademlia bootstrap process
    pub fn is_bootstrapped(&self) -> bool {
        self.bootstrapped
    }

    /// Main run loop of for the node
    #[instrument(skip_all, fields(id = %self.peer_id), name = "run node")]
    pub async fn run(
        &mut self,
        start: Instant,
        network_event_tx: mpsc::Sender<NetworkEvent>,
        cancellation_token: CancellationToken,
    ) -> Result<NodeResult> {
        info!(target: "node",
            "node {} now running",
            self.peer_id
        );

        self.start = Some(start);

        let _ = network_event_tx
            .send(NetworkEvent::NodeRunning {
                peer_id: self.peer_id,
                message_history: self.logs.clone(),
                node_connections: self.current_peers.clone(),
            })
            .await;

        self.swarm.listen_on(self.listen_address.clone())?;

        info!(target: "node", "node swarm listening on {}", self.listen_address);

        // Dial known peers passed by CLI and add to Kademlia routing table
        info!(target: "node", "dialing known peers");
        let mut dialed = 0;
        for addr in &self.known_peers {
            if self.swarm.dial(addr.clone()).is_ok() {
                debug!(target: "node", "successully dialed peer {addr}");
                dialed += 1
            } else {
                warn!(target: "node", "failed to dial peer {addr}");
            }
        }
        info!(target: "node", "dialed a total of {} peers", dialed);

        //if !self.known_peers.is_empty() {}

        while !self.quit {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    debug!(target: "node", "cancellation token signal received");
                    break;
                },
                Some(message) = self.from_network.recv() => {
                    self.handle_network_message(message);
                },

                Some(event) = self.swarm.next() => {
                    trace!("node swarm event {:?}", event);
                    self.logs.write().unwrap().1.recvd_count += 1;

                    self.handle_swarm_event(event, network_event_tx.clone()).await;
                },

            }
        }

        info!(target: "node", "node {} now shutting down", self.peer_id);

        let _ = network_event_tx
            .send(NetworkEvent::NodeStopped {
                peer_id: self.peer_id,
            })
            .await;

        Ok(NodeResult::Success)
    }

    /// Handle a message coming from the node network
    fn handle_network_message(&mut self, message: (NodeCommand, oneshot::Sender<NodeResponse>)) {
        let (command, reply_tx) = message;
        if let Some(response) = self.handle_node_command(command) {
            reply_tx.send(response).unwrap();
        }
    }

    /// Handles an node libp2p swarm event
    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<NodeNetworkEvent>,
        network_event_tx: mpsc::Sender<NetworkEvent>,
    ) {
        let start = self.start.unwrap();
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
                    .with_p2p(*self.swarm.local_peer_id())
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
                self.swarm
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
                        peer_two: self.peer_id,
                    })
                    .await
                    .unwrap();

                if !self.bootstrapped {
                    debug!(target: "kademlia_events", "attempting kademlia bootstrapping");
                    if let Ok(qid) = self.swarm.behaviour_mut().kad.bootstrap() {
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
                        peer_two: self.peer_id,
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

                identify_handler::handle_event(self, event);
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

                kad_handler::handle_event(self, event);
            }
            other => {
                debug!("new event: {:?}", other);
            }
        }
    }

    /// Handles an incoming node command from the node network, returns the NodeResponse if any
    fn handle_node_command(&mut self, command: NodeCommand) -> Option<NodeResponse> {
        match command {
            NodeCommand::ConnectTo { peer } => {
                debug!(target: "node", "connect to command recieved: {peer}");
                if self.swarm.dial(peer.clone()).is_ok() {
                    debug!(target: "node", "successully dialed peer {peer}");
                    self.known_peers.push(peer);
                } else {
                    warn!(target: "node", "failed to dial peer {peer}");
                }

                None
            }
            NodeCommand::DisconnectFrom { peer } => {
                debug!(target: "node", "disconnect from command received: {peer}");
                if self.swarm.disconnect_peer_id(peer).is_ok() {
                    debug!(target: "node", "successully disconnected from {peer}");

                    let mut current_peers = self.current_peers.write().unwrap();
                    current_peers.remove(&peer);
                } else {
                    warn!(target: "node", "failed to disconnect from {peer}");
                }

                Some(NodeResponse::Disconnected { peer })
            }
            NodeCommand::GetIdentifyInfo => {
                let info = self.identify_info.clone();

                Some(NodeResponse::IdentifyInfo { info })
            }
            NodeCommand::GetKademliaInfo => {
                self.update_kademlia_info();
                let info = self.kad_info.clone();

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
        let peer_key = self.peer_id.into();
        let closest = self
            .swarm
            .behaviour_mut()
            .kad
            .get_closest_local_peers(&peer_key);

        let closest: Vec<PeerId> = closest.map(|k| k.into_preimage()).collect();
        debug!(target: "node", "the closet peers: {:?}", closest);

        self.kad_info.set_closest_peers(closest);

        // Get the non-empty kbuckets
        let buckets = self.swarm.behaviour_mut().kad.kbuckets();
        let mut bucket_info: Vec<KBucketInfo> = vec![];
        buckets.for_each(|kb| {
            bucket_info.push(KBucketInfo {
                range: (kb.range().0.0, kb.range().1.0),
                num_entries: kb.num_entries(),
            });
        });

        self.kad_info.set_bucket_info(bucket_info);
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node: {}", self.peer_id)
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
        node::Node,
    };

    #[test]
    fn test_new_node() {
        let (node, _, listen_address) = Node::new().unwrap();

        // Validate the new node is in the proper state
        assert!(!node.is_bootstrapped());
        assert!(!node.quit);
        assert!(node.current_peers.read().unwrap().is_empty());
        assert!(node.known_peers.is_empty());
        assert!(node.logs.read().unwrap().0.all_messages().is_empty());
        assert!(node.logs.read().unwrap().1.recvd_count == 0);
        assert!(node.logs.read().unwrap().1.sent_count == 0);

        assert!(node.listen_address == listen_address);
        assert!(node.identify_info.listen_addr == listen_address);
        assert!(node.identify_info.public_key.to_peer_id() == node.peer_id);

        assert!(!node.kad_info.bootstrapped);
        assert!(node.kad_info.mode == Mode::Client);
    }

    #[tokio::test]
    async fn test_handle_get_kad_info() {
        let (mut node, command_tx, _listen_address) = Node::new().unwrap();
        let kad_info = node.kad_info.clone();

        let (network_tx, _network_rx) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let _task = tokio::task::spawn(async move {
            let _ = node
                .run(Instant::now(), network_tx, cancellation_token)
                .await;
        });

        let (tx, reply_rx) = oneshot::channel::<NodeResponse>();

        command_tx
            .send((NodeCommand::GetKademliaInfo, tx))
            .await
            .unwrap();

        let response = reply_rx.await.unwrap();

        assert!(response == NodeResponse::KademliaInfo { info: kad_info });
    }

    #[tokio::test]
    async fn test_handle_get_identify_info() {
        let (mut node, command_tx, _listen_address) = Node::new().unwrap();
        let identify_info = node.identify_info.clone();

        let (network_tx, _network_rx) = mpsc::channel::<NetworkEvent>(1);
        let cancellation_token = CancellationToken::new();
        let _task = tokio::task::spawn(async move {
            let _ = node
                .run(Instant::now(), network_tx, cancellation_token)
                .await;
        });

        let (tx, reply_rx) = oneshot::channel::<NodeResponse>();

        command_tx
            .send((NodeCommand::GetIdentifyInfo, tx))
            .await
            .unwrap();

        let response = reply_rx.await.unwrap();

        assert!(
            response
                == NodeResponse::IdentifyInfo {
                    info: identify_info
                }
        );
    }
}
