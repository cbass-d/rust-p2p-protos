use core::fmt;
use std::time::Duration;

use libp2p::{
    Multiaddr, PeerId,
    core::transport::ListenerId,
    identify::Info,
    kad::{Addresses, InboundRequest, Mode, QueryId},
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use tracing::debug;

pub(crate) enum LogMessage {
    Swarm { event: SwarmEventInfo, at: f32 },
    Kad { event: KadEventInfo, at: f32 },
    Identify { event: IdentifyEventInfo, at: f32 },
}

/// Wrappers for libp2p Swarm events for easier handling and formatting
#[derive(Debug, Clone)]
pub(crate) enum SwarmEventInfo {
    ConnectionEstablished {
        peer_id: PeerId,
    },
    ConnectionClosed {
        peer_id: PeerId,
    },
    IncomingConnection {
        peer_id: PeerId,
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

#[derive(Debug, Clone)]
pub(crate) enum KadEventInfo {
    InboundRequest {
        request: InboundRequest,
    },
    RoutingUpdated {
        peer: PeerId,
        is_new: bool,
        addresses: Addresses,
        old_peer: Option<PeerId>,
    },
    RoutablePeer {
        peer: PeerId,
        address: Multiaddr,
    },
    UnroutablePeer {
        peer: PeerId,
    },
    PendingRoutablePeer {
        peer: PeerId,
    },
    OutboundQueryProgressed {
        id: QueryId,
    },
    ModeChanged {
        new_mode: Mode,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum IdentifyEventInfo {
    Received { peer_id: PeerId, info: Info },
    Sent { peer_id: PeerId },
    Pushed { peer_id: PeerId, info: Info },
    Error { peer_id: PeerId },
}

/// Container for the swarm events a node has seen separated by protocol. Includes a timestamp
#[derive(Default, Debug, Clone)]
pub(crate) struct MessageHistory {
    pub identify: Vec<(IdentifyEventInfo, f32)>,
    pub kademlia: Vec<(KadEventInfo, f32)>,
    pub swarm: Vec<(SwarmEventInfo, f32)>,
}

fn format_duration(duration: Duration) -> f32 {
    duration.as_secs_f32()
}

impl MessageHistory {
    /// Adds a identify event, takes the event as String and the timestamp as f32
    pub fn add_identify_event(&mut self, event: IdentifyEventInfo, since_start: Duration) {
        let since_start = format_duration(since_start);
        self.identify.push((event, since_start));
    }

    /// Adds a kademlia event, takes the event as String and the timestamp as f32
    pub fn add_kademlia_event(&mut self, event: KadEventInfo, since_start: Duration) {
        let since_start = format_duration(since_start);
        self.kademlia.push((event, since_start));
    }

    /// Adds a swarm event, takes the event as String and the timestamp as f32
    pub fn add_swarm_event(&mut self, event: SwarmEventInfo, since_start: Duration) {
        let since_start = format_duration(since_start);
        self.swarm.push((event, since_start));
    }

    pub(crate) fn format_kad_message(&self, event: &KadEventInfo, time: f32) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                format!("{:#?}s", time),
                Style::new().add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw(" "),
            Span::styled(
                "KAD",
                Style::new()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(event.to_string()),
        ])
    }

    pub(crate) fn format_swarm_message(&self, event: &SwarmEventInfo, time: f32) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                format!("{:#?}s", time),
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
            Span::raw(event.to_string()),
        ])
    }

    pub(crate) fn format_identify_message(&self, event: &IdentifyEventInfo, time: f32) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                format!("{:#?}s", time),
                Style::new().add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw(" "),
            Span::styled(
                "IDENTIFY",
                Style::new()
                    .bg(Color::Green)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(event.to_string()),
        ])
    }

    /// Returns kademlia messages as strings
    pub fn kad_messages(&self) -> Vec<LogMessage> {
        self.kademlia
            .iter()
            .map(|(e, t)| LogMessage::Kad {
                event: e.to_owned(),
                at: t.to_owned(),
            })
            .collect()
    }

    pub fn identify_messages(&self) -> Vec<LogMessage> {
        self.identify
            .iter()
            .map(|(e, t)| LogMessage::Identify {
                event: e.to_owned(),
                at: t.to_owned(),
            })
            .collect()
    }

    /// Returns swarm messages as strings
    pub fn swarm_messages(&self) -> Vec<LogMessage> {
        self.swarm
            .iter()
            .map(|(e, t)| LogMessage::Swarm {
                event: e.to_owned(),
                at: t.to_owned(),
            })
            .collect()
    }

    /// Returns all messages as pretty and formatted ratatui Lines
    pub fn all_messages_formatted(&self) -> Vec<Line<'_>> {
        let mut messages = vec![];
        messages.append(&mut self.kad_messages());
        messages.append(&mut self.identify_messages());
        messages.append(&mut self.swarm_messages());

        messages.sort_by(|a, b| {
            let a_time = match a {
                LogMessage::Swarm { at, .. } => at,
                LogMessage::Kad { at, .. } => at,
                LogMessage::Identify { at, .. } => at,
            };
            let b_time = match b {
                LogMessage::Swarm { at, .. } => at,
                LogMessage::Kad { at, .. } => at,
                LogMessage::Identify { at, .. } => at,
            };

            a_time
                .partial_cmp(b_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        messages
            .iter()
            .map(|m| match m {
                LogMessage::Kad { event, at } => self.format_kad_message(event, *at),
                LogMessage::Swarm { event, at } => self.format_swarm_message(event, *at),
                LogMessage::Identify { event, at } => self.format_identify_message(event, *at),
            })
            .collect()
    }

    /// Returns all messages as strings
    pub fn all_messages(&self) -> Vec<LogMessage> {
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
            debug!(target: "simulation::node::history", "{:#?}s : {}", time, event);
            println!("{:#?}s : {}", time, event);
        }
    }

    /// Prints the kademlia messages to stdout
    pub fn display_kademlia_messages(&self) {
        for message in &self.kademlia {
            let (event, time) = message;
            debug!(target: "simulation::node::history", "{:#?}s : {}", time, event);
            println!("{:#?}s : {}", time, event);
        }
    }

    /// Prints the swarm messages to stdout
    pub fn display_swarm_messages(&self) {
        for message in &self.swarm {
            let (event, time) = message;
            debug!(target: "simulation::node::history", "{:#?}s : {}", time, event);
            println!("{:#?}s : {}", time, event);
        }
    }
}

impl fmt::Display for KadEventInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KadEventInfo::ModeChanged { new_mode } => {
                write!(f, "MODE Mode changed to {}", new_mode)
            }
            KadEventInfo::RoutablePeer { peer, address } => {
                write!(f, "ROUTABLE PEER Peer {} at {} now routable", peer, address)
            }
            KadEventInfo::InboundRequest { request } => {
                write!(f, "INBOUND REQUEST {:?}", request)
            }
            KadEventInfo::RoutingUpdated { peer, is_new, .. } => {
                if *is_new {
                    write!(f, "ROUTING UPDATED Routing for new peer {} updated", peer)
                } else {
                    write!(f, "ROUTING UPDATED Routing for peer {} updated", peer)
                }
            }
            KadEventInfo::UnroutablePeer { peer } => {
                write!(f, "UNROUTABLE Peer {} is unroutable", peer)
            }
            KadEventInfo::PendingRoutablePeer { peer } => {
                write!(
                    f,
                    "PENDING ROUTE Connection to peer {} established, pending addition to table",
                    peer
                )
            }
            KadEventInfo::OutboundQueryProgressed { id } => {
                write!(
                    f,
                    "OUTBOUND QUERY PROGRESSED Outbound query {} has progressed",
                    id
                )
            }
        }
    }
}

impl fmt::Display for IdentifyEventInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentifyEventInfo::Sent { peer_id } => {
                write!(f, "SENT Identification information to {}", peer_id)
            }
            IdentifyEventInfo::Pushed { peer_id, .. } => {
                write!(f, "PUSHED Identification information to {}", peer_id)
            }
            IdentifyEventInfo::Received { peer_id, .. } => {
                write!(f, "RECEIVED Identification information from {}", peer_id)
            }
            IdentifyEventInfo::Error { peer_id } => {
                write!(f, "ERROR Trying to identify {}", peer_id)
            }
        }
    }
}

impl fmt::Display for SwarmEventInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwarmEventInfo::Dialing { peer_id } => {
                write!(f, "DIALING Now dialing peer {}", peer_id.unwrap())
            }
            SwarmEventInfo::NewListenAddr {
                listener_id,
                address,
            } => {
                write!(
                    f,
                    "NEW LISTENER New listener {} with address {}",
                    listener_id, address
                )
            }
            SwarmEventInfo::ListenerClosed {
                listener_id,
                addresses,
            } => {
                write!(
                    f,
                    "LISTENER CLOSED The listener {} at addresses {:?} closed",
                    listener_id, addresses
                )
            }
            SwarmEventInfo::ConnectionClosed { peer_id } => {
                write!(
                    f,
                    "CONNECTION CLOSED The connection with {} closed",
                    peer_id
                )
            }
            SwarmEventInfo::IncomingConnection { peer_id } => {
                write!(f, "INCOMING CONNECTION New connection from {}", peer_id)
            }
            SwarmEventInfo::ConnectionEstablished { peer_id } => {
                write!(
                    f,
                    "CONNECTION ESTABLISHED Connection established with {}",
                    peer_id
                )
            }
        }
    }
}
