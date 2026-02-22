use color_eyre::eyre::Result;
use libp2p::identify::{self, Event};
use tracing::{debug, info, warn};

use crate::node::{NODE_NETWORK_AGENT, Node};

/// Handle an incoming identify event
pub fn handle_event(node: &mut Node, event: identify::Event) -> Result<()> {
    match event {
        Event::Received { peer_id, info, .. } => {
            debug!(target: "identify_events", "identify recv event {:?}", info);

            let current_peers = {
                let peers = node.current_peers.read().unwrap();

                peers
            };

            if info.agent_version == NODE_NETWORK_AGENT && !current_peers.contains(&peer_id) {
                for addr in info.listen_addrs.into_iter() {
                    if node.swarm.dial(addr.clone()).is_ok() {
                        info!(target: "swarm_events", "dialed peer {addr} from identify recv");

                        let mut peers = node.current_peers.write().unwrap();
                        peers.insert(peer_id);
                    } else {
                        warn!(target: "swarm_events", "failed to dial peer {addr} from identify recv");
                    }
                }
            }
        }
        Event::Sent { peer_id, .. } => {
            debug!(target: "identify_events", "identify sent event to {peer_id}");
        }
        Event::Pushed { peer_id, info, .. } => {
            debug!(target: "identify_events", "identify pushed event to {peer_id} {:?}", info);
        }
        Event::Error { peer_id, error, .. } => match error {
            libp2p::swarm::StreamUpgradeError::Timeout => {
                node.swarm.behaviour_mut().kad.remove_peer(&peer_id);
            }
            _ => {
                debug!(target: "identify_events", "identify error: {error}");
            }
        },
    }

    Ok(())
}
