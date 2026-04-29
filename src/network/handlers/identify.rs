use crate::messages::{IdentifyCommand, NetworkEvent, NodeCommand, NodeResponse};

use super::{ControlCtx, request};

/// Handle Identify protocol commands.
pub(crate) async fn handle(cmd: IdentifyCommand, ctx: &mut ControlCtx<'_>) {
    match cmd {
        IdentifyCommand::GetInfo { peer_id } => {
            request(
                peer_id,
                NodeCommand::GetIdentifyInfo,
                ctx,
                |resp| match resp {
                    NodeResponse::IdentifyInfo { info } => {
                        Some(NetworkEvent::IdentifyInfo { info })
                    }
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

    use libp2p::{PeerId, identity::Keypair};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::{
        bus::EventBus,
        messages::{IdentifyCommand, NetworkEvent, NodeCommand, NodeResponse},
        network::{
            TransportMode,
            handlers::{ControlCtx, handle_identify},
        },
        node::info::IdentifyInfo,
    };

    #[tokio::test]
    async fn test_get_info_command() {
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

        let info = IdentifyInfo::new(
            Keypair::generate_ed25519().public(),
            "/test/1.0.0".to_string(),
            "test-agent".to_string(),
            "/memory/1234".parse().unwrap(),
        );

        let stub_node = async {
            let (cmd, reply_tx) = tokio::time::timeout(Duration::from_secs(3), cmd_rx.recv())
                .await
                .expect("cmd rcv timedout")
                .expect("cmd channel closed");

            let NodeCommand::GetIdentifyInfo = cmd else {
                panic!("expected DisconnectFrom node command");
            };

            reply_tx
                .send(NodeResponse::IdentifyInfo { info: info.clone() })
                .expect("failed to send info reply");
        };

        let handler = handle_identify(IdentifyCommand::GetInfo { peer_id: p1 }, &mut ctx);

        tokio::join!(handler, stub_node);

        let event = tokio::time::timeout(Duration::from_secs(3), event_rx.recv())
            .await
            .expect("event rx timedout")
            .expect("bus closed");

        let NetworkEvent::IdentifyInfo { info: received } = event else {
            panic!("node disconnected network event expected");
        };

        assert_eq!(info, received);
    }
}
