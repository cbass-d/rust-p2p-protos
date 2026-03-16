use std::sync::Arc;

use parking_lot::RwLock;

use crate::node::{
    NodeStats,
    history::{MessageHistory, SwarmEventInfo},
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

    pub fn add_swarm_event(&mut self, event: SwarmEventInfo) {}
}

