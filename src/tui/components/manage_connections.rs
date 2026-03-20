use color_eyre::eyre::Result;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use tracing::{debug, trace};

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListState, Padding, Paragraph, Widget},
};

use crate::tui::{app::Action, components::popup::PopUpContent};

/// Component for managing the connections for a node in the network. Connections can be
/// established or deleted.
#[derive(Debug)]
pub(crate) struct ManageConnections {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// Hashset containig the list of active nodes, shared by the App
    /// and other components
    active_nodes: Arc<RwLock<IndexSet<PeerId>>>,

    /// Hashmap representing the connections between the nodes
    node_connections: HashMap<PeerId, Arc<RwLock<HashSet<PeerId>>>>,

    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,
}

impl ManageConnections {
    /// Build a fresh `ManageConnections` component
    pub fn new(active_nodes: Arc<RwLock<IndexSet<PeerId>>>) -> Self {
        Self {
            node: None,
            active_nodes,
            node_connections: HashMap::default(),
            len: 0,
            list_state: ListState::default(),
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    pub fn select_next(&mut self) {
        self.list_state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.list_state.select_previous();
    }

    /// Moving up and down the listcan move past the bounds of the list,
    /// we must make sure it does not
    pub fn clamp(&mut self, idx: usize) -> usize {
        if idx >= self.len { self.len - 1 } else { idx }
    }

    /// Handle a key event comming from the TUI
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        if let Some(peer_id) = self.node {
            match key_event.code {
                KeyCode::Up => {
                    self.select_previous();
                }
                KeyCode::Down => {
                    self.select_next();
                }
                KeyCode::Char('c') => {
                    let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                    let active_nodes = self.active_nodes.read();

                    debug!(target: "app::manage_connections", "connecting to peer: {}", active_nodes[node_idx]);

                    actions.push_back(Action::ConnectTo {
                        peer_one: peer_id,
                        peer_two: active_nodes[node_idx],
                    });
                }
                KeyCode::Char('d') => {
                    let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                    let active_nodes = self.active_nodes.read();

                    debug!(target: "app::manage_connections", "disconnecting from peer: {}", active_nodes[node_idx]);

                    let peer_two = active_nodes[node_idx];
                    if peer_two != peer_id {
                        actions.push_back(Action::DisconnectFrom {
                            peer_one: peer_id,
                            peer_two,
                        });
                    }
                }
                // We return back to the node commands when pressing esc (exit)
                KeyCode::Esc => {
                    actions.push_back(Action::Popup {
                        content: PopUpContent::NodeCommands,
                        peer_id,
                    });
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        Clear.render(area, frame.buffer_mut());

        let footer_text = Line::from(vec![
            Span::raw("<Esc> exit"),
            Span::raw(", "),
            Span::raw("<Up> <Down> select peer"),
            Span::raw(", "),
            Span::raw("<c> connect to peer"),
            Span::raw(", "),
            Span::raw("<d> disconnect from peer"),
        ])
        .style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("Manage Connections")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        if self.node.is_none() {
            debug!(target: "app::manage_connections", "no node selected, not rendering");
            return;
        }

        let active_nodes = self.active_nodes.read();
        let other_nodes: Vec<&PeerId> = active_nodes.iter().collect();

        trace!(target: "app::manage_connections", "rendering with peer list: {0:#?}", other_nodes);

        if other_nodes.is_empty() {
            Paragraph::new("--- No other peers --- ")
                .block(block)
                .render(area, frame.buffer_mut());

            return;
        }

        let list_items = self.format_peer_list(&other_nodes);

        let list = List::new(list_items)
            .highlight_style(Style::new().reversed())
            .block(block);

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Format the peer list to reflect active connections to be displayed
    fn format_peer_list(&self, peer_list: &[&PeerId]) -> Vec<String> {
        if let Some(node) = self.node {
            self.node_connections
                .get(&node)
                .map(|connections| {
                    let connections = connections.read();
                    peer_list
                        .iter()
                        .map(|p| {
                            if **p == node {
                                format!("(current node) -> {p}")
                            } else if connections.contains(p) {
                                format!("[*] {p}")
                            } else {
                                format!("[ ] {p}")
                            }
                        })
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default()
        } else {
            vec![]
        }
    }

    fn remove_peer_from_connections(&mut self, peer_id: &PeerId) {
        self.node_connections
            .iter_mut()
            .for_each(|(_, connections)| {
                let mut connections = connections.write();

                connections.remove(peer_id);
            });

        self.node_connections.remove(peer_id);
    }

    pub fn update(&mut self, action: Action, _actions: &mut VecDeque<Action>) {
        match action {
            Action::DisplayManageConnections { peer_id } => {
                self.node = Some(peer_id);
            }
            Action::CloseNodeCommands => {}
            Action::AddNode {
                peer_id,
                node_connections,
            } => {
                self.len += 1;

                self.node_connections.insert(peer_id, node_connections);

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));
                }
            }
            Action::RemoveNode { peer_id } => {
                debug!(target: "app::manage_connections", "Removing peer {0} from peer list", peer_id);

                debug!(target: "app::manage_connections", "new peer list: {0:#?}", self.active_nodes);
                self.remove_peer_from_connections(&peer_id);
                self.len -= 1;
            }
            _ => {}
        }
    }
}
