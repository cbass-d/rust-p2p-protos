use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::node::NodeStats;
use crate::node::history::MessageHistory;
use crate::node::info::IdentifyInfo;

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
    IdentifyInfo {
        info: IdentifyInfo,
    },
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartNode,
    StopNode { peer_id: PeerId },
    ConnectNodes { peer_one: PeerId, peer_two: PeerId },
    DisconectNodes { peer_one: PeerId, peer_two: PeerId },
    GetIdentifyInfo { peer_id: PeerId },
}

#[derive(Debug)]
pub enum NodeCommand {
    ConnectTo { peer: Multiaddr },
    DisconnectFrom { peer: PeerId },
    GetIdentifyInfo,
    Stop,
}

#[derive(Debug)]
pub enum NodeResponse {
    IdentifyInfo { info: IdentifyInfo },
}
