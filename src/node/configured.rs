use std::{net::Ipv4Addr, time::Instant};

use libp2p::{
    Multiaddr, Swarm, Transport,
    core::{
        transport::MemoryTransport,
        upgrade::{self, Version},
    },
    identify::{Behaviour as Identify, Config as IdentifyConfig},
    identity,
    kad::{Behaviour as Kademlia, store::MemoryStore},
    mdns::Config as MdnsConfig,
    mdns::tokio::Behaviour as Mdns,
    multiaddr::Protocol,
    noise,
    swarm::{self, behaviour::toggle::Toggle},
    tcp, yamux,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    messages::{CommandChannel, NetworkEvent},
    network::TransportMode,
    node::{
        IPFS_PROTO_NAME, NODE_NETWORK_AGENT, NodeError,
        base::NodeBase,
        behaviour::NodeBehaviour,
        connection_tracker::ConnectionTracker,
        info::{IdentifyInfo, KademliaInfo},
        kad_handler::KadQueries,
        logger::NodeLogger,
        running::RunningNode,
        state::State,
    },
};

/// A node with libp2p swarm configured
pub(crate) struct ConfiguredNode {
    /// The core/base of the node
    pub(crate) base: NodeBase,

    /// The peers the node knows at build time
    known_peers: Vec<Multiaddr>,

    /// `CancellationToken` that shared network
    cancellation_token: CancellationToken,

    /// mpsc sender for Network Events
    network_event_tx: mpsc::Sender<NetworkEvent>,

    /// mpsc channel for receiving commands to perform from the network
    from_network: mpsc::Receiver<CommandChannel>,
}

impl ConfiguredNode {
    pub fn new(
        cancellation_token: CancellationToken,
        network_event_tx: mpsc::Sender<NetworkEvent>,
        transport: TransportMode,
        bind_address: Option<Ipv4Addr>,
    ) -> Result<(Self, mpsc::Sender<CommandChannel>), NodeError> {
        // Build the mpsc channel where the node will be receiving commands from
        let (tx, rx) = mpsc::channel(100);

        let node_keys = identity::Keypair::generate_ed25519();
        let peer_id = node_keys.public().to_peer_id();
        let node_transport = match transport {
            TransportMode::Memory => MemoryTransport::default()
                .upgrade(upgrade::Version::V1)
                .authenticate(
                    noise::Config::new(&node_keys).map_err(|e| NodeError::Config(e.to_string()))?,
                )
                .multiplex(yamux::Config::default())
                .boxed(),
            TransportMode::Tcp => tcp::tokio::Transport::new(tcp::Config::default())
                .upgrade(Version::V1)
                .authenticate(
                    noise::Config::new(&node_keys).map_err(|e| NodeError::Config(e.to_string()))?,
                )
                .multiplex(yamux::Config::default())
                .boxed(),
        };

        let listen_address = match transport {
            TransportMode::Memory => Multiaddr::from(Protocol::Memory(rand::random::<u64>())),
            TransportMode::Tcp => {
                let bind_address = bind_address.unwrap();
                format!("/ip4/{bind_address}/tcp/0")
                    .parse()
                    .expect("invalid tcp listen address")
            }
        };

        let store = MemoryStore::new(peer_id);
        let kad = Kademlia::new(peer_id, store);
        let mdns = match transport {
            TransportMode::Tcp => Toggle::from(Some(
                Mdns::new(MdnsConfig::default(), peer_id)
                    .map_err(|e| NodeError::Config(e.to_string()))?,
            )),
            TransportMode::Memory => Toggle::from(None),
        };

        let identify = {
            let cfg = IdentifyConfig::new(IPFS_PROTO_NAME.to_string(), node_keys.public())
                .with_agent_version(NODE_NETWORK_AGENT.to_string());

            Identify::new(cfg)
        };
        let behaviour = NodeBehaviour {
            kad,
            identify,
            mdns,
        };

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

        let base = NodeBase::new(
            peer_id,
            swarm,
            identify_info,
            kad_info,
            listen_address,
            bind_address,
        );

        let node = ConfiguredNode {
            base,
            known_peers: vec![],
            cancellation_token,
            network_event_tx,
            from_network: rx,
        };

        Ok((node, tx))
    }

    /// Transitions from `ConfiguredNode` to a `RunningNode` instance consuming itself
    pub fn start(self) -> RunningNode {
        let mut connection_tracker = ConnectionTracker::default();
        connection_tracker.set_known(self.known_peers);
        RunningNode {
            base: self.base,
            logger: NodeLogger::default(),
            kad_queries: KadQueries::default(),
            connection_tracker,
            state: State::new(Instant::now(), self.cancellation_token),
            from_network: self.from_network,
            network_event_tx: self.network_event_tx,
        }
    }
}
