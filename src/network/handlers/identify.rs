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
