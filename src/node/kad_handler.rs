use std::time::Instant;

use color_eyre::eyre::Result;
use libp2p::{
    PeerId,
    kad::{self, GetRecordOk, InboundRequest, ProgressStep, QueryId, QueryResult, RecordKey},
};
use tracing::{debug, error, info, instrument, warn};

use crate::node::{NODE_NETWORK_AGENT, history::KadEventInfo, running::RunningNode};

/// Keeping track of important kademlia queries
#[derive(Default, Debug)]
pub(crate) struct KadQueries {
    pub bootstrap: Option<QueryId>,
    pub providing_agent: Option<QueryId>,
    pub get_providers: Option<QueryId>,
}

/// Handle an incoming kademlia event
#[instrument(skip_all, name = "kad_event")]
pub(crate) fn handle_event(node: &mut RunningNode, event: kad::Event) -> Result<()> {
    match event {
        kad::Event::InboundRequest { request } => {
            node.logger.add_kademlia_event(
                KadEventInfo::InboundRequest {
                    request: request.clone(),
                },
                Instant::now().duration_since(node.state.start()),
            );

            on_inbound_req(node, &request);
        }
        kad::Event::OutboundQueryProgressed {
            result, id, step, ..
        } => {
            node.logger.add_kademlia_event(
                KadEventInfo::OutboundQueryProgressed { id },
                Instant::now().duration_since(node.state.start()),
            );

            on_query_result(node, result, id, &step);
        }
        kad::Event::RoutingUpdated {
            peer,
            addresses,
            is_new_peer,
            old_peer,
            ..
        } => {
            node.logger.add_kademlia_event(
                KadEventInfo::RoutingUpdated {
                    peer,
                    is_new: is_new_peer,
                    addresses: addresses.clone(),
                    old_peer,
                },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::kademlia_events", %peer, ?addresses, "route updated");
        }
        kad::Event::RoutablePeer { peer, address, .. } => {
            node.logger.add_kademlia_event(
                KadEventInfo::RoutablePeer {
                    peer,
                    address: address.clone(),
                },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::kademlia_events", %peer, %address, "peer routable");
        }
        kad::Event::UnroutablePeer { peer } => {
            node.logger.add_kademlia_event(
                KadEventInfo::UnroutablePeer { peer },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::kademlia_events", %peer, "peer unroutable");
        }
        kad::Event::PendingRoutablePeer { peer, address, .. } => {
            node.logger.add_kademlia_event(
                KadEventInfo::PendingRoutablePeer { peer },
                Instant::now().duration_since(node.state.start()),
            );

            debug!(target: "simulation::node::kademlia_events", %peer, %address, "peer pending routable");
        }
        kad::Event::ModeChanged { new_mode } => {
            node.logger.add_kademlia_event(
                KadEventInfo::ModeChanged { new_mode },
                Instant::now().duration_since(node.state.start()),
            );

            info!(target: "simulation::node::kademlia_events", %new_mode, "node mode changed");

            node.base.kad_info.set_mode(new_mode);
        }
    }
    Ok(())
}

/// Handle an inbound kademlia request
fn on_inbound_req(_node: &mut RunningNode, request: &InboundRequest) {
    debug!(target: "simulation::node::kademlia_events", ?request, "inbound request received");

    // TODO
    match request {
        InboundRequest::FindNode { .. } => {}
        InboundRequest::GetProvider { .. } => {}
        InboundRequest::GetRecord { .. } => {}
        InboundRequest::AddProvider { .. } => {}
        InboundRequest::PutRecord { .. } => {}
    }
}

/// Handle result of kademlia query
fn on_query_result(node: &mut RunningNode, result: QueryResult, id: QueryId, step: &ProgressStep) {
    match result {
        QueryResult::Bootstrap(Ok(res)) => {
            if let Some(qid) = node.kad_queries.bootstrap
                && id == qid
            {
                if step.last {
                    node.kad_queries.bootstrap = None;
                    node.state.bootstrap();
                    node.base.kad_info.set_bootstrapped(true);

                    info!(target: "simulation::node::kademlia_events", "kademlia bootstrapped");

                    // Once the bootstrap nodes are connected to other
                    // bootstrap nodes we can set them to server mode
                    //if peer.is_bootstrap {
                    //    peer.swarm
                    //        .behaviour_mut()
                    //        .kademlia
                    //        .set_mode(Some(Mode::Server));
                    //}

                    // Get other providers of the agent string to
                    // get info about mesh

                    let key = RecordKey::new(&NODE_NETWORK_AGENT);
                    let qid = node.base.kad_get_providers(key);

                    node.kad_queries.get_providers = Some(qid);
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
            node.kad_queries.bootstrap = None;
        }
        QueryResult::GetClosestPeers(Ok(closest_res)) => {
            let key = PeerId::from_bytes(&closest_res.key);
            let peers = closest_res.peers;

            debug!(target: "simulation::node::kademlia_events", ?key, ?peers, "closest peers found");
            //peer.build_mesh(peers.peers);
        }
        QueryResult::GetClosestPeers(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", error = %e, "get closest peers failed");
        }
        QueryResult::GetProviders(Ok(providers)) => {
            if let Some(qid) = node.kad_queries.get_providers
                && id == qid
            {
                match providers {
                    kad::GetProvidersOk::FoundProviders { key, providers } => {
                        let key = String::from_utf8(key.to_vec());
                        debug!(target: "simulation::node::kademlia_events", ?key, "found providers");
                        for provider in &providers {
                            debug!(target: "simulation::node::kademlia_events", %provider, "provider found");

                            // Get other possible/peers providers
                            node.base.kad_get_closest_peers(*provider);
                        }
                        node.kad_queries.get_providers = None;
                    }
                    kad::GetProvidersOk::FinishedWithNoAdditionalRecord { closest_peers } => {
                        debug!(target: "simulation::node::kademlia_events", ?closest_peers, "get providers finished");
                        for new_peer in &closest_peers {
                            debug!(target: "simulation::node::kademlia_events", peer = %new_peer, "closest peer from providers");

                            // Get other possible/peers providers
                            node.base.kad_get_closest_peers(*new_peer);
                        }
                    }
                }
            }
        }
        QueryResult::GetProviders(Err(e)) => {
            if let Some(qid) = node.kad_queries.get_providers
                && id == qid
            {
                node.kad_queries.get_providers = None;
                error!(target: "simulation::node::kademlia_events", error = %e, "failed to get providers for wg mesh agent string");
            }
        }
        QueryResult::StartProviding(Ok(_)) => {
            if let Some(qid) = node.kad_queries.providing_agent
                && id == qid
                && step.last
            {
                node.kad_queries.providing_agent = None;
                debug!(target: "simulation::node::kademlia_events", "node providing wg mesh agent string");
            }
        }
        QueryResult::StartProviding(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", error = %e, "start providing failed");

            if let Some(qid) = node.kad_queries.providing_agent
                && id == qid
            {
                error!(target: "simulation::node::kademlia_events", "failed to provide wg agent string");
                node.kad_queries.providing_agent = None;
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
