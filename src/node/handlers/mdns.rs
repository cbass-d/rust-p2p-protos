use std::time::Instant;

use libp2p::{mdns, swarm::SwarmEvent};
use tracing::{debug, warn};

use crate::{
    messages::NetworkEvent,
    node::{NodeError, behaviour::NodeNetworkEvent, history::MdnsEventInfo},
    util,
};

use super::{Effects, HandlerCtx, LogKind, SwarmEventHandler};

/// Handles libp2p mDNS discovery events.
pub(crate) struct MdnsEventHandler;

impl SwarmEventHandler for MdnsEventHandler {
    fn handle(
        &self,
        event: &SwarmEvent<NodeNetworkEvent>,
        ctx: &HandlerCtx<'_>,
        effects: &mut Effects,
    ) -> Result<bool, NodeError> {
        let SwarmEvent::Behaviour(NodeNetworkEvent::Mdns(e)) = event else {
            return Ok(false);
        };

        let dur = Instant::now().duration_since(ctx.start_instant);

        match e {
            mdns::Event::Discovered(v) => {
                let (peer_id, address) = &v[0];

                debug!(target: "simulation::node::mdns_events", %peer_id, %address, "mdns discovered peer");

                effects.log_entries.push((
                    LogKind::Mdns(MdnsEventInfo::Discovered {
                        peer_id: *peer_id,
                        address: address.clone(),
                    }),
                    dur,
                ));

                let Some(ip) = util::extract_ip(address) else {
                    warn!(target: "simulation::node::mdns_events", "mdns record has no ip address");
                    return Ok(true);
                };

                if let Some(bind_address) = ctx.bind_address
                    && bind_address == ip
                {
                    effects.dials.push(address.clone());
                }

                effects
                    .published
                    .push(NetworkEvent::NodeDiscovered { peer_id: *peer_id });
            }
            mdns::Event::Expired(v) => {
                let (peer_id, address) = &v[0];

                debug!(target: "simulation::node::mdns_events", %peer_id, %address, "mdns record expired");

                effects.log_entries.push((
                    LogKind::Mdns(MdnsEventInfo::Expired {
                        peer_id: *peer_id,
                        address: address.clone(),
                    }),
                    dur,
                ));

                effects
                    .published
                    .push(NetworkEvent::NodeExpired { peer_id: *peer_id });
            }
        }

        Ok(true)
    }
}
