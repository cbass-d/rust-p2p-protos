use std::time::Duration;

use libp2p::{Multiaddr, PeerId, kad::Mode};
use p2p_protos::{
    bus::EventBus,
    messages::{CommandChannel, NetworkEvent},
    network::TransportMode,
    node::{
        NodeError, NodeResult,
        configured::{self, ConfiguredNode},
    },
};
use tokio::{
    sync::{broadcast::Receiver, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

pub fn test_bus() -> EventBus<NetworkEvent> {
    EventBus::new(48)
}

pub struct TestNode {
    pub peer_id: PeerId,
    pub listen_address: Multiaddr,
    pub cmd_tx: mpsc::Sender<CommandChannel>,
    pub cancel: CancellationToken,
    pub task: JoinHandle<Result<NodeResult, NodeError>>,
}

pub fn spawn_test_node(event_bus: EventBus<NetworkEvent>, kad_mode: Mode) -> TestNode {
    let cancel = CancellationToken::new();
    let (configured, cmd_tx) = ConfiguredNode::new(
        cancel.clone(),
        event_bus,
        TransportMode::Memory,
        None,
        kad_mode,
    )
    .expect("failed to build configured test node");

    let peer_id = configured.base.peer_id;
    let listen_address = configured.base.listen_address.clone();

    let task = tokio::spawn(async move {
        let mut node = configured.build();
        node.run().await
    });

    TestNode {
        peer_id,
        listen_address,
        cmd_tx,
        cancel,
        task,
    }
}

pub async fn wait_for_event<F>(
    rx: &mut Receiver<NetworkEvent>,
    pred: F,
    timeout: Duration,
) -> NetworkEvent
where
    F: Fn(&NetworkEvent) -> bool,
{
    tokio::time::timeout(timeout, async {
        loop {
            match rx.recv().await {
                Ok(event) if pred(&event) => return event,
                Ok(_) => continue,
                Err(e) => panic!("event bus error: {e}"),
            }
        }
    })
    .await
    .expect("timedout waiting for event")
}
