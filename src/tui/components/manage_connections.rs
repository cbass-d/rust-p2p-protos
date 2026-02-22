use color_eyre::eyre::Result;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, RwLock},
};

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
use tracing::debug;

use crate::tui::{app::Action, components::popup::PopUpContent};

/// Component for managing the connections for a node in the network. Connections can be
/// established or deleted.
#[derive(Debug)]
pub struct ManageConnections {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// A IndexSet (a hashset that be accessed using []) of the actively
    /// running nodes that is used to build the list
    active_nodes: IndexSet<PeerId>,

    /// Hashmap representing the connections between the nodes
    node_connections: HashMap<PeerId, Arc<RwLock<HashSet<PeerId>>>>,

    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    /// If the component is currenlty in focus in the TUI
    focus: bool,
}

impl ManageConnections {
    /// Build a fresh ManageConnections component
    pub fn new() -> Self {
        Self {
            node: None,
            active_nodes: IndexSet::new(),
            node_connections: HashMap::default(),
            len: 0,
            focus: false,
            list_state: ListState::default(),
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    /// Set the focus field for the component
    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
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
        if idx >= self.len {
            self.len - 1
        } else if idx < 0 {
            0
        } else {
            idx
        }
    }

    /// Handle a key event comming from the TUI
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();
            }
            KeyCode::Down => {
                self.select_next();
            }
            KeyCode::Char('c') => {
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));

                debug!(target: "manage_connections", "connecting to peer: {}", self.active_nodes[node_idx]);

                if let Some(peer_one) = self.node {
                    let peer_two = self.active_nodes[node_idx];
                    actions.push_back(Action::ConnectTo { peer_one, peer_two });
                }
            }
            KeyCode::Char('d') => {
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));

                debug!(target: "manage_connections", "disconnecting from peer: {}", self.active_nodes[node_idx]);

                if let Some(peer_one) = self.node {
                    let peer_two = self.active_nodes[node_idx];
                    actions.push_back(Action::DisconnectFrom { peer_one, peer_two });
                }
            }
            // We return back to the node commands when pressing esc (exit)
            KeyCode::Esc => {
                actions.push_back(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id: self.node.unwrap(),
                });
            }
            _ => {}
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
            debug!(target: "manage_connections", "no node selected, not rendering");
            return;
        }

        let other_nodes = self.active_nodes.clone();

        if other_nodes.is_empty() {
            Paragraph::new("--- No other peers --- ")
                .block(block)
                .render(area, frame.buffer_mut());

            return;
        }

        let list_items = self.format_peer_list(other_nodes);

        let list = List::new(list_items)
            .highlight_style(Style::new().reversed())
            .block(block);

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Format the peer list to reflect active connections to be displayed
    fn format_peer_list(&self, peer_list: IndexSet<PeerId>) -> Vec<String> {
        if let Some(node) = self.node {
            let connected_to = self.node_connections.get(&node).unwrap().read().unwrap();
            peer_list
                .iter()
                .map(|p| {
                    if connected_to.contains(p) {
                        format!("[*] {}", p.to_string())
                    } else {
                        format!("[ ] {}", p.to_string())
                    }
                })
                .collect::<Vec<String>>()
        } else {
            vec![]
        }
    }

    pub fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::DisplayManageConnections { peer_id } => {
                self.node = Some(peer_id);
                self.active_nodes.swap_remove(&peer_id);
            }
            Action::CloseNodeCommands => {
                if let Some(node) = self.node {
                    self.active_nodes.insert(node);
                }
            }
            Action::AddNode {
                peer_id,
                node_connections,
            } => {
                self.active_nodes.insert(peer_id);
                self.len += 1;

                self.node_connections.insert(peer_id, node_connections);

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));
                }
            }
            Action::RemoveNode { peer_id } => {
                self.active_nodes.swap_remove(&peer_id);
                self.len -= 1;
            }
            _ => {}
        }
    }
}
