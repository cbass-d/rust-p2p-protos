mod behaviour;
pub mod history;
mod identify_handler;
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
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    messages::{NetworkEvent, NodeCommand},
    node::{
        behaviour::{NodeBehaviour, NodeNetworkEvent},
        history::{MessageHistory, SwarmEventInfo},
        kad_handler::KadQueries,
    },
};

const IPFS_KAD_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");
const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/id/1.0.0");
const NODE_NETWORK_AGENT: &str = "node-network/0.1";

#[derive(Debug, Clone)]
pub enum NodeResult {
    Success,
    Error(String),
}

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

// Node strucure representing a peer or participant in the network
pub struct Node {
    pub peer_id: PeerId,
    to_network: mpsc::Sender<NodeCommand>,
    from_network: mpsc::Receiver<NodeCommand>,
    current_peers: HashSet<PeerId>,
    known_peers: Vec<Multiaddr>,

    message_history: Arc<RwLock<MessageHistory>>,
    kad_queries: KadQueries,

    // libp2p swarm listen address
    pub listen_address: Multiaddr,

    bootstrapped: bool,
    swarm: Swarm<NodeBehaviour>,

    pub stats: Arc<Mutex<NodeStats>>,
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
    pub fn new(to_network: mpsc::Sender<NodeCommand>) -> Result<(Self, mpsc::Sender<NodeCommand>)> {
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

        let message_history = Arc::new(RwLock::new(MessageHistory::default()));

        let node = Node {
            peer_id,
            to_network,
            from_network: rx,
            current_peers: HashSet::new(),
            known_peers: vec![],
            message_history: message_history.clone(),
            kad_queries: KadQueries::default(),
            bootstrapped: false,
            swarm,
            listen_address,
            stats: Arc::new(Mutex::new(NodeStats::default())),
        };

        Ok((node, tx))
    }

    pub fn message_history_copy(&self) -> Arc<RwLock<MessageHistory>> {
        self.message_history.clone()
    }

    pub fn add_peer(&mut self, peer: Multiaddr) {
        self.known_peers.push(peer);
    }

    // pub async fn send_to(&mut self, destination: Ipv4Addr, message: &[u8]) -> Result<()> {
    //     let message = Message::new(self.ip_address, destination, message);

    //     self.to_network.send(message).await?;
    //     self.stats.lock().unwrap().sent_count += 1;

    //     Ok(())
    // }

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

        let _ = network_event_tx
            .send(NetworkEvent::NodeRunning((
                self.peer_id,
                self.message_history_copy(),
            )))
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

        if !self.known_peers.is_empty() {}

        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    break;
                },
                _ = interval.tick() => {
                        let messages = self.message_history.read().unwrap();
                        debug!(target: "node", "current node messages: {:?}", messages);
                },
                message = self.from_network.recv() => {
                    if message.is_none() {
                        continue;
                    }

                    let command = message.unwrap();
                    self.handle_node_command(command);

                    self.stats.lock().unwrap().recvd_count+= 1;
                },

                Some(event) = self.swarm.next() => {
                    trace!("node swarm event {:?}", event);
                    match event {
                        SwarmEvent::Dialing { peer_id, ..} => {
                            debug!(target: "node", "dialing peer {:?}", peer_id);
                        },
                        SwarmEvent::NewListenAddr { listener_id, address, .. } => {
                            let local_p2p_addr = address.clone()
                                    .with_p2p(*self.swarm.local_peer_id()).unwrap();
                            debug!(target: "node", "listening on p2p address {:?}", local_p2p_addr);

                            let mut message_history = self.message_history.write().unwrap();

                            message_history.add_swarm_event(SwarmEventInfo::NewListenAddr { listener_id, address }, Instant::now().duration_since(start).as_secs_f32());
                        },
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            debug!(target: "node", "new peer {} from {:?}", peer_id, endpoint);

                            let peer_addr = match endpoint.clone() {
                                ConnectedPoint::Dialer { address, ..} => address,
                                ConnectedPoint::Listener { send_back_addr, ..} => send_back_addr,
                            };
                            self.swarm.behaviour_mut().kad.add_address(&peer_id, peer_addr);

                            let mut message_history = self.message_history.write().unwrap();

                            message_history.add_swarm_event(SwarmEventInfo::ConnectionEstablished { peer_id, endpoint }, Instant::now().duration_since(start).as_secs_f32());

                            if !self.bootstrapped {
                                debug!(target: "kademlia_events", "attempting kademlia bootstrapping");
                                if let Ok(qid) = self.swarm.behaviour_mut().kad.bootstrap() {
                                    debug!(target: "kademlia_events", "kademlia bootstrap started");
                                    self.kad_queries.bootsrap_id = Some(qid);
                                } else {
                                    warn!(target: "kademlia_events", "initial kademlia bootstrap failed");
                                }
                            }
                        },
                        SwarmEvent::ConnectionClosed { peer_id, endpoint, cause, .. } => {
                            debug!(target: "node", "connection closed peer {} ({:?}) cause: {:?}", peer_id, endpoint, cause);

                            let mut message_history = self.message_history.write().unwrap();

                            message_history.add_swarm_event(SwarmEventInfo::ConnectionClosed { peer_id, endpoint }, Instant::now().duration_since(start).as_secs_f32());

                        },
                        SwarmEvent::ListenerClosed { listener_id, addresses, .. } => {
                            debug!(target: "node", "listener now closed");
                            let mut message_history = self.message_history.write().unwrap();

                            message_history.add_swarm_event(SwarmEventInfo::ListenerClosed { listener_id, addresses }, Instant::now().duration_since(start).as_secs_f32());
                        }
                        SwarmEvent::IncomingConnectionError { peer_id, error, ..} => {
                                error!(target: "node", "incoming connection failed, peer {:?}: {error}", peer_id);
                        },
                        SwarmEvent::Behaviour(NodeNetworkEvent::Identify(event)) => {
                            {
                                debug!(target: "node", "new identify event been added");
                                let mut message_history = self.message_history.write().unwrap();

                                message_history.add_identify_event(format!("{:?}", event), Instant::now().duration_since(start).as_secs_f32());
                            }

                            identify_handler::handle_event(self, event)?;

                        },
                        SwarmEvent::Behaviour(NodeNetworkEvent::Kademlia(event)) => {
                            {
                                debug!(target: "node", "new kademlia event been added");
                                let mut message_history = self.message_history.write().unwrap();

                                message_history.add_kademlia_event(format!("{:?}", event), Instant::now().duration_since(start).as_secs_f32());
                            }

                            kad_handler::handle_event(self, event)?;
                        },
                        other => {
                            debug!("new event: {:?}", other);
                        }
                    }
                },

            }
        }

        info!(target: "node", "node {} now shutting down", self.peer_id);

        let _ = network_event_tx
            .send(NetworkEvent::NodeStopped(self.peer_id))
            .await;

        Ok(NodeResult::Success)
    }

    fn handle_node_command(&mut self, command: NodeCommand) {
        match command {
            NodeCommand::AddPeer(peer) => {
                if self.swarm.dial(peer.clone()).is_ok() {
                    debug!(target: "node", "successully dialed peer {peer}");
                    self.known_peers.push(peer);
                } else {
                    warn!(target: "node", "failed to dial peer {peer}");
                }
            }
        }
    }

    pub fn identify_messages(&self) -> Vec<String> {
        let messages = self.message_history.read().unwrap();

        messages.identify_messages()
    }

    pub fn kad_messages(&self) -> Vec<String> {
        let messages = self.message_history.read().unwrap();

        messages.kad_messages()
    }

    pub fn swarm_messages(&self) -> Vec<String> {
        let messages = self.message_history.read().unwrap();

        messages.swarm_messages()
    }

    pub fn all_messages(&self) -> Vec<String> {
        let messages = self.message_history.read().unwrap();

        messages.all_messages()
    }

    pub fn get_stats(&self) -> NodeStats {
        *self.stats.lock().unwrap()
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node: {}", self.peer_id)
    }
}
