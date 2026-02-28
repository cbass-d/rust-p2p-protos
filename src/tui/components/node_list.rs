use color_eyre::eyre::Result;
use std::collections::VecDeque;
use tracing::debug;

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
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
pub struct NodeList {
    /// A IndexSet (a hashset that be accessed using []) of the actively
    /// running nodes that is used to build the list
    active_nodes: IndexSet<PeerId>,

    /// The length of the current list of active nodes
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    /// If the component is currenlty in focus in the TUI
    focus: bool,
}

impl NodeList {
    pub fn new() -> Self {
        Self {
            active_nodes: IndexSet::new(),
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

        if self.active_nodes.len() < 10 {
            block = block.title_bottom("<a> Add to add node");
        } else {
            block = block.title_bottom("MAX NODES");
        }

        let list = List::new(
            self.active_nodes
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<String>>(),
        )
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
                actions.push_back(Action::DisplayLogs {
                    peer_id: self.active_nodes[node_idx],
                });
            }
            KeyCode::Down => {
                self.select_next();

                // Get the index of the newly selected node
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                actions.push_back(Action::DisplayLogs {
                    peer_id: self.active_nodes[node_idx],
                });
            }
            KeyCode::Char('a') => {
                actions.push_back(Action::StartNode);
            }
            KeyCode::Enter => {
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                actions.push_back(Action::Popup {
                    content: PopUpContent::NodeCommands,
                    peer_id: self.active_nodes[node_idx],
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
                self.active_nodes.insert(peer_id);
                self.len += 1;

                // Auto select the first node we add
                if self.list_state.selected().is_none() {
                    self.list_state = self.list_state.with_selected(Some(0));

                    debug!(target: "node_list", "display log action added");

                    actions.push_back(Action::DisplayLogs {
                        peer_id: self.active_nodes[0],
                    });

                    debug!(target: "node_list", "display log action added");
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
