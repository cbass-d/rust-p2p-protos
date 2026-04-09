use std::time::Instant;

use color_eyre::eyre::Result;
use libp2p::identify::{self, Event};
use tracing::{debug, info, instrument, warn};

use crate::node::{NODE_NETWORK_AGENT, history::IdentifyEventInfo, running::RunningNode};

/// Handle an incoming identify event
#[instrument(skip_all, name = "identify_event")]
pub(crate) fn handle_event(node: &mut RunningNode, event: identify::Event) -> Result<()> {
    match event {
        Event::Received { peer_id, info, .. } => {
            debug!(target: "simulation::node::identify_events", %peer_id, ?info, "identify received");

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
                        info!(target: "simulation::node::swarm_events", %addr, %peer_id, "dialed peer from identify recv");
                        node.connection_tracker.add_active_peer(peer_id);
                        break;
                    } else {
                        warn!(target: "simulation::node::swarm_events", %addr, %peer_id, "failed to dial peer from identify recv");
                    }
                }
            }
        }
        Event::Sent { peer_id, .. } => {
            node.logger.add_identify_event(
                IdentifyEventInfo::Sent { peer_id },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::identify_events", %peer_id, "identify sent");
        }
        Event::Pushed { peer_id, info, .. } => {
            node.logger.add_identify_event(
                IdentifyEventInfo::Pushed {
                    peer_id,
                    info: info.clone(),
                },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::identify_events", %peer_id, ?info, "identify pushed");
        }
        Event::Error { peer_id, error, .. } => {
            if let libp2p::swarm::StreamUpgradeError::Timeout = error {
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

                debug!(target: "simulation::node::identify_events", %peer_id, %error, "identify error");
            }
        }
    }

    Ok(())
}
