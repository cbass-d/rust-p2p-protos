use libp2p::{Multiaddr, PeerId, multiaddr::Protocol};

// Takes in a libp2p Multiaddr and returns an tuple containing
// the peer ID and the original Multiaddr
pub fn split_peer_id(addr: Multiaddr) -> Option<(PeerId, Multiaddr)> {
    let mut base_addr = Multiaddr::empty();
    let mut peer_id = None;

    for component in addr.into_iter() {
        if let Protocol::P2p(id) = component {
            peer_id = Some(id);
            break;
        } else {
            base_addr.push(component);
        }
    }

    peer_id.map(|id| (id, base_addr))
}

#[cfg(test)]
mod tests {
    use libp2p::{Multiaddr, PeerId};

    use crate::utils::split_peer_id;

    #[test]
    fn test_split_peer_id_with_peer() {
        let peer_id = PeerId::random();
        let addr: Multiaddr = format!("/ip4/127.0.0.1/tcp/8080/p2p/{}", peer_id)
            .parse()
            .unwrap();

        let result = split_peer_id(addr.clone());
        assert!(result.is_some());

        let (extracted_peer, base) = result.unwrap();
        assert_eq!(extracted_peer, peer_id);
        assert_eq!(
            base,
            "/ip4/127.0.0.1/tcp/8080".parse::<Multiaddr>().unwrap()
        );
    }

    #[test]
    fn test_split_peer_id_without_peer() {
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();

        let result = split_peer_id(addr);
        assert!(result.is_none());
    }
}
