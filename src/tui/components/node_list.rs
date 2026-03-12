use color_eyre::eyre::Result;
use indexmap::IndexSet;
use parking_lot::RwLock;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tracing::debug;

use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListState},
};

use crate::tui::{app::Action, components::popup::PopUpContent};

/// A display for the currently active nodes
/// Consists of a list that can be iterated through by
/// the user
#[derive(Debug, Clone)]
pub(crate) struct NodeList {
    /// Hashset containig the list of active nodes, shared by the App
    /// and other components
    active_nodes: Arc<RwLock<IndexSet<PeerId>>>,

    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    /// If the component is currenlty in focus in the TUI
    focus: bool,
}

impl NodeList {
    pub fn new(active_nodes: Arc<RwLock<IndexSet<PeerId>>>) -> Self {
        Self {
            active_nodes,
            list_state: ListState::default(),
            len: 0,
            focus: false,
        }
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let mut block = if self.focus {
            Block::new()
                .title("Nodes")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Color::LightRed)
        } else {
            Block::new().title("Nodes").borders(Borders::ALL)
        };

        let active_nodes = self.active_nodes.read();

        // Hashmap of idx in the list to the peer id
        let mut idx_to_peer = HashMap::new();
        let mut nodes = vec![];
        active_nodes.iter().enumerate().for_each(|(i, p)| {
            idx_to_peer.insert(i, p);
            nodes.push(p);
        });

        if active_nodes.len() < 10 {
            block = block.title_bottom("<a> Add to add node");
        } else {
            block = block.title_bottom("MAX NODES");
        }

        let list = List::new(nodes.iter().map(|p| p.to_string()).collect::<Vec<String>>())
            .highlight_style(Style::new().reversed())
            .highlight_symbol(">")
            .block(block);

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();

                // Get the index of the newly selected node
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                let active_nodes = self.active_nodes.read();
                actions.push_back(Action::DisplayLogs {
                    peer_id: active_nodes[node_idx],
                });
            }
            KeyCode::Down => {
                self.select_next();

                // Get the index of the newly selected node
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                let active_nodes = self.active_nodes.read();
                actions.push_back(Action::DisplayLogs {
                    peer_id: active_nodes[node_idx],
                });
            }
            KeyCode::Char('a') => {
                actions.push_back(Action::StartNode);
            }
            KeyCode::Enter => {
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                let active_nodes = self.active_nodes.read();
                actions.push_back(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id: active_nodes[node_idx],
                });
            }
            _ => {}
        }

        Ok(())
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

    pub fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::AddNode { peer_id, .. } => {
                self.len += 1;
                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));

                    debug!(target: "node_list", "display log action added");

                    let active_nodes = self.active_nodes.read();
                    actions.push_back(Action::DisplayLogs {
                        peer_id: active_nodes[0],
                    });

                    debug!(target: "node_list", "display log action added");
                }
            }
            Action::RemoveNode { peer_id } => {
                self.len -= 1;
            }
            _ => {}
        }
    }
}
