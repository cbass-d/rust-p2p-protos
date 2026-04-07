use libp2p::PeerId;

/// Represents an node that is running outside of the scope
/// of the actively running program (discovered by mdns)
pub(crate) struct ExternalNode {
    /// `PeerId` of the node in the libp2p swarm network
    pub(crate) peer_id: PeerId,
}

impl ExternalNode {
    pub fn new(peer_id: PeerId) -> Self {
        ExternalNode { peer_id }
    }
}
