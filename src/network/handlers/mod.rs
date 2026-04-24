//! Per-protocol network command handlers, one `handle` fn per sub-enum.

use std::{collections::HashMap, net::Ipv4Addr};

use libp2p::{Multiaddr, PeerId};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::{
    bus::EventBus,
    messages::{NetworkEvent, NodeCommand, NodeResponse},
    network::TransportMode,
};

mod identify;
mod kademlia;
mod lifecycle;
mod swarm;

pub(crate) use identify::handle as handle_identify;
pub(crate) use kademlia::handle as handle_kademlia;
pub(crate) use lifecycle::handle as handle_lifecycle;
pub(crate) use swarm::handle as handle_swarm;

/// Mutable view of `NodeNetwork` state passed to each protocol handler.
pub(crate) struct ControlCtx<'a> {
    pub nodes: &'a mut HashMap<PeerId, mpsc::Sender<(NodeCommand, oneshot::Sender<NodeResponse>)>>,
    pub addresses: &'a mut HashMap<PeerId, Multiaddr>,
    pub network_event_tx: &'a EventBus<NetworkEvent>,
    pub max_nodes: u8,
    pub transport: TransportMode,
    pub bind_address: Option<Ipv4Addr>,
    pub cancellation_token: &'a CancellationToken,
}

/// Send a `NodeCommand` to a peer and publish the translated reply on the bus.
/// Single chokepoint for per-node command traffic.
pub(crate) async fn request<F>(
    target: PeerId,
    node_cmd: NodeCommand,
    ctx: &ControlCtx<'_>,
    translate: F,
) where
    F: FnOnce(NodeResponse) -> Option<NetworkEvent>,
{
    let Some(ch) = ctx.nodes.get(&target) else {
        warn!(target: "simulation::network", %target, "no channel for peer");
        return;
    };
    let (tx, rx) = oneshot::channel();
    if let Err(e) = ch.send((node_cmd, tx)).await {
        warn!(target: "simulation::network", error = %e, "command send failed");
        return;
    }
    match rx.await {
        Ok(resp) => {
            if let Some(evt) = translate(resp) {
                ctx.network_event_tx.publish(evt);
            }
        }
        Err(e) => warn!(target: "simulation::network", error = %e, "oneshot reply failed"),
    }
}
