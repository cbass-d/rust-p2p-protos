use std::time::Instant;

use libp2p::{identify, swarm::SwarmEvent};
use tracing::debug;

use crate::node::{
    NODE_NETWORK_AGENT, NodeError, behaviour::NodeNetworkEvent, history::IdentifyEventInfo,
};

use super::{Effects, HandlerCtx, KadOp, LogKind, SwarmEventHandler};

/// Handles libp2p Identify protocol events.
pub(crate) struct IdentifyEventHandler;

impl SwarmEventHandler for IdentifyEventHandler {
    fn handle(
        &self,
        event: &SwarmEvent<NodeNetworkEvent>,
        ctx: &HandlerCtx<'_>,
        effects: &mut Effects,
    ) -> Result<bool, NodeError> {
        let SwarmEvent::Behaviour(NodeNetworkEvent::Identify(e)) = event else {
            return Ok(false);
        };

        let dur = Instant::now().duration_since(ctx.start_instant);

        match e {
            identify::Event::Received { peer_id, info, .. } => {
                debug!(target: "simulation::node::identify_events", %peer_id, ?info, "identify received");

                effects.log_entries.push((
                    LogKind::Identify(IdentifyEventInfo::Received {
                        peer_id: *peer_id,
                        info: info.clone(),
                    }),
                    dur,
                ));

                if info.agent_version == NODE_NETWORK_AGENT
                    && !ctx.connections.has_connection(peer_id)
                {
                    // Dial only the first listen address — matches previous
                    // behavior (the old handler broke on first success).
                    if let Some(addr) = info.listen_addrs.first() {
                        effects.dials.push(addr.clone());
                    }
                }
            }
            identify::Event::Sent { peer_id, .. } => {
                debug!(target: "simulation::node::identify_events", %peer_id, "identify sent");
                effects.log_entries.push((
                    LogKind::Identify(IdentifyEventInfo::Sent { peer_id: *peer_id }),
                    dur,
                ));
            }
            identify::Event::Pushed { peer_id, info, .. } => {
                debug!(target: "simulation::node::identify_events", %peer_id, ?info, "identify pushed");
                effects.log_entries.push((
                    LogKind::Identify(IdentifyEventInfo::Pushed {
                        peer_id: *peer_id,
                        info: info.clone(),
                    }),
                    dur,
                ));
            }
            identify::Event::Error { peer_id, error, .. } => {
                effects.log_entries.push((
                    LogKind::Identify(IdentifyEventInfo::Error { peer_id: *peer_id }),
                    dur,
                ));

                if let libp2p::swarm::StreamUpgradeError::Timeout = error {
                    effects.kad_ops.push(KadOp::RemovePeer { peer: *peer_id });
                } else {
                    debug!(target: "simulation::node::identify_events", %peer_id, %error, "identify error");
                }
            }
        }

        Ok(true)
    }
}
