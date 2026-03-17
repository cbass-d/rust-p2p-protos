use std::{sync::Arc, time::Duration};

use parking_lot::RwLock;

use crate::node::{
    NodeStats,
    history::{IdentifyEventInfo, KadEventInfo, MessageHistory, SwarmEventInfo},
};

#[derive(Default)]
pub(crate) struct NodeLogger {
    message_history: Arc<RwLock<MessageHistory>>,
    node_stats: Arc<RwLock<NodeStats>>,
}

impl NodeLogger {
    pub fn history(&self) -> Arc<RwLock<MessageHistory>> {
        self.message_history.clone()
    }

    pub fn stats(&self) -> Arc<RwLock<NodeStats>> {
        self.node_stats.clone()
    }

    pub fn add_swarm_event(&mut self, event: SwarmEventInfo, duration: Duration) {
        let mut message_history = self.message_history.write();
        message_history.add_swarm_event(event, duration);
    }

    pub fn add_kademlia_event(&mut self, event: KadEventInfo, duration: Duration) {
        let mut message_history = self.message_history.write();
        message_history.add_kademlia_event(event, duration);
    }

    pub fn add_identify_event(&mut self, event: IdentifyEventInfo, duration: Duration) {
        let mut message_history = self.message_history.write();
        message_history.add_identify_event(event, duration);
    }

    pub fn increment_sent(&mut self) {
        let mut stats = self.node_stats.write();
        stats.recvd_count += 1
    }
}
