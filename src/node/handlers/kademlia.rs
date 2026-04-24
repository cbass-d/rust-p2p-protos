use std::time::Instant;

use libp2p::{
    PeerId,
    kad::{self, GetRecordOk, InboundRequest, ProgressStep, QueryId, QueryResult, RecordKey},
    swarm::SwarmEvent,
};
use tracing::{debug, error, info, warn};

use crate::node::{
    NODE_NETWORK_AGENT, NodeError, behaviour::NodeNetworkEvent, history::KadEventInfo,
};

use super::{Effects, HandlerCtx, KadOp, LogKind, StateMutation, SwarmEventHandler};

/// Handles libp2p Kademlia protocol events.
pub(crate) struct KadEventHandler;

impl SwarmEventHandler for KadEventHandler {
    fn handle(
        &self,
        event: &SwarmEvent<NodeNetworkEvent>,
        ctx: &HandlerCtx<'_>,
        effects: &mut Effects,
    ) -> Result<bool, NodeError> {
        let SwarmEvent::Behaviour(NodeNetworkEvent::Kademlia(e)) = event else {
            return Ok(false);
        };

        let dur = Instant::now().duration_since(ctx.start_instant);

        match e {
            kad::Event::InboundRequest { request } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::InboundRequest {
                        request: request.clone(),
                    }),
                    dur,
                ));

                handle_inbound_request(request);
            }
            kad::Event::OutboundQueryProgressed {
                result, id, step, ..
            } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::OutboundQueryProgressed { id: *id }),
                    dur,
                ));

                handle_query_result(result, *id, step, ctx, effects);
            }
            kad::Event::RoutingUpdated {
                peer,
                addresses,
                is_new_peer,
                old_peer,
                ..
            } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::RoutingUpdated {
                        peer: *peer,
                        is_new: *is_new_peer,
                        addresses: addresses.clone(),
                        old_peer: *old_peer,
                    }),
                    dur,
                ));

                debug!(target: "simulation::node::kademlia_events", %peer, ?addresses, "route updated");
            }
            kad::Event::RoutablePeer { peer, address, .. } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::RoutablePeer {
                        peer: *peer,
                        address: address.clone(),
                    }),
                    dur,
                ));

                debug!(target: "simulation::node::kademlia_events", %peer, %address, "peer routable");
            }
            kad::Event::UnroutablePeer { peer } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::UnroutablePeer { peer: *peer }),
                    dur,
                ));

                debug!(target: "simulation::node::kademlia_events", %peer, "peer unroutable");
            }
            kad::Event::PendingRoutablePeer { peer, address, .. } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::PendingRoutablePeer { peer: *peer }),
                    dur,
                ));

                debug!(target: "simulation::node::kademlia_events", %peer, %address, "peer pending routable");
            }
            kad::Event::ModeChanged { new_mode } => {
                effects.log_entries.push((
                    LogKind::Kademlia(KadEventInfo::ModeChanged {
                        new_mode: *new_mode,
                    }),
                    dur,
                ));

                info!(target: "simulation::node::kademlia_events", %new_mode, "node mode changed");
                effects.kad_ops.push(KadOp::SetMode(*new_mode));
            }
        }

        Ok(true)
    }
}

fn handle_inbound_request(request: &InboundRequest) {
    debug!(target: "simulation::node::kademlia_events", ?request, "inbound request received");
    // Placeholder — the old handler had per-variant TODOs with empty bodies.
    match request {
        InboundRequest::FindNode { .. } => {}
        InboundRequest::GetProvider { .. } => {}
        InboundRequest::GetRecord { .. } => {}
        InboundRequest::AddProvider { .. } => {}
        InboundRequest::PutRecord { .. } => {}
    }
}

fn handle_query_result(
    result: &QueryResult,
    id: QueryId,
    step: &ProgressStep,
    ctx: &HandlerCtx<'_>,
    effects: &mut Effects,
) {
    match result {
        QueryResult::Bootstrap(Ok(res)) => {
            if let Some(qid) = ctx.kad_queries.bootstrap
                && id == qid
            {
                if step.last {
                    effects
                        .state_mutations
                        .push(StateMutation::ClearBootstrapQuery);
                    effects
                        .state_mutations
                        .push(StateMutation::MarkBootstrapped);
                    effects.kad_ops.push(KadOp::SetBootstrapped(true));

                    info!(target: "simulation::node::kademlia_events", "kademlia bootstrapped");

                    // Start advertising / discovering peers providing the agent string.
                    effects.kad_ops.push(KadOp::GetProviders {
                        key: RecordKey::new(&NODE_NETWORK_AGENT),
                    });
                } else {
                    debug!(
                        target: "simulation::node::kademlia_events",
                        peer = %res.peer, remaining = res.num_remaining,
                        "kademlia bootstrapping",
                    );
                }
            }
        }
        QueryResult::Bootstrap(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", error = %e, "failed to bootstrap");
            effects
                .state_mutations
                .push(StateMutation::ClearBootstrapQuery);
        }
        QueryResult::GetClosestPeers(Ok(closest_res)) => {
            let key = PeerId::from_bytes(&closest_res.key);
            debug!(target: "simulation::node::kademlia_events", ?key, peers = ?closest_res.peers, "closest peers found");
        }
        QueryResult::GetClosestPeers(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", error = %e, "get closest peers failed");
        }
        QueryResult::GetProviders(Ok(providers)) => {
            if let Some(qid) = ctx.kad_queries.get_providers
                && id == qid
            {
                match providers {
                    kad::GetProvidersOk::FoundProviders { key, providers } => {
                        let key = String::from_utf8(key.to_vec());
                        debug!(target: "simulation::node::kademlia_events", ?key, "found providers");
                        for provider in providers {
                            debug!(target: "simulation::node::kademlia_events", %provider, "provider found");
                            effects
                                .kad_ops
                                .push(KadOp::GetClosestPeers { peer: *provider });
                        }
                        effects
                            .state_mutations
                            .push(StateMutation::ClearGetProvidersQuery);
                    }
                    kad::GetProvidersOk::FinishedWithNoAdditionalRecord { closest_peers } => {
                        debug!(target: "simulation::node::kademlia_events", ?closest_peers, "get providers finished");
                        for new_peer in closest_peers {
                            debug!(target: "simulation::node::kademlia_events", peer = %new_peer, "closest peer from providers");
                            effects
                                .kad_ops
                                .push(KadOp::GetClosestPeers { peer: *new_peer });
                        }
                    }
                }
            }
        }
        QueryResult::GetProviders(Err(e)) => {
            if let Some(qid) = ctx.kad_queries.get_providers
                && id == qid
            {
                effects
                    .state_mutations
                    .push(StateMutation::ClearGetProvidersQuery);
                error!(target: "simulation::node::kademlia_events", error = %e, "failed to get providers for agent string");
            }
        }
        QueryResult::StartProviding(Ok(_)) => {
            if let Some(qid) = ctx.kad_queries.providing_agent
                && id == qid
                && step.last
            {
                effects
                    .state_mutations
                    .push(StateMutation::ClearProvidingAgentQuery);
                debug!(target: "simulation::node::kademlia_events", "node providing agent string");
            }
        }
        QueryResult::StartProviding(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", error = %e, "start providing failed");
            if let Some(qid) = ctx.kad_queries.providing_agent
                && id == qid
            {
                error!(target: "simulation::node::kademlia_events", "failed to provide agent string");
                effects
                    .state_mutations
                    .push(StateMutation::ClearProvidingAgentQuery);
            }
        }
        QueryResult::RepublishRecord(_) => {
            debug!(target: "simulation::node::kademlia_events", "handling republish record result");
        }
        QueryResult::GetRecord(Ok(record_res)) => match record_res {
            GetRecordOk::FoundRecord(record) => {
                debug!(target: "simulation::node::kademlia_events", record = ?record.record, peer = ?record.peer, "found record");
            }
            GetRecordOk::FinishedWithNoAdditionalRecord { .. } => {
                debug!(target: "simulation::node::kademlia_events", "get record finished with no additional records");
            }
        },
        QueryResult::GetRecord(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", error = %e, "get record failed");
        }
        QueryResult::PutRecord(Ok(res)) => {
            let key = String::from_utf8(res.key.to_vec());
            debug!(target: "simulation::node::kademlia_events", ?key, "put record result");
        }
        QueryResult::PutRecord(Err(e)) => {
            debug!(target: "simulation::node::kademlia_events", error = %e, "put record failed");
        }
        QueryResult::RepublishProvider(_) => {
            debug!(target: "simulation::node::kademlia_events", "handling republish provider result");
        }
    }
}
