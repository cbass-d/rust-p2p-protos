use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::oneshot;

use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::node::NodeStats;
use crate::node::history::MessageHistory;
use crate::node::info::{IdentifyInfo, KademliaInfo};

pub(crate) type CommandChannel = (NodeCommand, oneshot::Sender<NodeResponse>);

/// Event emitted by the Node Network
#[derive(Clone, Debug)]
pub(crate) enum NetworkEvent {
    /// A new node has started running
    NodeRunning {
        peer_id: PeerId,
        message_history: Arc<RwLock<MessageHistory>>,
        stats: Arc<RwLock<NodeStats>>,
        node_connections: Arc<RwLock<HashSet<PeerId>>>,
    },

    /// A node has bound a real listen address (emitted per `NewListenAddr`)
    NodeListening { peer_id: PeerId, address: Multiaddr },

    /// A node in the network has stopped
    NodeStopped { peer_id: PeerId },

    /// Two nodes have established a connection between each other
    NodesConnected { peer_one: PeerId, peer_two: PeerId },

    /// A node has been discovered using MDNS
    NodeDiscovered { peer_id: PeerId },

    /// A MDNS record has expired so node has to be removed
    NodeExpired { peer_id: PeerId },

    /// Two nodes have closed the connection between each other
    NodesDisconnected { peer_one: PeerId, peer_two: PeerId },

    /// The local identify info for a node
    IdentifyInfo { info: IdentifyInfo },

    /// The kademlia info for a node
    KademliaInfo { info: KademliaInfo },

    /// Max number of nodes reached
    MaxNodes,
}

/// A network command sent from the TUI to the Node Network, grouped by protocol.
#[derive(Debug)]
pub(crate) enum NetworkCommand {
    Swarm(SwarmCommand),
    Identify(IdentifyCommand),
    Kademlia(KademliaCommand),
    Lifecycle(LifecycleCommand),
}

/// Commands that act on the libp2p swarm connection graph.
#[derive(Debug)]
pub(crate) enum SwarmCommand {
    /// Create a connection between two nodes, `peer_one` dials `peer_two`.
    Connect { peer_one: PeerId, peer_two: PeerId },

    /// Remove the connection between two nodes.
    Disconnect { peer_one: PeerId, peer_two: PeerId },
}

/// Commands that interact with the Identify protocol.
#[derive(Debug)]
pub(crate) enum IdentifyCommand {
    /// Get the local identify info for the requested node.
    GetInfo { peer_id: PeerId },
}

/// Commands that interact with the Kademlia protocol.
#[derive(Debug)]
pub(crate) enum KademliaCommand {
    /// Get the kademlia info for the requested node.
    GetInfo { peer_id: PeerId },

    /// Put a new KAD record on the selected peer.
    PutRecord {
        peer_id: PeerId,
        key: String,
        value: String,
    },
}

/// Commands that manage network-wide node lifecycle.
#[derive(Debug)]
pub(crate) enum LifecycleCommand {
    /// Start a new node in the network.
    Start,

    /// Stop a node from running.
    Stop { peer_id: PeerId },

    /// Add a new external node that was discovered outside the simulation.
    AddExternal { peer_id: PeerId, address: Multiaddr },
}

/// A command sent to a node from the node network
#[derive(Debug)]
pub(crate) enum NodeCommand {
    /// Establish a connection with the requested peer
    ConnectTo { peer: Multiaddr },

    /// Disconnect from the requested peer
    DisconnectFrom { peer: PeerId },

    /// Return the local identify info
    GetIdentifyInfo,

    /// Return the kademlia info
    GetKademliaInfo,

    /// Put a new KAD record on the node
    PutRecord { key: String, value: String },

    /// Stop the node from running
    Stop,
}

/// The response a node sends back after receiving a `NodeCommand`
#[derive(Debug, PartialEq)]
pub(crate) enum NodeResponse {
    /// Local identify info
    IdentifyInfo { info: IdentifyInfo },

    /// Kademlia info
    KademliaInfo { info: KademliaInfo },

    /// Disconnected from peer
    Disconnected { peer: PeerId },

    /// Connected to peer
    Dialed { addr: Multiaddr },

    /// Record placed
    RecordPlaced,

    /// Node stopped
    Stopped,

    /// Node command failed
    Failed,
}
