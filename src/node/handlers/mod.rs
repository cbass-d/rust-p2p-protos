//! Per-event-type node-layer handlers. Handlers are tried in order; the
//! first to return `Ok(true)` claims the event.

use std::{net::Ipv4Addr, time::Duration, time::Instant};

use libp2p::{
    Multiaddr, PeerId,
    kad::{self, RecordKey},
    swarm::SwarmEvent,
};

use crate::{
    messages::NetworkEvent,
    node::{
        NodeError,
        behaviour::NodeNetworkEvent,
        connection_tracker::ConnectionTracker,
        history::{IdentifyEventInfo, KadEventInfo, MdnsEventInfo, SwarmEventInfo},
    },
};

mod core;
mod identify;
mod kademlia;
mod mdns;

pub(crate) use core::CoreSwarmHandler;
pub(crate) use identify::IdentifyEventHandler;
pub(crate) use kademlia::{KadEventHandler, KadQueries};
pub(crate) use mdns::MdnsEventHandler;

/// Turns a swarm event into effects. `Ok(true)` claims the event,
/// `Ok(false)` passes it on, `Err` is a soft failure (logged, not fatal).
pub(crate) trait SwarmEventHandler: Send {
    fn handle(
        &self,
        event: &SwarmEvent<NodeNetworkEvent>,
        ctx: &HandlerCtx<'_>,
        effects: &mut Effects,
    ) -> Result<bool, NodeError>;
}

/// Read-only view of `RunningNode` state handed to each handler.
pub(crate) struct HandlerCtx<'a> {
    pub self_peer_id: PeerId,
    pub bind_address: Option<Ipv4Addr>,
    pub start_instant: Instant,
    pub bootstrapped: bool,
    pub kad_queries: &'a KadQueries,
    pub connections: &'a ConnectionTracker,
}

/// Everything a handler asks the node to do; applied in the order the
/// fields are declared.
#[derive(Default)]
pub(crate) struct Effects {
    pub log_entries: Vec<(LogKind, Duration)>,
    pub dials: Vec<Multiaddr>,
    pub kad_ops: Vec<KadOp>,
    pub connection_ops: Vec<ConnectionOp>,
    pub state_mutations: Vec<StateMutation>,
    pub published: Vec<NetworkEvent>,
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum LogKind {
    Swarm(SwarmEventInfo),
    Identify(IdentifyEventInfo),
    Kademlia(KadEventInfo),
    Mdns(MdnsEventInfo),
}

pub(crate) enum KadOp {
    AddAddress { peer: PeerId, addr: Multiaddr },
    RemovePeer { peer: PeerId },
    Bootstrap,
    GetProviders { key: RecordKey },
    GetClosestPeers { peer: PeerId },
    SetMode(kad::Mode),
    SetBootstrapped(bool),
}

pub(crate) enum ConnectionOp {
    AddActive { peer: PeerId },
    RemoveActive { peer: PeerId },
}

pub(crate) enum StateMutation {
    MarkBootstrapped,
    ClearBootstrapQuery,
    ClearGetProvidersQuery,
    ClearProvidingAgentQuery,
}
