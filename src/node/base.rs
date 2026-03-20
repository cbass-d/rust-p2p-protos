use color_eyre::{Result, eyre::Context};
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm,
    core::transport::ListenerId,
    kad::{KBucketKey, QueryId, RecordKey, RoutingUpdate},
    swarm::SwarmEvent,
};

use crate::node::{
    NodeError,
    behaviour::{NodeBehaviour, NodeNetworkEvent},
    info::{IdentifyInfo, KBucketInfo, KademliaInfo},
};

/// The base structure for a node
pub(crate) struct NodeBase {
    /// `PeerId` of the node in the libp2p swarm network
    pub(crate) peer_id: PeerId,

    /// The custom libp2p swarm behaviour
    /// - identify
    /// - kademlia
    swarm: Swarm<NodeBehaviour>,

    /// Struct to hold local identify info (info that is pushed to other peers)
    pub(crate) identify_info: IdentifyInfo,

    /// Struct to hold local kademlia info
    pub(crate) kad_info: KademliaInfo,

    /// libp2p swarm listen address
    pub(crate) listen_address: Multiaddr,
}

impl NodeBase {
    pub fn new(
        peer_id: PeerId,
        swarm: Swarm<NodeBehaviour>,
        identify_info: IdentifyInfo,
        kad_info: KademliaInfo,
        listen_address: Multiaddr,
    ) -> Self {
        Self {
            peer_id,
            swarm,
            identify_info,
            kad_info,
            listen_address,
        }
    }

    /// Dials a peer using the lip2p swarm
    pub(crate) fn dial(&mut self, addr: Multiaddr) -> Result<()> {
        self.swarm.dial(addr).wrap_err("swarm dial failed")?;
        Ok(())
    }

    /// Disconnect from a swarm peer
    pub(crate) fn disconnect_peer(&mut self, peer_id: PeerId) -> Result<(), ()> {
        self.swarm.disconnect_peer_id(peer_id)
    }

    /// Listens on the swarm address
    pub(crate) fn listen(&mut self) -> Result<ListenerId, NodeError> {
        let listener_id = self.swarm.listen_on(self.listen_address.clone())?;

        Ok(listener_id)
    }

    pub(crate) async fn next_event(&mut self) -> Option<SwarmEvent<NodeNetworkEvent>> {
        self.swarm.next().await
    }

    pub(crate) fn kad_remove_peer(&mut self, peer_id: &PeerId) -> bool {
        self.swarm
            .behaviour_mut()
            .kad
            .remove_peer(peer_id)
            .is_some()
    }

    pub(crate) fn kad_bootstrap(&mut self) -> Result<QueryId, NodeError> {
        self.swarm
            .behaviour_mut()
            .kad
            .bootstrap()
            .map_err(NodeError::KadBootstrapFailed)
    }

    pub(crate) fn kad_get_providers(&mut self, key: RecordKey) -> QueryId {
        self.swarm.behaviour_mut().kad.get_providers(key)
    }

    pub(crate) fn kad_get_closest_peers(&mut self, peer_id: PeerId) -> QueryId {
        self.swarm.behaviour_mut().kad.get_closest_peers(peer_id)
    }

    pub(crate) fn kad_get_closest_local_peers(&mut self, peer_id: PeerId) -> Vec<PeerId> {
        let key = KBucketKey::from(peer_id);
        self.swarm
            .behaviour_mut()
            .kad
            .get_closest_local_peers(&key)
            .map(libp2p::kad::KBucketKey::into_preimage)
            .collect()
    }

    pub(crate) fn kad_kbuckets(&mut self) -> Vec<KBucketInfo> {
        self.swarm
            .behaviour_mut()
            .kad
            .kbuckets()
            .map(|kb| KBucketInfo {
                range: (kb.range().0.0, kb.range().1.0),
                num_entries: kb.num_entries(),
            })
            .collect()
    }

    pub(crate) fn kad_add_address(
        &mut self,
        peer_id: &PeerId,
        peer_addr: Multiaddr,
    ) -> RoutingUpdate {
        self.swarm
            .behaviour_mut()
            .kad
            .add_address(peer_id, peer_addr)
    }
}
