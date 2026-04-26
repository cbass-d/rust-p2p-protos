use libp2p::{Multiaddr, swarm::SwarmEvent};
use tokio::sync::mpsc;

use crate::{
    bus::EventBus,
    messages::{CommandChannel, NetworkEvent, NodeCommand, NodeResponse},
    node::{
        NodeError, NodeResult,
        base::NodeBase,
        behaviour::NodeNetworkEvent,
        connection_tracker::ConnectionTracker,
        handlers::{
            ConnectionOp, Effects, HandlerCtx, KadOp, KadQueries, LogKind, StateMutation,
            SwarmEventHandler,
        },
        logger::NodeLogger,
        state::State,
    },
};
use tracing::{debug, error, info, instrument, trace, warn};

/// A node that is actively running in the network
pub(crate) struct RunningNode {
    /// The core/base of the node
    pub(crate) base: NodeBase,

    /// Manages the active connections to other peers in the libp2p swarm
    pub(crate) connection_tracker: ConnectionTracker,

    /// Manages the state of the node
    pub(crate) state: State,

    /// mpsc channel for receiving commands to perform from the network
    pub(crate) from_network: mpsc::Receiver<CommandChannel>,

    /// Broadcast bus for Network Events (shared across all nodes).
    pub(crate) network_event_tx: EventBus<NetworkEvent>,

    /// Handler for logging of events
    pub(crate) logger: NodeLogger,

    /// Important kad queries that we must keep track of
    pub(crate) kad_queries: KadQueries,

    /// Ordered list of per-protocol swarm event handlers. First handler
    /// returning `Ok(true)` claims the event; the rest are skipped.
    pub(crate) handlers: Vec<Box<dyn SwarmEventHandler>>,
}

impl RunningNode {
    /// Main run loop of for the node
    #[instrument(skip_all, fields(id = %self.base.peer_id), name = "running node")]
    pub async fn run(&mut self) -> Result<NodeResult, NodeError> {
        info!(target: "simulation::node",
            peer_id = %self.base.peer_id,
            "node now running",
        );

        // Send out event that the node is now running
        let running_event = NetworkEvent::NodeRunning {
            peer_id: self.base.peer_id,
            message_history: self.logger.history(),
            stats: self.logger.stats(),
            node_connections: self.connection_tracker.connections(),
            mode: self.base.kad_mode,
        };

        self.apply_effects(Effects {
            published: vec![running_event],
            ..Default::default()
        });

        self.base.listen()?;

        info!(target: "simulation::node", listen_address = %self.base.listen_address, "node swarm listening");

        self.dial_known_peers();

        while !self.state.stopped() {
            tokio::select! {
                () = self.state.cancelled() => {
                    debug!(target: "simulation::node", "cancellation token signal received");
                    break;
                },
                Some(message) = self.from_network.recv() => {
                    self.handle_network_message(message);
                },

                maybe_event = self.base.next_event() => {
                    if let Some(event) = maybe_event {
                        trace!("node swarm event {:?}", event);

                        self.handle_swarm_event(event).await;
                    } else {
                        error!(target: "simulation::node", "swarm stream ended");
                        return Err(NodeError::SwarmStreamEnded);
                    }
                },
            }
        }

        info!(target: "simulation::node", peer_id = %self.base.peer_id, "node now shutting down");

        // Send out event that the node has stopped running
        self.apply_effects(Effects {
            published: vec![NetworkEvent::NodeStopped {
                peer_id: self.base.peer_id,
            }],
            ..Default::default()
        });

        if self.state.killed() {
            Ok(NodeResult::Killed)
        } else {
            Ok(NodeResult::Success)
        }
    }

    /// Dial known peers passed by CLI and add to Kademlia routing table
    fn dial_known_peers(&mut self) {
        info!(target: "simulation::node", "dialing known peers");
        let mut dialed = 0;
        for addr in self.connection_tracker.known() {
            if self.base.dial(addr.clone()).is_ok() {
                debug!(target: "simulation::node", addr = %addr, "successfully dialed peer");
                dialed += 1;
            } else {
                warn!(target: "simulation::node", addr = %addr, "failed to dial peer");
            }
        }

        info!(target: "simulation::node", dialed, "dialed known peers");
    }

    /// Handle a message coming from the node network
    fn handle_network_message(&mut self, message: CommandChannel) {
        let (command, reply_tx) = message;
        let response = self.handle_node_command(command);
        if let Err(e) = reply_tx.send(response) {
            warn!(target: "simulation::node", error = ?e, "failed to send command response");
        }
    }

    /// Walk each handler until one claims the event, then apply the effects.
    async fn handle_swarm_event(&mut self, event: SwarmEvent<NodeNetworkEvent>) {
        self.logger.increment_recvd();

        let ctx = HandlerCtx {
            self_peer_id: self.base.peer_id,
            bind_address: self.base.bind_address,
            start_instant: self.state.start(),
            bootstrapped: self.state.bootstrapped(),
            kad_queries: &self.kad_queries,
            connections: &self.connection_tracker,
        };

        let mut effects = Effects::default();
        let mut claimed = false;
        for handler in &self.handlers {
            match handler.handle(&event, &ctx, &mut effects) {
                Ok(true) => {
                    claimed = true;
                    break;
                }
                Ok(false) => continue,
                Err(e) => warn!(target: "simulation::node", error = %e, "handler error"),
            }
        }
        if !claimed {
            trace!("unhandled swarm event: {:?}", event);
        }

        self.apply_effects(effects);
    }

    /// The single imperative shell for swarm-driven side effects.
    fn apply_effects(&mut self, effects: Effects) {
        for (kind, dur) in effects.log_entries {
            match kind {
                LogKind::Swarm(info) => self.logger.add_swarm_event(info, dur),
                LogKind::Identify(info) => self.logger.add_identify_event(info, dur),
                LogKind::Kademlia(info) => self.logger.add_kademlia_event(info, dur),
                LogKind::Mdns(info) => self.logger.add_mdns_event(info, dur),
            }
        }

        for addr in effects.dials {
            if let Err(e) = self.base.dial(addr.clone()) {
                warn!(target: "simulation::node", %addr, error = %e, "dial failed");
            }
        }

        for op in effects.kad_ops {
            match op {
                KadOp::AddAddress { peer, addr } => {
                    self.base.kad_add_address(&peer, addr);
                }
                KadOp::RemovePeer { peer } => {
                    self.base.kad_remove_peer(&peer);
                }
                KadOp::Bootstrap => match self.base.kad_bootstrap() {
                    Ok(qid) => {
                        debug!(target: "simulation::node::kademlia_events", "kademlia bootstrap started");
                        self.kad_queries.bootstrap = Some(qid);
                    }
                    Err(e) => {
                        warn!(target: "simulation::node::kademlia_events", error = %e, "kademlia bootstrap failed");
                    }
                },
                KadOp::GetProviders { key } => {
                    let qid = self.base.kad_get_providers(key);
                    self.kad_queries.get_providers = Some(qid);
                }
                KadOp::GetClosestPeers { peer } => {
                    self.base.kad_get_closest_peers(peer);
                }
                KadOp::SetMode(mode) => self.base.kad_info.set_mode(mode),
                KadOp::SetBootstrapped(b) => self.base.kad_info.set_bootstrapped(b),
            }
        }

        for op in effects.connection_ops {
            match op {
                ConnectionOp::AddActive { peer } => self.connection_tracker.add_active_peer(peer),
                ConnectionOp::RemoveActive { peer } => {
                    self.connection_tracker.remove_active_peer(&peer);
                }
            }
        }
        debug!(target: "simulation::node", peer_count = self.connection_tracker.connection_count(), "peer count");

        for mutation in effects.state_mutations {
            match mutation {
                StateMutation::MarkBootstrapped => self.state.bootstrap(),
                StateMutation::ClearBootstrapQuery => self.kad_queries.bootstrap = None,
                StateMutation::ClearGetProvidersQuery => self.kad_queries.get_providers = None,
                StateMutation::ClearProvidingAgentQuery => self.kad_queries.providing_agent = None,
            }
        }

        for event in effects.published {
            self.network_event_tx.publish(event);
        }
    }

    /// Dial a peer; transport is dispatched by the swarm based on the multiaddr.
    fn connect_to_peer(&mut self, peer: Multiaddr) -> NodeResponse {
        match self.base.dial(peer.clone()) {
            Ok(()) => {
                debug!(target: "simulation::node", addr = %peer, "successfully dialed peer");
                NodeResponse::Dialed { addr: peer }
            }
            Err(e) => {
                warn!(target: "simulation::node", addr = %peer, error = %e, "failed to dial peer");
                NodeResponse::Failed
            }
        }
    }

    // Handles an incoming node command from the node network, returns the NodeResponse if any
    fn handle_node_command(&mut self, command: NodeCommand) -> NodeResponse {
        debug!(target: "simulation::node", cmd = ?command, "new node command recieved");
        match command {
            NodeCommand::ConnectTo { peer } => {
                debug!(target: "simulation::node", addr = %peer, "connect to command received");
                self.connect_to_peer(peer)
            }
            NodeCommand::DisconnectFrom { peer } => {
                debug!(target: "simulation::node", %peer, "disconnect from command received");
                if self.base.disconnect_peer(peer).is_ok() {
                    debug!(target: "simulation::node", %peer, "successfully disconnected");

                    NodeResponse::Disconnected { peer }
                } else {
                    warn!(target: "simulation::node", %peer, "failed to disconnect");

                    NodeResponse::Failed
                }
            }
            NodeCommand::GetIdentifyInfo => {
                let info = self.base.identify_info.clone();

                NodeResponse::IdentifyInfo { info }
            }
            NodeCommand::GetKademliaInfo => {
                self.update_kademlia_info();
                let info = self.base.kad_info.clone();

                NodeResponse::KademliaInfo { info }
            }
            NodeCommand::PutRecord { key, value } => {
                debug!(target: "simulation::node", "put record command received: {key} - {value}");

                if let Err(e) = self.base.put_record(key.clone(), value.clone()) {
                    error!(target: "simulation::node", "put record command failed: {e}");
                    NodeResponse::Failed
                } else {
                    NodeResponse::RecordPlaced
                }
            }
            NodeCommand::GetRecord { key } => {
                debug!(target: "simulation::node", "get record command received: {key}");

                self.base.get_record(key.clone());
                NodeResponse::GetRecordPlaced
            }
            NodeCommand::ListRecords => {
                debug!(target: "simulation::node", "list records command received");

                let records = self.base.list_records();

                debug!(target: "simulation::node", "returning list of records: {:?}", records);

                NodeResponse::RecordsList { records }
            }
            NodeCommand::Stop => {
                debug!(target: "simulation::node", "stop command received");

                self.state.stop();
                self.state.kill();

                NodeResponse::Stopped
            }
        }
    }

    fn update_kademlia_info(&mut self) {
        // Get the closest peers
        let closest = self.base.kad_get_closest_local_peers(self.base.peer_id);

        debug!(target: "simulation::node", ?closest, "closest peers");

        self.base.kad_info.set_closest_peers(closest);

        // Get the non-empty kbuckets
        let buckets = self.base.kad_kbuckets();

        self.base.kad_info.set_bucket_info(buckets);
    }
}

//#[cfg(test)]
//mod tests {
//    #![allow(clippy::unwrap_used)]
//
//    use libp2p::kad::Mode;
//    use tokio::sync::{mpsc, oneshot};
//    use tokio_util::sync::CancellationToken;
//
//    use crate::{
//        messages::{NetworkEvent, NodeCommand, NodeResponse},
//        node::configured::ConfiguredNode,
//    };
//
//    #[test]
//    fn test_new_node() {
//        let cancellation_token = CancellationToken::new();
//        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
//        let (configured_node, _) = ConfiguredNode::new(cancellation_token, tx).unwrap();
//        let node = configured_node.start();
//
//        // Validate the new node is in the proper state
//        assert!(!node.state.bootstrapped());
//        assert!(!node.state.stopped());
//        assert_eq!(node.connection_tracker.connection_count(), 0);
//        assert_eq!(node.connection_tracker.known_peers_count(), 0);
//        assert_eq!(node.logger.total_recvd(), 0);
//        assert_eq!(node.logger.all_messages().len(), 0);
//
//        assert_eq!(
//            node.base.identify_info.public_key.to_peer_id(),
//            node.base.peer_id
//        );
//
//        assert!(!node.base.kad_info.bootstrapped);
//        assert_eq!(node.base.kad_info.mode, Mode::Client);
//    }
//
//    #[tokio::test]
//    async fn test_handle_get_kad_info() {
//        let cancellation_token = CancellationToken::new();
//        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
//        let (configured_node, command_tx) = ConfiguredNode::new(cancellation_token, tx).unwrap();
//        let mut node = configured_node.start();
//        let kad_info = node.base.kad_info.clone();
//
//        let _task = tokio::task::spawn(async move {
//            let _ = node.run().await;
//        });
//
//        let (tx, reply_rx) = oneshot::channel::<NodeResponse>();
//
//        command_tx
//            .send((NodeCommand::GetKademliaInfo, tx))
//            .await
//            .unwrap();
//
//        let response = reply_rx.await.unwrap();
//
//        assert_eq!(response, NodeResponse::KademliaInfo { info: kad_info });
//    }
//
//    #[tokio::test]
//    async fn test_handle_get_identify_info() {
//        let cancellation_token = CancellationToken::new();
//        let (tx, _) = mpsc::channel::<NetworkEvent>(1);
//        let (configured_node, command_tx) = ConfiguredNode::new(cancellation_token, tx).unwrap();
//        let mut node = configured_node.start();
//        let identify_info = node.base.identify_info.clone();
//
//        let _task = tokio::task::spawn(async move {
//            let _ = node.run().await;
//        });
//
//        let (tx, reply_rx) = oneshot::channel::<NodeResponse>();
//
//        command_tx
//            .send((NodeCommand::GetIdentifyInfo, tx))
//            .await
//            .unwrap();
//
//        let response = reply_rx.await.unwrap();
//
//        assert_eq!(
//            response,
//            NodeResponse::IdentifyInfo {
//                info: identify_info
//            }
//        );
//    }
//}
