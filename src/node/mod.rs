mod base;
mod behaviour;
pub mod configured;
mod connection_tracker;
pub(crate) mod handlers;
pub(crate) mod history;
pub(crate) mod info;
mod logger;
pub mod running;
mod state;

use core::fmt;
use libp2p::StreamProtocol;
use parking_lot::RwLock;
use std::{io, sync::Arc};
use thiserror::Error;

use crate::node::history::MessageHistory;

const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/id/1.0.0");
const NODE_NETWORK_AGENT: &str = "node-network/0.1";

/// Shared handle to a node's message history and stats — used by the TUI.
pub type NodeLogsHandle = (Arc<RwLock<MessageHistory>>, Arc<RwLock<NodeStats>>);

/// Errors that occur at the node level
#[derive(Error, Debug)]
pub enum NodeError {
    #[error("node configuration failed: {0}")]
    Config(String),
    #[error("swarm listen failed")]
    ListenFailed(#[from] libp2p::TransportError<io::Error>),
    #[error("swarm stream ended unexpectedly")]
    SwarmStreamEnded,
    #[error("kad bootstrap failed: {0}")]
    KadBootstrapFailed(#[from] libp2p::kad::NoKnownPeers),
    #[error("error while handling mdns event: {0}")]
    MdnsHandler(String),
    #[error("error while handling kad event: {0}")]
    KadHandler(String),
    #[error("error while handling identify event: {0}")]
    IdentifyHandler(String),
}

/// The result after a node has stopped running
#[derive(Debug)]
pub enum NodeResult {
    Success,
    Killed,
}

/// The number of swarm events the node has received
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeStats {
    recvd_count: u64,
}

impl NodeStats {
    pub(crate) fn total_recvd(&self) -> u64 {
        self.recvd_count
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn inc_recvd(&mut self) {
        self.recvd_count += 1;
    }
}

impl fmt::Display for NodeStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Packets received: {}", self.recvd_count)
    }
}

#[cfg(test)]
mod tests {
    use crate::node::NodeStats;

    #[test]
    fn test_new_stats() {
        let stats = NodeStats::default();

        assert_eq!(stats.total_recvd(), 0);
    }

    #[test]
    fn test_inc_recvd() {
        let mut stats = NodeStats::default();

        stats.inc_recvd();

        assert_eq!(stats.total_recvd(), 1);
    }
}
