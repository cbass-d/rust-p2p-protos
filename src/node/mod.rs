mod base;
mod behaviour;
pub mod configured;
pub mod history;
mod identify_handler;
pub mod info;
mod kad_handler;
pub mod running;

use core::fmt;

use libp2p::StreamProtocol;

const IPFS_KAD_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");
const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/id/1.0.0");
const NODE_NETWORK_AGENT: &str = "node-network/0.1";

/// The result after a node has stopped running
#[derive(Debug, Clone)]
pub(crate) enum NodeResult {
    Success,
    Error(String),
}

/// The number of messages/swarm events the node has sent and received
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NodeStats {
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
