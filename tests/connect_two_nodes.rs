use std::time::Duration;

use libp2p::kad::Mode;
use p2p_protos::messages::{NetworkEvent, NodeCommand};
use tokio::sync::oneshot;

use crate::common::{spawn_test_node, test_bus, wait_for_event};

mod common;

#[tokio::test]
async fn test_connecting_two_nodes() {
    let event_bus = test_bus();
    let kad_mode = Mode::Client;
    let mut rx = event_bus.subscribe();

    let node_one = spawn_test_node(event_bus.clone(), kad_mode);
    let node_two = spawn_test_node(event_bus.clone(), kad_mode);

    wait_for_event(
        &mut rx,
        |e| matches!(e, NetworkEvent::NodeListening { peer_id, ..} if *peer_id == node_two.peer_id),
        Duration::from_secs(5),
    )
    .await;

    let (reply_tx, _) = oneshot::channel();

    node_one
        .cmd_tx
        .send((
            NodeCommand::ConnectTo {
                peer: node_two.listen_address,
            },
            reply_tx,
        ))
        .await
        .expect("failed to send connect nodes command");

    let event = wait_for_event(
        &mut rx,
        |e| matches!(e, NetworkEvent::NodesConnected { .. }),
        Duration::from_secs(5),
    )
    .await;

    let NetworkEvent::NodesConnected { peer_one, peer_two } = event else {
        panic!("expected nodes connected");
    };

    let pair = [peer_one, peer_two];
    assert!(pair.contains(&peer_one));
    assert!(pair.contains(&peer_two));

    node_one.cancel.cancel();
    node_two.cancel.cancel();
    let _ = node_one.task.await;
    let _ = node_two.task.await;
}
