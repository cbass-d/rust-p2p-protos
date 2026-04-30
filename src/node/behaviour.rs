use std::fmt::Display;

use libp2p::{
    identify,
    kad::{self, store::MemoryStore},
    mdns,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
};

/// Node behaviour for our peers. Currently has identify and kademlia. (more in the future)
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "NodeNetworkEvent")]
pub struct NodeBehaviour {
    pub kad: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
    pub mdns: Toggle<mdns::tokio::Behaviour>,
}

/// libp2p swarm events for our node behaviour
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum NodeNetworkEvent {
    Identify(identify::Event),
    Kademlia(kad::Event),
    Mdns(mdns::Event),
}

impl From<identify::Event> for NodeNetworkEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<kad::Event> for NodeNetworkEvent {
    fn from(event: kad::Event) -> Self {
        Self::Kademlia(event)
    }
}

impl From<mdns::Event> for NodeNetworkEvent {
    fn from(event: mdns::Event) -> Self {
        Self::Mdns(event)
    }
}

impl Display for NodeNetworkEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeNetworkEvent::Identify(event) => {
                write!(f, "{event:?}")
            }
            NodeNetworkEvent::Kademlia(event) => {
                write!(f, "{event:?}")
            }
            NodeNetworkEvent::Mdns(event) => {
                write!(f, "{event:?}")
            }
        }
    }
}
