use std::time::Instant;

use color_eyre::eyre::Result;
use libp2p::mdns;
use mdns::Event as MdnsEvent;
use tracing::{debug, info, warn};

use crate::{
    node::{history::MdnsEventInfo, running::RunningNode},
    util,
};

/// Handle an incoming identify event
pub(crate) fn handle_event(node: &mut RunningNode, event: MdnsEvent) -> Result<()> {
    match event {
        MdnsEvent::Discovered(v) => {
            let (peer_id, address) = &v[0];

            debug!(target: "simulation::node::mdns_events", "mdns discovered peer {peer_id} {address}");

            node.logger.add_mdns_event(
                MdnsEventInfo::Discovered {
                    peer_id: peer_id.to_owned(),
                    address: address.to_owned(),
                },
                Instant::now().duration_since(node.state.start()),
            );

            let is_loopback = address.iter().any(
                |proto| matches!(proto, libp2p::multiaddr::Protocol::Ip4(ip) if ip.is_loopback()),
            );

            let ip = util::extract_ip(address).expect("mdns record has not ip address");

            debug!(target: "simulation::node::mdns_events", "address is loopback: {is_loopback}");

            if let Some(bind_address) = node.base.bind_address
                && bind_address == ip
            {
                match node.base.dial(address.to_owned()) {
                    Ok(()) => {
                        debug!(target: "simulation::node::mdns_events", "dialed peer from mdns discovery");
                    }
                    Err(e) => {
                        warn!(target: "simulation::node::mdns_events", "failed to dial peer from mdns discovery: {e}");
                    }
                }
            }
        }
        MdnsEvent::Expired(v) => {
            let (peer_id, address) = &v[0];

            debug!(target: "simulation::node::mdns_events", "mdns record expired for {peer_id} {address}");

            node.logger.add_mdns_event(
                MdnsEventInfo::Expired {
                    peer_id: peer_id.to_owned(),
                    address: address.to_owned(),
                },
                Instant::now().duration_since(node.state.start()),
            );
        }
    }

    Ok(())
}
