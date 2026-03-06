use libp2p::{Multiaddr, PeerId, Swarm};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{
    messages::{NetworkEvent, NodeCommand, NodeResponse},
    node::{
        behaviour::NodeBehaviour,
        info::{IdentifyInfo, KademliaInfo},
    },
};

/// The base structure for a node
pub(crate) struct NodeBase {
    /// PeerId of the node in the libp2p swarm network
    pub(crate) peer_id: PeerId,

    /// The custom libp2p swarm behaviour
    /// - identify
    /// - kademlia
    pub(crate) swarm: Swarm<NodeBehaviour>,

    /// Struct to hold local identify info (info that is pushed to other peers)
    pub(crate) identify_info: IdentifyInfo,

    /// Struct to hold local kademlia info
    pub(crate) kad_info: KademliaInfo,

    /// libp2p swarm listen address
    pub(crate) listen_address: Multiaddr,

    /// mpsc channel for receiving commands to perform from the network
    pub(crate) from_network: mpsc::Receiver<(NodeCommand, oneshot::Sender<NodeResponse>)>,

    /// CancellationToken that shared network
    pub(crate) cancellation_token: CancellationToken,

    /// mpsc sender for Network Events
    pub(crate) network_event_tx: mpsc::Sender<NetworkEvent>,
}
