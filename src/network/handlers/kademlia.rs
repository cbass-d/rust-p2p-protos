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
    }
}
