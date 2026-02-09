use libp2p::{Multiaddr, identity::PublicKey};

#[derive(Clone, Debug)]
pub struct IdentifyInfo {
    pub public_key: PublicKey,
    pub protocol_version: String,
    pub agent_string: String,
    pub listen_addr: Multiaddr,
}

impl IdentifyInfo {
    pub fn new(
        public_key: PublicKey,
        protocol_version: String,
        agent_string: String,
        listen_addr: Multiaddr,
    ) -> Self {
        Self {
            public_key,
            protocol_version,
            agent_string,
            listen_addr,
        }
    }
}
