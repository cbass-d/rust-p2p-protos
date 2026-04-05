use libp2p::multiaddr::Protocol;
use std::net::IpAddr;

pub(crate) fn extract_ip(addr: &libp2p::Multiaddr) -> Option<IpAddr> {
    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(ip) => return Some(IpAddr::V4(ip)),
            Protocol::Ip6(ip) => return Some(IpAddr::V6(ip)),
            _ => continue,
        }
    }
    None
}
