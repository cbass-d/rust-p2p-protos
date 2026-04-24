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
        LifecycleCommand::Start => {
            if ctx.nodes.len() >= ctx.max_nodes as usize {
                ctx.network_event_tx.publish(NetworkEvent::MaxNodes);
                return;
            }

            match ConfiguredNode::new(
                ctx.cancellation_token.clone(),
                ctx.network_event_tx.clone(),
                ctx.transport,
                ctx.bind_address,
            ) {
                Ok((node, tx)) => {
                    let peer_id = node.base.peer_id;
                    let listen_address = node.base.listen_address.clone();

                    ctx.nodes.insert(peer_id, tx);
                    ctx.addresses.insert(peer_id, listen_address);

                    task_set.spawn(async move {
                        let mut running_node = node.start();
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
