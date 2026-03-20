use std::time::Instant;

use color_eyre::eyre::Result;
use libp2p::identify::{self, Event};
use tracing::{debug, info, warn};

use crate::node::{NODE_NETWORK_AGENT, history::IdentifyEventInfo, running::RunningNode};

/// Handle an incoming identify event
pub(crate) fn handle_event(node: &mut RunningNode, event: identify::Event) -> Result<()> {
    match event {
        Event::Received { peer_id, info, .. } => {
            debug!(target: "simulation::node::identify_events", "identify recv event {:?}", info);

            node.logger.add_identify_event(
                IdentifyEventInfo::Received {
                    peer_id,
                    info: info.clone(),
                },
                Instant::now().duration_since(node.state.start()),
            );

            if info.agent_version == NODE_NETWORK_AGENT
                && !node.connection_tracker.has_connection(&peer_id)
            {
                for addr in info.listen_addrs {
                    if node.base.dial(addr.clone()).is_ok() {
                        info!(target: "simulation::node::swarm_events", "dialed peer {addr} from identify recv");

                        node.connection_tracker.add_active_peer(peer_id);
                    } else {
                        warn!(target: "simulation::node::swarm_events", "failed to dial peer {addr} from identify recv");
                    }
                }
            }
        }
        Event::Sent { peer_id, .. } => {
            node.logger.add_identify_event(
                IdentifyEventInfo::Sent { peer_id },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::identify_events", "identify sent event to {peer_id}");
        }
        Event::Pushed { peer_id, info, .. } => {
            node.logger.add_identify_event(
                IdentifyEventInfo::Pushed {
                    peer_id,
                    info: info.clone(),
                },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::identify_events", "identify pushed event to {peer_id} {:?}", info);
        }
        Event::Error { peer_id, error, .. } => if let libp2p::swarm::StreamUpgradeError::Timeout = error {
            node.logger.add_identify_event(
                IdentifyEventInfo::Error { peer_id },
                Instant::now().duration_since(node.state.start()),
            );

            node.base.kad_remove_peer(&peer_id);
        } else {
            node.logger.add_identify_event(
                IdentifyEventInfo::Error { peer_id },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::identify_events", "identify error: {error}");
        },
    }

    Ok(())
}
