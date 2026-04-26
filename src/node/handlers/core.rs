use std::time::Instant;

use libp2p::{core::ConnectedPoint, swarm::SwarmEvent};
use tracing::{debug, error, info};

use crate::{
    messages::NetworkEvent,
    node::{NodeError, behaviour::NodeNetworkEvent, history::SwarmEventInfo},
};

use super::{ConnectionOp, Effects, HandlerCtx, KadOp, LogKind, SwarmEventHandler};

/// Handles non-behaviour swarm events (connections, listeners, errors).
pub(crate) struct CoreSwarmHandler;

impl SwarmEventHandler for CoreSwarmHandler {
    fn handle(
        &self,
        event: &SwarmEvent<NodeNetworkEvent>,
        ctx: &HandlerCtx<'_>,
        effects: &mut Effects,
    ) -> Result<bool, NodeError> {
        let dur = Instant::now().duration_since(ctx.start_instant);

        match event {
            SwarmEvent::Dialing { peer_id, .. } => {
                debug!(target: "simulation::node", ?peer_id, "dialing peer");
                effects.log_entries.push((
                    LogKind::Swarm(SwarmEventInfo::Dialing { peer_id: *peer_id }),
                    dur,
                ));
            }
            SwarmEvent::IncomingConnection { send_back_addr, .. } => {
                debug!(target: "simulation::node", addr = %send_back_addr, "incoming connection");
                effects.log_entries.push((
                    LogKind::Swarm(SwarmEventInfo::IncomingConnection {
                        address: send_back_addr.clone(),
                    }),
                    dur,
                ));
            }
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
                ..
            } => {
                effects.log_entries.push((
                    LogKind::Swarm(SwarmEventInfo::NewListenAddr {
                        listener_id: *listener_id,
                        address: address.clone(),
                    }),
                    dur,
                ));

                effects.published.push(NetworkEvent::NodeListening {
                    peer_id: ctx.self_peer_id,
                    address: address.clone(),
                });
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!(target: "simulation::node", %peer_id, ?endpoint, "connection established");

                let peer_addr = match endpoint.clone() {
                    ConnectedPoint::Dialer { address, .. } => address,
                    ConnectedPoint::Listener { send_back_addr, .. } => send_back_addr,
                };

                effects.kad_ops.push(KadOp::AddAddress {
                    peer: *peer_id,
                    addr: peer_addr,
                });

                effects
                    .connection_ops
                    .push(ConnectionOp::AddActive { peer: *peer_id });

                effects.log_entries.push((
                    LogKind::Swarm(SwarmEventInfo::ConnectionEstablished { peer_id: *peer_id }),
                    dur,
                ));

                effects.published.push(NetworkEvent::NodesConnected {
                    peer_one: *peer_id,
                    peer_two: ctx.self_peer_id,
                });

                if !ctx.bootstrapped {
                    debug!(target: "simulation::node::kademlia_events", "attempting kademlia bootstrapping");
                    effects.kad_ops.push(KadOp::Bootstrap);
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                endpoint,
                cause,
                num_established,
                ..
            } => {
                info!(target: "simulation::node", %peer_id, ?endpoint, ?cause, "connection closed");

                effects.log_entries.push((
                    LogKind::Swarm(SwarmEventInfo::ConnectionClosed { peer_id: *peer_id }),
                    dur,
                ));

                effects.published.push(NetworkEvent::NodesDisconnected {
                    peer_one: *peer_id,
                    peer_two: ctx.self_peer_id,
                });

                if *num_established == 0 {
                    effects
                        .connection_ops
                        .push(ConnectionOp::RemoveActive { peer: *peer_id });
                }
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                ..
            } => {
                debug!(target: "simulation::node", "listener now closed");
                effects.log_entries.push((
                    LogKind::Swarm(SwarmEventInfo::ListenerClosed {
                        listener_id: *listener_id,
                        addresses: addresses.clone(),
                    }),
                    dur,
                ));
            }
            SwarmEvent::IncomingConnectionError { peer_id, error, .. } => {
                error!(target: "simulation::node", ?peer_id, %error, "incoming connection failed");
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                error!(target: "simulation::node", ?peer_id, %error, "outgoing connection failed");
            }
            // Behaviour events and any unknown events are not ours.
            _ => return Ok(false),
        }

        Ok(true)
    }
}
