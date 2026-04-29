use crate::messages::{KademliaCommand, NetworkEvent, NodeCommand, NodeResponse};

use super::{ControlCtx, request};

/// Handle Kademlia protocol commands.
pub(crate) async fn handle(cmd: KademliaCommand, ctx: &mut ControlCtx<'_>) {
    match cmd {
        KademliaCommand::GetInfo { peer_id } => {
            request(
                peer_id,
                NodeCommand::GetKademliaInfo,
                ctx,
                |resp| match resp {
                    NodeResponse::KademliaInfo { info } => {
                        Some(NetworkEvent::KademliaInfo { info })
                    }
                    _ => None,
                },
            )
            .await;
        }
        KademliaCommand::PutRecord {
            peer_id,
            key,
            value,
        } => {
            request(peer_id, NodeCommand::PutRecord { key, value }, ctx, |_| {
                None
            })
            .await;
        }
        KademliaCommand::RecordsList { peer_id } => {
            request(peer_id, NodeCommand::ListRecords, ctx, |resp| match resp {
                NodeResponse::RecordsList { records } => {
                    Some(NetworkEvent::RecordsList { records })
                }
                _ => None,
            })
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use libp2p::{PeerId, kad::Mode};
    use tokio::sync::{broadcast::error::TryRecvError, mpsc};
    use tokio_util::sync::CancellationToken;

    use crate::{
        bus::EventBus,
        messages::{KademliaCommand, NetworkEvent, NodeCommand, NodeResponse},
        network::{
            TransportMode,
            handlers::{ControlCtx, handle_kademlia},
        },
        node::info::KademliaInfo,
    };

    #[tokio::test]
    async fn test_get_kad_info_command() {
        let p1 = PeerId::random();

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

        let info = KademliaInfo::new(Mode::Server, false);

        let stub_node = async {
            let (cmd, reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");

            let NodeCommand::GetKademliaInfo = cmd else {
                panic!("expected DisconnectFrom node command");
            };

            reply_tx
                .send(NodeResponse::KademliaInfo { info: info.clone() })
                .expect("failed to send info reply");
        };

        let handler = handle_kademlia(KademliaCommand::GetInfo { peer_id: p1 }, &mut ctx);

        tokio::join!(handler, stub_node);

        let event = tokio::time::timeout(Duration::from_secs(3), event_rx.recv())
            .await
            .expect("event rx timedout")
            .expect("bus closed");

        let NetworkEvent::KademliaInfo { info: received } = event else {
            panic!("node disconnected network event expected");
        };

        assert_eq!(info, received);
    }

    #[tokio::test]
    async fn test_put_record_command() {
        let p1 = PeerId::random();

        let (cmd_tx, mut cmd_rx) = mpsc::channel(10);
        let mut nodes = HashMap::new();
        nodes.insert(p1, cmd_tx);

        let mut addresses = HashMap::new();

        let bus = EventBus::<NetworkEvent>::new(1);
        let mut rx = bus.subscribe();
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

        let sent_key = String::from("key");
        let sent_value = String::from("key_value");

        let handler = handle_kademlia(
            KademliaCommand::PutRecord {
                peer_id: p1,
                key: sent_key.clone(),
                value: sent_value.clone(),
            },
            &mut ctx,
        );

        let stub_node = async {
            let (cmd, reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");

            let NodeCommand::PutRecord { key, value } = cmd else {
                panic!("expect put record command");
            };

            assert_eq!(sent_key, key);
            assert_eq!(sent_value, value);

            reply_tx
                .send(NodeResponse::RecordPlaced)
                .expect("failed to send node response");
        };

        tokio::join!(handler, stub_node);

        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn test_records_list_command() {
        let p1 = PeerId::random();

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

        let handler = handle_kademlia(KademliaCommand::RecordsList { peer_id: p1 }, &mut ctx);

        let expected = vec![("key".to_string(), "value".to_string())];

        let stub_node = async {
            let (cmd, reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");

            let NodeCommand::ListRecords = cmd else {
                panic!("expected records list command");
            };

            reply_tx
                .send(NodeResponse::RecordsList {
                    records: expected.clone(),
                })
                .expect("failed to send node response");
        };

        tokio::join!(handler, stub_node);

        let event = tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
            .await
            .expect("event rx timedout")
            .expect("event rx channel closed");

        let NetworkEvent::RecordsList { records } = event else {
            panic!("expected records list network event");
        };

        assert_eq!(records, expected);
    }
}
