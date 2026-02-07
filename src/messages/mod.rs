use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::{Arc, RwLock};

use libp2p::Multiaddr;
use libp2p::PeerId;

use crate::node::NodeStats;
use crate::node::history::MessageHistory;

#[derive(Debug)]
pub enum NetworkEvent {
    NodeRunning(
        (
            PeerId,
            Arc<RwLock<(MessageHistory, NodeStats)>>,
            Arc<RwLock<HashSet<PeerId>>>,
        ),
    ),
    NodeStopped(PeerId),
    NodesConnected {
        peer_one: PeerId,
        peer_two: PeerId,
    },
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartNode(PeerId),
    StopNode(PeerId),
    ConnectNodes((PeerId, PeerId)),
}

#[derive(Debug)]
pub enum NodeCommand {
    AddPeer(Multiaddr),
}
