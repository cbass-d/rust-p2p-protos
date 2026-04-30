use std::{collections::HashSet, sync::Arc};

use libp2p::{Multiaddr, PeerId};
use parking_lot::RwLock;
use tracing::debug;

/// Handles the tracking of active connections to other
/// peers in the libp2p swarm
#[derive(Default)]
pub(crate) struct ConnectionTracker {
    /// The peers the node has active connections to
    current_peers: Arc<RwLock<HashSet<PeerId>>>,

    /// The peers the node knows at build time
    known_peers: HashSet<Multiaddr>,
}

impl ConnectionTracker {
    pub(crate) fn set_known(&mut self, known_peers: Vec<Multiaddr>) {
        let known_peers = HashSet::from_iter(known_peers);

        self.known_peers = known_peers;
    }

    pub(crate) fn connection_count(&self) -> usize {
        let current_peers = self.current_peers.read();
        current_peers.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn known_peers_count(&self) -> usize {
        self.known_peers.len()
    }

    pub(crate) fn connections(&self) -> Arc<RwLock<HashSet<PeerId>>> {
        debug!(target: "simulation::node::connection_tracker", "creating copy of node connections");
        self.current_peers.clone()
    }

    pub(crate) fn has_connection(&self, peer_id: &PeerId) -> bool {
        let current_peers = self.current_peers.read();

        current_peers.contains(peer_id)
    }

    pub(crate) fn known(&self) -> &HashSet<Multiaddr> {
        &self.known_peers
    }

    pub(crate) fn add_active_peer(&self, peer_id: PeerId) {
        debug!(target: "simulation::node::connection_tracker", "adding new peer {} to node connections", peer_id);
        let mut current_peers = self.current_peers.write();
        current_peers.insert(peer_id);
    }

    pub(crate) fn remove_active_peer(&self, peer_id: &PeerId) {
        debug!(target: "simulation::node::connection_tracker", "removing peer {} from node connections", peer_id);
        let mut current_peers = self.current_peers.write();
        current_peers.remove(peer_id);
    }
}

#[cfg(test)]
mod tests {
    use libp2p::{Multiaddr, PeerId, multiaddr::Protocol};

    use crate::node::connection_tracker::ConnectionTracker;

    #[test]
    fn test_empty_default() {
        let connections = ConnectionTracker::default();

        assert_eq!(connections.connection_count(), 0);
        assert_eq!(connections.known_peers_count(), 0);
    }

    #[test]
    fn test_add_peer() {
        let connections = ConnectionTracker::default();
        let peer_id = PeerId::random();

        connections.add_active_peer(peer_id);
        assert!(connections.has_connection(&peer_id));
        assert_eq!(connections.connection_count(), 1);
    }

    #[test]
    fn test_add_and_remove_peer() {
        let connections = ConnectionTracker::default();
        let peer_id = PeerId::random();

        connections.add_active_peer(peer_id);
        assert!(connections.has_connection(&peer_id));
        assert_eq!(connections.connection_count(), 1);

        connections.remove_active_peer(&peer_id);
        assert_eq!(connections.known_peers_count(), 0);
    }

    #[test]
    fn test_add_duplicate_peer() {
        let connections = ConnectionTracker::default();
        let peer_id = PeerId::random();

        connections.add_active_peer(peer_id);
        connections.add_active_peer(peer_id);
        assert!(connections.has_connection(&peer_id));
        assert_eq!(connections.connection_count(), 1);
    }

    #[test]
    fn test_set_known_peers() {
        let mut connections = ConnectionTracker::default();
        let addrs: Vec<Multiaddr> = (0..3)
            .map(|i| Multiaddr::from(Protocol::Memory(i)))
            .collect();

        connections.set_known(addrs.clone());
        assert_eq!(connections.known_peers_count(), 3);

        connections.set_known(vec![addrs[0].clone()]);
        assert_eq!(connections.known_peers_count(), 1);
    }

    #[test]
    fn test_returns_shared_ref() {
        let connections = ConnectionTracker::default();
        let peer_id = PeerId::random();
        connections.add_active_peer(peer_id);

        let shared_ref = connections.connections();

        assert!(shared_ref.read().contains(&peer_id))
    }
}
