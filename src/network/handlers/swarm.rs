use crate::messages::{NetworkEvent, NodeCommand, NodeResponse, SwarmCommand};

use super::{ControlCtx, request};

/// Handle swarm-level commands (connect, disconnect).
pub(crate) async fn handle(cmd: SwarmCommand, ctx: &mut ControlCtx<'_>) {
    match cmd {
        SwarmCommand::Connect { peer_one, peer_two } => {
            let Some(peer_addr) = ctx.addresses.get(&peer_two).cloned() else {
                return;
            };
            // NodesConnected is published by the node on SwarmEvent::ConnectionEstablished,
            // not here — so we ignore the response.
            request(
                peer_one,
                NodeCommand::ConnectTo { peer: peer_addr },
                ctx,
                |_| None,
            )
            .await;
        }
        SwarmCommand::Disconnect { peer_one, peer_two } => {
            request(
                peer_one,
                NodeCommand::DisconnectFrom { peer: peer_two },
                ctx,
                move |resp| match resp {
                    NodeResponse::Disconnected { peer } => Some(NetworkEvent::NodesDisconnected {
                        peer_one,
                        peer_two: peer,
                    }),
                    _ => None,
                },
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use libp2p::{Multiaddr, PeerId};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::{
        bus::EventBus,
        messages::{NetworkEvent, NodeCommand, NodeResponse, SwarmCommand},
        network::{
            TransportMode,
            handlers::{ControlCtx, handle_swarm},
        },
    };

    #[tokio::test]
    async fn test_connect_command() {
        let peer_one = PeerId::random();
        let peer_two = PeerId::random();
        let peer_two_addr: Multiaddr = "/memory/1234".parse().expect("invalid multiaddr");

        let (cmd_tx, mut cmd_rx) = mpsc::channel(10);
        let mut nodes = HashMap::new();
        nodes.insert(peer_one, cmd_tx);

        let mut addresses = HashMap::new();
        addresses.insert(peer_two, peer_two_addr.clone());

        let bus = EventBus::<NetworkEvent>::new(1);
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

        let stub_node = async {
            let (cmd, _reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");
            cmd
        };

        let handler = handle_swarm(SwarmCommand::Connect { peer_one, peer_two }, &mut ctx);

        let ((), cmd) = tokio::join!(handler, stub_node);

        let NodeCommand::ConnectTo { peer } = cmd else {
            panic!("expected ConnectTo node command");
        };

        assert_eq!(peer, peer_two_addr);
    }

    #[tokio::test]
    async fn test_disconnect_network_event() {
        let p1 = PeerId::random();
        let p2 = PeerId::random();

        let (cmd_tx, mut cmd_rx) = mpsc::channel(10);
        let mut nodes = HashMap::new();
        nodes.insert(p1, cmd_tx);

        let mut addresses = HashMap::new();

        let bus = EventBus::<NetworkEvent>::new(1);
        let mut event_rx = bus.subscribe();
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

        let stub_node = async {
            let (cmd, reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");

            let NodeCommand::DisconnectFrom { peer } = cmd else {
                panic!("expected DisconnectFrom node command");
            };

            assert_eq!(peer, p2);

            reply_tx
                .send(NodeResponse::Disconnected { peer })
                .expect("oneshot reply send failed");
        };

        let handler = handle_swarm(
            SwarmCommand::Disconnect {
                peer_one: p1,
                peer_two: p2,
            },
            &mut ctx,
        );

        tokio::join!(handler, stub_node);

        let event = tokio::time::timeout(Duration::from_secs(3), event_rx.recv())
            .await
            .expect("event rx timedout")
            .expect("bus closed");

        let NetworkEvent::NodesDisconnected { peer_one, peer_two } = event else {
            panic!("nodes disconnected network event expected");
        };

        assert_eq!(p1, peer_one);
        assert_eq!(p2, peer_two);
    }
}
