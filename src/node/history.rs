use libp2p::{
    Multiaddr, PeerId,
    core::{ConnectedPoint, transport::ListenerId},
    identify::Event as IdentifyEvent,
    kad::Event as KadEvent,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use tracing::debug;

/// Wrappers for libp2p Swarm events for easier handling and formatting
#[derive(Debug, Clone)]
pub enum SwarmEventInfo {
    ConnectionEstablished {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    ConnectionClosed {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    IncomingConnection {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
    },
    ListenerClosed {
        listener_id: ListenerId,
        addresses: Vec<Multiaddr>,
    },
    NewListenAddr {
        listener_id: ListenerId,
        address: Multiaddr,
    },
    Dialing {
        peer_id: Option<PeerId>,
    },
}

/// Container for the swarm events a node has seen separated by protocol. Includes a timestamp
#[derive(Default, Debug, Clone)]
pub struct MessageHistory {
    pub identify: Vec<(String, String)>,
    pub kademlia: Vec<(String, String)>,
    pub swarm: Vec<(SwarmEventInfo, String)>,
}

impl MessageHistory {
    /// Adds a identify event, takes the event as String and the timestamp as f32
    pub fn add_identify_event(&mut self, event: String, since_start: f32) {
        self.identify.push((event, format!("{:.5}", since_start)));
    }

    /// Adds a kademlia event, takes the event as String and the timestamp as f32
    pub fn add_kademlia_event(&mut self, event: String, since_start: f32) {
        self.kademlia.push((event, format!("{:.5}", since_start)));
    }

    /// Adds a swarm event, takes the event as String and the timestamp as f32
    pub fn add_swarm_event(&mut self, event: SwarmEventInfo, since_start: f32) {
        self.swarm.push((event, format!("{:.5}", since_start)));
    }

    pub fn identify_messages(&self) -> Vec<String> {
        self.identify
            .iter()
            .map(|(e, t)| format!("{}s : {:?}", t, e))
            .collect()
    }

    /// Returns the Identify messages as pretty and formatted ratatui Lines
    pub fn identify_messages_formatted(&self) -> Vec<Line<'_>> {
        self.identify
            .iter()
            .map(|(m, t)| {
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}s", t),
                        Style::new().add_modifier(Modifier::UNDERLINED),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        "Identify",
                        Style::new()
                            .bg(Color::Green)
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(m),
                ]);

                line
            })
            .collect()
    }

    /// Returns the Kademlia messages as pretty and formatted ratatui Lines
    pub fn kad_messages_formatted(&self) -> Vec<Line<'_>> {
        self.kademlia
            .iter()
            .map(|(m, t)| {
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}s", t),
                        Style::new().add_modifier(Modifier::UNDERLINED),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        "Kademlia",
                        Style::new()
                            .bg(Color::Blue)
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(m),
                ]);

                line
            })
            .collect()
    }

    /// Returns the Swarm messages as pretty and formatted ratatui Lines
    pub fn swarm_messages_formatted(&self) -> Vec<Line<'_>> {
        self.swarm
            .iter()
            .map(|(m, t)| {
                let line = Line::from(vec![
                    Span::styled(
                        format!("{}s", t),
                        Style::new().add_modifier(Modifier::UNDERLINED),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        "SWARM",
                        Style::new()
                            .bg(Color::Yellow)
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(format_swarm_event_to_string(m)),
                ]);

                line
            })
            .collect()
    }

    /// Returns kademlia messages as strings
    pub fn kad_messages(&self) -> Vec<String> {
        self.kademlia
            .iter()
            .map(|(e, t)| format!("{}s : {:?}", t, e))
            .collect()
    }

    /// Returns swarm messages as strings
    pub fn swarm_messages(&self) -> Vec<String> {
        self.swarm
            .iter()
            .map(|(e, t)| format!("{}s : {:?}", t, e))
            .collect()
    }

    /// Returns all messages as pretty and formatted ratatui Lines
    pub fn all_messages_formmatted(&self) -> Vec<Line<'_>> {
        let mut messages = vec![];
        messages.append(&mut self.identify_messages_formatted());
        messages.append(&mut self.kad_messages_formatted());
        messages.append(&mut self.swarm_messages_formatted());

        messages
    }

    /// Returns all messages as strings
    pub fn all_messages(&self) -> Vec<String> {
        let mut messages = vec![];
        messages.append(&mut self.identify_messages());
        messages.append(&mut self.kad_messages());
        messages.append(&mut self.swarm_messages());

        messages
    }

    /// Prints the identify messages to stdout
    pub fn display_identify_messages(&self) {
        for message in &self.identify {
            let (event, time) = message;
            debug!(target: "history", "{}s : {}", time, event);
            println!("{}s : {:?}", time, event);
        }
    }

    /// Prints the kademlia messages to stdout
    pub fn display_kademlia_messages(&self) {
        for message in &self.kademlia {
            let (event, time) = message;
            debug!(target: "history", "{}s : {}", time, event);
            println!("{}s : {:?}", time, event);
        }
    }

    /// Prints the swarm messages to stdout
    pub fn display_swarm_messages(&self) {
        for message in &self.swarm {
            let (event, time) = message;
            debug!(target: "history", "{}s : {:?}", time, event);
            println!("{}s : {:?}", time, event);
        }
    }
}

/// Formats the identify event to a readable string
pub fn identify_event_to_string(event: &IdentifyEvent) -> String {
    match event {
        IdentifyEvent::Sent {
            connection_id,
            peer_id,
        } => {
            format!(
                "SENT Identification information to {} ({})",
                peer_id, connection_id
            )
        }
        IdentifyEvent::Received {
            connection_id,
            peer_id,
            info,
        } => {
            format!(
                "RECIEVED Identification information from {} ({})",
                peer_id, connection_id
            )
        }
        IdentifyEvent::Pushed {
            connection_id,
            peer_id,
            info,
        } => {
            format!(
                "PUSHED Identification information to {} ({})",
                peer_id, connection_id
            )
        }
        IdentifyEvent::Error {
            connection_id,
            peer_id,
            error,
        } => {
            format!(
                "ERROR Trying to identify {}: {:?} ({})",
                peer_id, error, connection_id
            )
        }
    }
}

/// Formats the kademlia event to a readable string
pub fn kad_event_to_string(event: &KadEvent) -> String {
    match event {
        KadEvent::ModeChanged { new_mode } => {
            format!("MODE Mode changed to {}", new_mode)
        }
        KadEvent::RoutablePeer { peer, address } => {
            format!("ROUTABLE PEER Peer {} at {} now routable", peer, address)
        }
        KadEvent::InboundRequest { request } => {
            format!("INBOUND REQUEST {:?}", request)
        }
        KadEvent::RoutingUpdated {
            peer,
            is_new_peer,
            addresses,
            bucket_range,
            old_peer,
        } => {
            if *is_new_peer {
                format!("ROUTING UPDATED Routing for new peer {} updated", peer)
            } else {
                format!("ROUTING UPDATED Routing for peer {} updated", peer)
            }
        }
        KadEvent::UnroutablePeer { peer } => {
            format!("UNROUTABLE Peer {} is unroutable", peer)
        }
        KadEvent::PendingRoutablePeer { peer, address } => {
            format!(
                "PENIDNG ROUTE Connection to peer {} established, pednding addition to table",
                peer
            )
        }
        KadEvent::OutboundQueryProgressed {
            id,
            result,
            stats,
            step,
        } => {
            format!(
                "OUTBOUD QUERY PROGRESSES Outbound query {} has progressed",
                id
            )
        }
    }
}

/// Formats the swarm event to a readable string
pub fn format_swarm_event_to_string(event: &SwarmEventInfo) -> String {
    match event {
        SwarmEventInfo::Dialing { peer_id } => {
            format!("DIALING Now dialing peer {}", peer_id.unwrap())
        }
        SwarmEventInfo::NewListenAddr {
            listener_id,
            address,
        } => {
            format!(
                "NEW LISTENER New listener {} with address {}",
                listener_id, address
            )
        }
        SwarmEventInfo::ListenerClosed {
            listener_id,
            addresses,
        } => {
            format!(
                "LISTENER CLOSED The listener {} at addresses {:?} closed",
                listener_id, addresses
            )
        }
        SwarmEventInfo::ConnectionClosed { peer_id, endpoint } => {
            format!("CONNECTION CLOSED The connection with {} closed", peer_id)
        }
        SwarmEventInfo::IncomingConnection { peer_id, endpoint } => {
            format!("INCOMING CONNECTION New connection from {}", peer_id)
        }
        SwarmEventInfo::ConnectionEstablished { peer_id, endpoint } => {
            format!(
                "CONNECTION ESTABLISHED Connection established with {}",
                peer_id
            )
        }
    }
}
