use tokio::task::JoinSet;
use tracing::{debug, warn};

use crate::{
    messages::{LifecycleCommand, NetworkEvent, NodeCommand},
    node::{NodeError, NodeResult, configured::ConfiguredNode},
};

use super::{ControlCtx, request};

/// Start, stop, or register external nodes in the network.
pub(crate) async fn handle(
    cmd: LifecycleCommand,
    ctx: &mut ControlCtx<'_>,
    task_set: &mut JoinSet<Result<NodeResult, NodeError>>,
) {
    match cmd {
        LifecycleCommand::Start { kad_mode } => {
            if ctx.nodes.len() >= ctx.max_nodes as usize {
                ctx.network_event_tx.publish(NetworkEvent::MaxNodes);
                return;
            }

            match ConfiguredNode::new(
                ctx.cancellation_token.clone(),
                ctx.network_event_tx.clone(),
                ctx.transport,
                ctx.bind_address,
                kad_mode,
            ) {
                Ok((node, tx)) => {
                    let peer_id = node.base.peer_id;
                    let listen_address = node.base.listen_address.clone();

                    ctx.nodes.insert(peer_id, tx);
                    ctx.addresses.insert(peer_id, listen_address);

                    task_set.spawn(async move {
                        let mut running_node = node.build();
                        running_node.run().await
                    });

                    debug!(target: "simulation::network", %peer_id, "new node task spawned");
                }
                Err(e) => {
                    warn!(target: "simulation::network", error = %e, "failed to build node");
                }
            }
        }
        LifecycleCommand::Stop { peer_id } => {
            request(peer_id, NodeCommand::Stop, ctx, |_| None).await;
            // Clean up the local address book regardless of response — if the
            // node really stopped, its command channel is now useless anyway.
            ctx.nodes.remove(&peer_id);
            ctx.addresses.remove(&peer_id);
        }
        LifecycleCommand::AddExternal { peer_id, address } => {
            ctx.addresses.insert(peer_id, address);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use libp2p::{Multiaddr, PeerId, kad::Mode};
    use tokio::{sync::mpsc, task::JoinSet};
    use tokio_util::sync::CancellationToken;

    use crate::{
        bus::EventBus,
        messages::{LifecycleCommand, NetworkEvent, NodeCommand},
        network::{
            TransportMode,
            handlers::{ControlCtx, handle_lifecycle},
        },
        node::{NodeError, NodeResult},
    };

    #[tokio::test]
    async fn test_start_node_command() {
        let mut addresses = HashMap::new();
        let mut nodes = HashMap::new();
        let cancel = CancellationToken::new();

        let bus = EventBus::<NetworkEvent>::new(10);
        let mut event_rx = bus.subscribe();

        let mut ctx = ControlCtx {
            nodes: &mut nodes,
            addresses: &mut addresses,
            network_event_tx: &bus,
            max_nodes: 10,
            transport: TransportMode::Memory,
            bind_address: None,
            cancellation_token: &cancel,
        };

        let mut task_set: JoinSet<Result<NodeResult, NodeError>> = JoinSet::new();
        let kad_mode = Mode::Server;

        let stub_subscriber = async {
            let event = tokio::time::timeout(Duration::from_secs(3), event_rx.recv())
                .await
                .expect("event recv timedout")
                .expect("event bus closed");

            event
        };

        let handler = handle_lifecycle(
            LifecycleCommand::Start { kad_mode },
            &mut ctx,
            &mut task_set,
        );

        let ((), event) = tokio::join!(handler, stub_subscriber);

        let NetworkEvent::NodeRunning { peer_id, mode, .. } = event else {
            panic!("expected node running event");
        };

        assert!(nodes.contains_key(&peer_id));
        assert!(addresses.contains_key(&peer_id));
        assert_eq!(mode, kad_mode);
    }

    #[tokio::test]
    async fn test_max_nodes() {
        let mut addresses = HashMap::new();
        let mut nodes = HashMap::new();
        nodes.insert(PeerId::random(), mpsc::channel(1).0);
        let cancel = CancellationToken::new();

        let bus = EventBus::<NetworkEvent>::new(10);
        let mut event_rx = bus.subscribe();

        let mut ctx = ControlCtx {
            nodes: &mut nodes,
            addresses: &mut addresses,
            network_event_tx: &bus,
            max_nodes: 1,
            transport: TransportMode::Memory,
            bind_address: None,
            cancellation_token: &cancel,
        };

        let mut task_set: JoinSet<Result<NodeResult, NodeError>> = JoinSet::new();
        let kad_mode = Mode::Server;

        let stub_subscriber = async {
            let event = tokio::time::timeout(Duration::from_secs(3), event_rx.recv())
                .await
                .expect("event recv timedout")
                .expect("event bus closed");

            event
        };

        let handler = handle_lifecycle(
            LifecycleCommand::Start { kad_mode },
            &mut ctx,
            &mut task_set,
        );

        let ((), event) = tokio::join!(handler, stub_subscriber);

        assert!(matches!(event, NetworkEvent::MaxNodes));
        assert_eq!(nodes.len(), 1)
    }

    #[tokio::test]
    async fn test_stop_node_command() {
        let peer_one = PeerId::random();
        let peer_one_addr: Multiaddr = "/memory/1234".parse().expect("invalid multiaddr");

        let (cmd_tx, mut cmd_rx) = mpsc::channel(10);
        let mut nodes = HashMap::new();
        nodes.insert(peer_one, cmd_tx);

        let mut addresses = HashMap::new();
        addresses.insert(peer_one, peer_one_addr.clone());

        let bus = EventBus::<NetworkEvent>::new(1);
        let cancel = CancellationToken::new();

        let mut task_set: JoinSet<Result<NodeResult, NodeError>> = JoinSet::new();

        let mut ctx = ControlCtx {
            nodes: &mut nodes,
            addresses: &mut addresses,
            network_event_tx: &bus,
            max_nodes: 10,
            transport: TransportMode::Memory,
            bind_address: None,
            cancellation_token: &cancel,
        };

        let stub_node = async {
            let (cmd, _reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");
            cmd
        };

        let handler = handle_lifecycle(
            LifecycleCommand::Stop {
                peer_id: peer_one.clone(),
            },
            &mut ctx,
            &mut task_set,
        );

        let ((), cmd) = tokio::join!(handler, stub_node);

        let NodeCommand::Stop = cmd else {
            panic!("expected Stop node command");
        };

        assert!(!nodes.contains_key(&peer_one));
        assert!(!addresses.contains_key(&peer_one));
    }

    #[tokio::test]
    async fn test_add_external_node() {
        let mut nodes = HashMap::new();
        let mut addresses = HashMap::new();
        let bus = EventBus::new(10);
        let cancel = CancellationToken::new();

        let mut ctx = ControlCtx {
            nodes: &mut nodes,
            addresses: &mut addresses,
            network_event_tx: &bus,
            max_nodes: 10,
            transport: TransportMode::Memory,
            bind_address: None,
            cancellation_token: &cancel,
        };

        let peer_id = PeerId::random();
        let address: Multiaddr = "/memory/1234".parse().expect("invalid multiaddr");

        let mut task_set: JoinSet<Result<NodeResult, NodeError>> = JoinSet::new();

        handle_lifecycle(
            LifecycleCommand::AddExternal {
                peer_id: peer_id,
                address: address.clone(),
            },
            &mut ctx,
            &mut task_set,
        )
        .await;

        assert!(addresses.contains_key(&peer_id));
        assert_eq!(addresses.get(&peer_id), Some(&address));
        assert!(nodes.is_empty());
    }
}
