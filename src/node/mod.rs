mod behaviour;

use anyhow::{Result, anyhow};
use core::fmt;
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder, Transport,
    core::{
        muxing::StreamMuxerBox,
        transport::{ListenerId, MemoryTransport, upgrade},
    },
    identify::{self, Behaviour as Identify, Config as IdentifyConfig},
    identity,
    kad::{Behaviour as Kademlia, store::MemoryStore},
    multiaddr::Protocol,
    noise, swarm, yamux,
};
use rand::prelude::*;
use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, instrument, trace, warn};

use crate::{network::message::Message, node::behaviour::NodeBehaviour, utils::split_peer_id};

const IPFS_KAD_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/kad/1.0.0");
const IPFS_PROTO_NAME: StreamProtocol = StreamProtocol::new("/ipfs/id/1.0.0");

#[derive(Clone, Copy, Debug, Default)]
pub struct NodeStats {
    pub recvd_count: u64,
    pub sent_count: u64,
}

impl fmt::Display for NodeStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Packets recieved: {}\nPackets sent: {}\n",
            self.recvd_count, self.sent_count
        )
    }
}

// Node strucure representing a peer or participant in the network
pub struct Node {
    pub peer_id: PeerId,
    pub ip_address: Ipv4Addr,
    to_network: mpsc::Sender<Message>,
    from_network: mpsc::Receiver<Message>,
    kill_signal: broadcast::Receiver<()>,
    known_peers: Vec<Multiaddr>,

    // libp2p swarm listen address
    pub listen_address: Multiaddr,

    swarm: Swarm<NodeBehaviour>,

    pub stats: Arc<Mutex<NodeStats>>,
}

impl Node {
    // Constructs a new node with the provided addresss.
    // Also takes in the write end for an mpsc channel used to
    // send messages to the network, as well as the reciever for
    // the broadccast channel where the kill signal will be sent at
    // program shutdown
    //
    // Returns the constructed node as well as the write end of the mpsc
    // channel where the node will be recieving messages
    pub fn new(
        address: Ipv4Addr,
        to_network: mpsc::Sender<Message>,
        kill_signal: broadcast::Receiver<()>,
    ) -> Result<(Self, mpsc::Sender<Message>)> {
        // Build the mpsc channel where the channel will be recieving messages from
        let (tx, rx) = mpsc::channel(100);

        let node_keys = identity::Keypair::generate_ed25519();
        let peer_id = node_keys.public().to_peer_id();
        let node_transport = MemoryTransport::default()
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::Config::new(&node_keys)?)
            .multiplex(yamux::Config::default())
            .boxed();

        let listen_address = Multiaddr::from(Protocol::Memory(rand::random::<u64>()));

        let store = MemoryStore::new(peer_id);
        let kad = Kademlia::new(peer_id, store);
        let identify = {
            let cfg = IdentifyConfig::new(IPFS_PROTO_NAME.to_string(), node_keys.public());

            Identify::new(cfg)
        };
        let behaviour = NodeBehaviour { kad, identify };

        let swarm = Swarm::new(
            node_transport,
            behaviour,
            peer_id,
            swarm::Config::with_tokio_executor(),
        );

        let node = Node {
            peer_id,
            ip_address: address,
            to_network,
            from_network: rx,
            kill_signal,
            known_peers: vec![],
            swarm,
            listen_address,
            stats: Arc::new(Mutex::new(NodeStats::default())),
        };

        Ok((node, tx))
    }

    pub fn add_peer(&mut self, peer: Multiaddr) {
        self.known_peers.push(peer);
    }

    pub async fn send_to(&mut self, destination: Ipv4Addr, message: &[u8]) -> Result<()> {
        let message = Message::new(self.ip_address, destination, message);

        self.to_network.send(message).await?;
        self.stats.lock().unwrap().sent_count += 1;

        Ok(())
    }

    // Main run loop of for the node
    #[instrument(skip(self), fields(id = %self.peer_id), name = "run node")]
    pub async fn run(&mut self) -> Result<()> {
        info!(target: "node",
            "node {} now running with ipv4 addresss {}",
            self.peer_id, self.ip_address
        );

        self.swarm.listen_on(self.listen_address.clone())?;

        info!(target: "node", "node swarm listening on {}", self.listen_address);

        // Dial known peers passed by CLI and add to Kademlia routing table
        info!(target: "node", "dialing known peers");
        let mut dialed = 0;
        for addr in &self.known_peers {
            if self.swarm.dial(addr.clone()).is_ok() {
                debug!(target: "node", "successully dialed peer {addr}");
                dialed += 1
            } else {
                warn!(target: "node", "failed to dial peer {addr}");
            }
        }
        info!(target: "node", "dialed a total of {} peers", dialed);

        loop {
            tokio::select! {
                _ = self.kill_signal.recv() => {
                    break;
                },
                message = self.from_network.recv() => {
                    if message.is_none() {
                        continue;
                    }

                    let message = message.unwrap();

                    debug!(target: "node", "message from {}", message.source());

                    self.stats.lock().unwrap().recvd_count+= 1;
                },

                Some(event) = self.swarm.next() => {
                    trace!("node swarm event {:?}", event);
                },

            }
        }

        info!(target: "node", "node {} now shutting down", self.peer_id);

        Ok(())
    }

    pub fn get_stats(&self) -> NodeStats {
        *self.stats.lock().unwrap()
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node: {}", self.peer_id)
    }
}
