use libp2p::PeerId;

#[derive(Debug)]
pub enum NetworkEvent {
    NodeRunning(PeerId),
    NodeStopped(PeerId),
}

#[derive(Debug)]
pub enum NetworkCommand {
    StartNode(PeerId),
    StopNode(PeerId),
}
