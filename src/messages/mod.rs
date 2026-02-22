use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::node::NodeStats;
use crate::node::history::MessageHistory;
use crate::node::info::{IdentifyInfo, KademliaInfo};

/// Event emitted by the Node Network
#[derive(Debug)]
pub enum NetworkEvent {
    /// A new node has started running
    NodeRunning {
        peer_id: PeerId,
        message_history: Arc<RwLock<(MessageHistory, NodeStats)>>,
        node_connections: Arc<RwLock<HashSet<PeerId>>>,
    },

    /// A node in the network has stopped
    NodeStopped { peer_id: PeerId },

    /// Two nodes have established a connection between each other
    NodesConnected { peer_one: PeerId, peer_two: PeerId },

    /// The local identify info for a node
    IdentifyInfo { info: IdentifyInfo },

    /// The kademlia info for a node
    KademliaInfo { info: KademliaInfo },
}

/// A network command sent from the TUI to the Node Network
#[derive(Debug)]
pub enum NetworkCommand {
    /// Start a new node in the network
    StartNode,

    /// Stop a node from running
    StopNode { peer_id: PeerId },

    /// Create a connection between two nodes
    ConnectNodes { peer_one: PeerId, peer_two: PeerId },

    /// Remove the connection between two nodes
    DisconectNodes { peer_one: PeerId, peer_two: PeerId },

    /// Get the local identify info for the requested node
    GetIdentifyInfo { peer_id: PeerId },

    /// Get the kademlia info for the requested node
    GetKademliaInfo { peer_id: PeerId },
}

/// A command sent to a node from the node network
#[derive(Debug)]
pub enum NodeCommand {
    /// Establish a connection with the requested peer
    ConnectTo { peer: Multiaddr },

    /// Disconnect from the requested peer
    DisconnectFrom { peer: PeerId },

    /// Return the local identify info
    GetIdentifyInfo,

    /// Return the kademlia info
    GetKademliaInfo,

    /// Stop the node from running
    Stop,
}

/// The response a node sends back after receiving a NodeCommand
#[derive(Debug)]
pub enum NodeResponse {
    /// Local identify info
    IdentifyInfo { info: IdentifyInfo },

    /// Kademlia info
    KademliaInfo { info: KademliaInfo },
}
