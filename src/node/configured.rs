use parking_lot::RwLock;
use std::{collections::HashSet, sync::Arc, time::Instant};

use libp2p::{
    Multiaddr, Swarm, Transport,
    core::transport::{MemoryTransport, upgrade},
    identify::{Behaviour as Identify, Config as IdentifyConfig},
    identity,
    kad::{Behaviour as Kademlia, store::MemoryStore},
    multiaddr::Protocol,
    noise,
    swarm::{self},
    yamux,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{
    error::AppError,
    messages::{NetworkEvent, NodeCommand, NodeResponse},
    node::{
        IPFS_PROTO_NAME, NODE_NETWORK_AGENT, NodeError, NodeStats,
        base::NodeBase,
        behaviour::NodeBehaviour,
        history::MessageHistory,
        info::{IdentifyInfo, KademliaInfo},
        kad_handler::KadQueries,
        running::RunningNode,
    },
};

/// A node with libp2p swarm configured
pub(crate) struct ConfiguredNode {
    /// The core/base of the node
    pub(crate) base: NodeBase,

    /// The peers the node knows at build time
    known_peers: Vec<Multiaddr>,
}

impl ConfiguredNode {
    pub fn new(
        cancellation_token: CancellationToken,
        network_event_tx: mpsc::Sender<NetworkEvent>,
    ) -> Result<
        (
            Self,
            mpsc::Sender<(NodeCommand, oneshot::Sender<NodeResponse>)>,
        ),
        NodeError,
    > {
        // Build the mpsc channel where the channel will be recieving messages from
        let (tx, rx) = mpsc::channel(100);

        let node_keys = identity::Keypair::generate_ed25519();
        let peer_id = node_keys.public().to_peer_id();
        let node_transport = MemoryTransport::default()
            .upgrade(upgrade::Version::V1)
            .authenticate(
                noise::Config::new(&node_keys).map_err(|e| NodeError::Config(e.to_string()))?,
            )
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

        let identify_info = IdentifyInfo::new(
            node_keys.public(),
            IPFS_PROTO_NAME.to_string(),
            NODE_NETWORK_AGENT.to_string(),
            listen_address.clone(),
        );

        let kad_info = KademliaInfo::new(swarm.behaviour().kad.mode(), false);

        let base = NodeBase {
            peer_id,
            swarm,
            identify_info,
            kad_info,
            listen_address,
            from_network: rx,
            network_event_tx,
            cancellation_token,
        };

        let node = ConfiguredNode {
            base,
            known_peers: vec![],
        };

        Ok((node, tx))
    }

    /// Transitions from ConfiguredNode to a RunningNode instance consuming itself
    pub fn start(self) -> RunningNode {
        RunningNode {
            base: self.base,
            quit: false,
            killed: false,
            start: Instant::now(),
            logs: Arc::new(RwLock::new((
                MessageHistory::default(),
                NodeStats::default(),
            ))),
            kad_queries: KadQueries::default(),
            bootstrapped: false,
            current_peers: Arc::new(RwLock::new(HashSet::new())),
            known_peers: self.known_peers,
        }
    }
}
