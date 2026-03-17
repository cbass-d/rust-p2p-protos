use std::time::Instant;

use color_eyre::eyre::Result;
use libp2p::{
    PeerId,
    kad::{self, GetRecordOk, InboundRequest, ProgressStep, QueryId, QueryResult, RecordKey},
};
use tracing::{debug, error, info, warn};

use crate::node::{NODE_NETWORK_AGENT, history::KadEventInfo, running::RunningNode};

/// Keeping track of important kademlia queries
#[derive(Default, Debug)]
pub(crate) struct KadQueries {
    pub bootsrap_id: Option<QueryId>,
    pub providing_agent_id: Option<QueryId>,
    pub get_providers_id: Option<QueryId>,
}

/// Handle an incoming kademlia event
pub(crate) fn handle_event(node: &mut RunningNode, event: kad::Event) -> Result<()> {
    match event {
        kad::Event::InboundRequest { request } => {
            node.logger.add_kademlia_event(
                KadEventInfo::InboundRequest {
                    request: request.clone(),
                },
                Instant::now().duration_since(node.start),
            );

            on_inbound_req(node, request);
        }
        kad::Event::OutboundQueryProgressed {
            result, id, step, ..
        } => {
            node.logger.add_kademlia_event(
                KadEventInfo::OutboundQueryProgressed { id },
                Instant::now().duration_since(node.start),
            );

            on_query_result(node, result, id, step);
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
                Instant::now().duration_since(node.start),
            );

            debug!(target: "simulation::node::kademlia_events", "route updated for {peer}: {:?}", addresses);
        }
        kad::Event::RoutablePeer { peer, address, .. } => {
            node.logger.add_kademlia_event(
                KadEventInfo::RoutablePeer {
                    peer,
                    address: address.clone(),
                },
                Instant::now().duration_since(node.start),
            );

            debug!(target: "simulation::node::kademlia_events", "peer {peer} {:?} routable", address);
        }
        kad::Event::PendingRoutablePeer { peer, address, .. } => {
            node.logger.add_kademlia_event(
                KadEventInfo::PendingRoutablePeer { peer },
                Instant::now().duration_since(node.start),
            );

            debug!(target: "simulation::node::kademlia_events", "peer {peer} {:?} pending routable", address);
        }
        kad::Event::ModeChanged { new_mode } => {
            node.logger.add_kademlia_event(
                KadEventInfo::ModeChanged { new_mode },
                Instant::now().duration_since(node.start),
            );

            info!(target: "simulation::node::kademlia_events", "node mode changed to {new_mode}");

            node.base.kad_info.set_mode(new_mode);
        }
        other => {
            debug!("some other kad event: {:?}", other);
        }
    }
    Ok(())
}

/// Handle an inbound kademlia request
fn on_inbound_req(_node: &mut RunningNode, request: InboundRequest) {
    match request {
        InboundRequest::FindNode { .. } => {}
        InboundRequest::GetProvider { .. } => {}
        InboundRequest::GetRecord { .. } => {}
        InboundRequest::AddProvider { .. } => {}
        InboundRequest::PutRecord { .. } => {}
    }
}

/// Handle result of kademlia query
fn on_query_result(node: &mut RunningNode, result: QueryResult, id: QueryId, step: ProgressStep) {
    match result {
        QueryResult::Bootstrap(Ok(res)) => {
            if let Some(qid) = node.kad_queries.bootsrap_id
                && id == qid
            {
                if step.last {
                    node.kad_queries.bootsrap_id = None;
                    node.bootstrapped = true;
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

                    node.kad_queries.get_providers_id = Some(qid);
                } else {
                    debug!(
                        target: "simulation::node::kademlia_events",
                        "kademlia bootstrapping peer {}, remaining {}",
                        res.peer, res.num_remaining
                    )
                }
            }
        }
        QueryResult::Bootstrap(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", "failed to bootstrap error: {e}");
            node.kad_queries.bootsrap_id = None;
        }
        QueryResult::GetClosestPeers(Ok(closest_res)) => {
            let key = PeerId::from_bytes(&closest_res.key);
            let peers = closest_res.peers;

            debug!(target: "simulation::node::kademlia_events", "closest peers for key {:?}: {:?}", key, peers);
            //peer.build_mesh(peers.peers);
        }
        QueryResult::GetClosestPeers(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", "get closest peers error: {e}");
        }
        QueryResult::GetProviders(Ok(providers)) => {
            if let Some(qid) = node.kad_queries.get_providers_id
                && id == qid
            {
                match providers {
                    kad::GetProvidersOk::FoundProviders { key, providers } => {
                        let key = String::from_utf8(key.to_vec());
                        debug!(target: "simulation::node::kademlia_events", "found providers for {:?}:", key);
                        for provider in &providers {
                            debug!(target: "simulation::node::kademlia_events", "- {provider}");

                            // Get other possible/peers providers
                            node.base.kad_get_closest_peers(*provider);
                        }
                        node.kad_queries.get_providers_id = None;
                    }
                    kad::GetProvidersOk::FinishedWithNoAdditionalRecord { closest_peers } => {
                        debug!(target: "simulation::node::kademlia_events", "get providers closest_peers: {:?}", closest_peers);
                        for new_peer in &closest_peers {
                            debug!(target: "simulation::node::kademlia_events", "- {new_peer}");

                            // Get other possible/peers providers
                            node.base.kad_get_closest_peers(*new_peer);
                        }
                    }
                }
            }
        }
        QueryResult::GetProviders(Err(e)) => {
            if let Some(qid) = node.kad_queries.get_providers_id
                && id == qid
            {
                node.kad_queries.get_providers_id = None;
                error!(target: "simulation::node::kademlia_events", "failed to get providers for wg mesh agent string: {e}");
            }
        }
        QueryResult::StartProviding(Ok(_)) => {
            if let Some(qid) = node.kad_queries.providing_agent_id
                && id == qid
                && step.last
            {
                node.kad_queries.providing_agent_id = None;
                debug!(target: "simulation::node::kademlia_events", "node providing wg mesh agent string");
            }
        }
        QueryResult::StartProviding(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", "start providing error: {e}");

            if let Some(qid) = node.kad_queries.providing_agent_id
                && id == qid
            {
                error!(target: "simulation::node::kademlia_events", "failed to provide wg agent string");
                node.kad_queries.providing_agent_id = None;
            }
        }
        QueryResult::RepublishRecord(_) => {
            debug!(target: "simulation::node::kademlia_events", "handling republish record result");
        }
        QueryResult::GetRecord(Ok(record_res)) => match record_res {
            GetRecordOk::FoundRecord(record) => {
                debug!(target: "simulation::node::kademlia_events", "found record {:?} at {:?}", record.record, record.peer);
            }
            _ => {
                debug!(target: "simulation::node::kademlia_events", "get record finished with no additional records");
            }
        },
        QueryResult::GetRecord(Err(e)) => {
            warn!(target: "simulation::node::kademlia_events", "get record error: {e}");
        }
        QueryResult::PutRecord(Ok(res)) => {
            let key = String::from_utf8(res.key.to_vec());
            debug!(target: "simulation::node::kademlia_events", "put record result: {:?}", key);
        }
        QueryResult::PutRecord(Err(e)) => {
            debug!(target: "simulation::node::kademlia_events", "put record error: {e}");
        }
        QueryResult::RepublishProvider(_) => {
            debug!(target: "simulation::node::kademlia_events", "handling republish provider result");
        }
    }
}
