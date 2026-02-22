use libp2p::{
    Multiaddr, PeerId,
    identity::PublicKey,
    kad::{Mode, U256},
};

/// Local identify info for a node
#[derive(Clone, Debug)]
pub struct IdentifyInfo {
    pub public_key: PublicKey,
    pub protocol_version: String,
    pub agent_string: String,
    pub listen_addr: Multiaddr,
}

impl IdentifyInfo {
    /// Build a new strucutre holding a node's local identify info
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

/// Struct to hold info about a kbucket
#[derive(Clone, Debug)]
pub struct KBucketInfo {
    pub range: (U256, U256),
    pub num_entries: usize,
}

/// Local identify info for a node
#[derive(Clone, Debug)]
pub struct KademliaInfo {
    pub mode: Mode,
    pub bootstrapped: bool,
    pub closest_peers: Vec<PeerId>,
    pub bucket_info: Vec<KBucketInfo>,
}

impl KademliaInfo {
    /// Build a new strucutre holding a node's local identify info
    pub fn new(mode: Mode, bootstrapped: bool) -> Self {
        Self {
            mode,
            bootstrapped,
            closest_peers: Vec::new(),
            bucket_info: Vec::new(),
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn set_closest_peers(&mut self, peers: Vec<PeerId>) {
        self.closest_peers = peers;
    }

    pub fn set_bucket_info(&mut self, bucket_info: Vec<KBucketInfo>) {
        self.bucket_info = bucket_info;
    }

    pub fn set_bootstrapped(&mut self, bootstrapped: bool) {
        self.bootstrapped = bootstrapped;
    }
}
