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
