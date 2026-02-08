use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::node::NodeStats;
use crate::node::history::MessageHistory;

#[derive(Debug)]
pub enum NetworkEvent {
    NodeRunning {
        peer_id: PeerId,
        message_history: Arc<RwLock<(MessageHistory, NodeStats)>>,
        node_connections: Arc<RwLock<HashSet<PeerId>>>,
    },
    NodeStopped {
        peer_id: PeerId,
    },
    NodesConnected {
        peer_one: PeerId,
        peer_two: PeerId,
    },
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartNode { peer_id: PeerId },
    StopNode { peer_id: PeerId },
    ConnectNodes { peer_one: PeerId, peer_two: PeerId },
    DisconectNodes { peer_one: PeerId, peer_two: PeerId },
}

#[derive(Debug)]
pub enum NodeCommand {
    ConnectTo { peer: Multiaddr },
    DisconnectFrom { peer: PeerId },
    Stop,
}
