use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListState},
};

use crate::tui::app::Action;

#[derive(Debug, Clone)]
pub struct NodeBox {
    active_nodes: IndexSet<PeerId>,
    len: usize,
    pub list_state: ListState,
    focus: bool,
}

impl NodeBox {
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
        let block = if self.focus {
            Block::new()
                .title("Node Graph")
                .borders(Borders::ALL)
                .border_style(Color::LightRed)
        } else {
            Block::new().title("Node Graph").borders(Borders::ALL)
        };

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

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();

                // Get the index of the newly selected node
                let node_idx = self.list_state.selected().unwrap_or(0);
                Some(Action::DisplayLogs(self.active_nodes[node_idx]))
            }
            KeyCode::Down => {
                self.select_next();

                // Get the index of the newly selected node
                let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
                Some(Action::DisplayLogs(self.active_nodes[node_idx]))
            }
            _ => None,
        }
    }

    pub fn select_next(&mut self) {
        self.list_state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.list_state.select_previous();
    }

    /// select_next() can move past the last item in the list so
    /// we must make sure it does not
    pub fn clamp(&mut self, idx: usize) -> usize {
        if idx >= self.len { self.len - 1 } else { idx }
    }

    pub fn update(&mut self, action: Action) {
        match action {
            Action::AddNode(peer) => {
                self.active_nodes.insert(peer);
                self.len += 1;
            }
            Action::RemoveNode(peer) => {
                self.active_nodes.swap_remove(&peer);
                self.len -= 1;
            }
            _ => {}
        }
    }
}
