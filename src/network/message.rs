use std::net::Ipv4Addr;

use libp2p::Multiaddr;

pub enum NodeCommand {
    AddPeer(Multiaddr),
}
