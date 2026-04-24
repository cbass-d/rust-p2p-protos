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
    bus::EventBus,
    messages::{CommandChannel, NetworkEvent},
    network::TransportMode,
    node::{
        IPFS_PROTO_NAME, NODE_NETWORK_AGENT, NodeError,
        base::NodeBase,
        behaviour::NodeBehaviour,
        connection_tracker::ConnectionTracker,
        handlers::{
            CoreSwarmHandler, IdentifyEventHandler, KadEventHandler, MdnsEventHandler,
            SwarmEventHandler,
        },
        info::{IdentifyInfo, KademliaInfo},
        kad_handler::KadQueries,
        logger::NodeLogger,
        running::RunningNode,
        state::State,
    },
};

const NODE_COMMAND_CHANNEL_SIZE: usize = 100;

/// A node with libp2p swarm configured
pub(crate) struct ConfiguredNode {
    /// The core/base of the node
    pub(crate) base: NodeBase,

    /// The peers the node knows at build time
    known_peers: Vec<Multiaddr>,

    /// `CancellationToken` that shared network
    cancellation_token: CancellationToken,

    /// Broadcast bus for Network Events (cloned from the NodeNetwork's bus).
    network_event_tx: EventBus<NetworkEvent>,

    /// mpsc channel for receiving commands to perform from the network
    from_network: mpsc::Receiver<CommandChannel>,
}

impl ConfiguredNode {
    /// Build a fully-wired node and the sender used to issue commands to it.
    pub fn new(
        cancellation_token: CancellationToken,
        network_event_tx: EventBus<NetworkEvent>,
        transport: TransportMode,
        bind_address: Option<Ipv4Addr>,
    ) -> Result<(Self, mpsc::Sender<CommandChannel>), NodeError> {
        // Build the mpsc channel where the node will be receiving commands from
        let (tx, rx) = mpsc::channel(NODE_COMMAND_CHANNEL_SIZE);

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
                let bind_address = bind_address.ok_or(NodeError::Config(
                    "bind address required for tcp mode".into(),
                ))?;

                let listen_addr = format!("/ip4/{bind_address}/tcp/0")
                    .parse()
                    .map_err(|e| NodeError::Config(format!("invalid listen address: {e}")))?;

                listen_addr
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
            transport,
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

        // Canonical handler order: the core swarm-lifecycle handler runs first
        // (fast-path for Dialing / Connection* / Listener* events); the three
        // behaviour handlers are disjoint, so their relative order is cosmetic.
        let handlers: Vec<Box<dyn SwarmEventHandler>> = vec![
            Box::new(CoreSwarmHandler),
            Box::new(IdentifyEventHandler),
            Box::new(KadEventHandler),
            Box::new(MdnsEventHandler),
        ];

        RunningNode {
            base: self.base,
            logger: NodeLogger::default(),
            kad_queries: KadQueries::default(),
            connection_tracker,
            state: State::new(Instant::now(), self.cancellation_token),
            from_network: self.from_network,
            network_event_tx: self.network_event_tx,
            handlers,
        }
    }
}
