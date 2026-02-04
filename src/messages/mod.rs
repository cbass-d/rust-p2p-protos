use std::sync::{Arc, RwLock};

use libp2p::PeerId;

use crate::node::history::MessageHistory;

#[derive(Debug)]
pub enum NetworkEvent {
    NodeRunning((PeerId, Arc<RwLock<MessageHistory>>)),
    NodeStopped(PeerId),
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartNode(PeerId),
    StopNode(PeerId),
}
