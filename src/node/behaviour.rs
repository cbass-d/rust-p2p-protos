use std::fmt::Display;

use libp2p::{
    identify,
    kad::{self, store::MemoryStore},
    swarm::NetworkBehaviour,
};

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "NodeNetworkEvent")]
pub struct NodeBehaviour {
    pub kad: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
}

#[derive(Debug)]
pub enum NodeNetworkEvent {
    Identify(identify::Event),
    Kademlia(kad::Event),
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

impl Display for NodeNetworkEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeNetworkEvent::Identify(event) => {
                write!(f, "{:?}", event)
            }
            NodeNetworkEvent::Kademlia(event) => {
                write!(f, "{:?}", event)
            }
        }
    }
}
