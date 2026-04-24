use libp2p::kad::QueryId;

/// Keeping track of important kademlia queries. Written to by
/// `RunningNode::apply_effects` as handlers produce `KadOp`s.
#[derive(Default, Debug)]
pub(crate) struct KadQueries {
    pub bootstrap: Option<QueryId>,
    pub providing_agent: Option<QueryId>,
    pub get_providers: Option<QueryId>,
}
