use crate::node::behaviour::NodeNetworkEvent;
use libp2p::{
    Multiaddr, PeerId,
    core::{ConnectedPoint, transport::ListenerId},
};
use tracing::{debug, info};

#[derive(Debug)]
pub enum SwarmEventInfo {
    ConnectionEstablished {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    ConnectionClosed {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    IncomingConnection {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    ListenerClosed {
        listener_id: ListenerId,
        addresses: Vec<Multiaddr>,
    },
    NewListenAddr {
        listener_id: ListenerId,
        address: Multiaddr,
    },
    Dialing {
        peer_id: Option<PeerId>,
    },
}

#[derive(Default, Debug)]
pub struct MessageHistory {
    pub identify: Vec<(String, f32)>,
    pub kademlia: Vec<(String, f32)>,
    pub swarm: Vec<(SwarmEventInfo, f32)>,
}

impl MessageHistory {
    pub fn add_identify_event(&mut self, event: String, since_start: f32) {
        self.identify.push((event, since_start));
    }

    pub fn add_kademlia_event(&mut self, event: String, since_start: f32) {
        self.kademlia.push((event, since_start));
    }

    pub fn identify_messages(&self) -> Vec<String> {
        self.identify
            .iter()
            .map(|(e, t)| format!("{}s : {:?}", t, e))
            .collect()
    }

    pub fn kad_messages(&self) -> Vec<String> {
        self.kademlia
            .iter()
            .map(|(e, t)| format!("{}s : {:?}", t, e))
            .collect()
    }

    pub fn swarm_messages(&self) -> Vec<String> {
        self.swarm
            .iter()
            .map(|(e, t)| format!("{}s : {:?}", t, e))
            .collect()
    }

    pub fn all_messages(&self) -> Vec<String> {
        let mut messages = vec![];
        messages.append(&mut self.identify_messages());
        messages.append(&mut self.kad_messages());
        messages.append(&mut self.swarm_messages());

        messages
    }

    pub fn display_identify_messages(&self) {
        for message in &self.identify {
            let (event, time) = message;
            debug!(target: "history", "{}s : {}", time, event);
            println!("{}s : {:?}", time, event);
        }
    }

    pub fn display_kademlia_messages(&self) {
        for message in &self.kademlia {
            let (event, time) = message;
            debug!(target: "history", "{}s : {}", time, event);
            println!("{}s : {:?}", time, event);
        }
    }

    pub fn display_swarm_messages(&self) {
        for message in &self.swarm {
            let (event, time) = message;
            debug!(target: "history", "{}s : {:?}", time, event);
            println!("{}s : {:?}", time, event);
        }
    }

    pub fn add_swarm_event(&mut self, event: SwarmEventInfo, since_start: f32) {
        self.swarm.push((event, since_start));
    }
}
