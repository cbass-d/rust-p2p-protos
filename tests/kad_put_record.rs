use std::time::Duration;

use libp2p::kad::Mode;
use p2p_protos::messages::{NetworkEvent, NodeCommand, NodeResponse};
use tokio::sync::oneshot;

use crate::common::{spawn_test_node, test_bus, wait_for_event};

mod common;

#[tokio::test]
async fn test_kad_put_record() {
    let event_bus = test_bus();
    let kad_mode = Mode::Server;
    let mut rx = event_bus.subscribe();

    let node_one = spawn_test_node(event_bus.clone(), kad_mode);

    wait_for_event(
        &mut rx,
        |e| matches!(e, NetworkEvent::NodeListening { peer_id, ..} if *peer_id == node_one.peer_id),
        Duration::from_secs(5),
    )
    .await;

    let (reply_tx, reply_rx) = oneshot::channel();

    let key = String::from("key");
    let value = String::from("value");

    node_one
        .cmd_tx
        .send((NodeCommand::PutRecord { key, value }, reply_tx))
        .await
        .expect("failed to send put record command");

    let response = tokio::time::timeout(Duration::from_secs(3), reply_rx)
        .await
        .expect("reply rx timedout")
        .expect("reply oneshot chanel closed");

    assert!(matches!(response, NodeResponse::RecordPlaced));

    node_one.cancel.cancel();
    let _ = node_one.task.await;
}
