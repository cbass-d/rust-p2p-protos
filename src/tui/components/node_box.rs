use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, List, ListState, StatefulWidget, Widget},
};
use std::collections::HashSet;

use crate::tui::app::Action;

#[derive(Debug, Clone)]
pub struct NodeBox {
    active_nodes: HashSet<PeerId>,
    pub list_state: ListState,
}

impl NodeBox {
    pub fn new() -> Self {
        Self {
            active_nodes: HashSet::new(),
            list_state: ListState::default(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::new().title("Active Nodes").borders(Borders::ALL);

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

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();
            }
            KeyCode::Down => {
                self.select_next();
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

    pub fn update(&mut self, action: Action) {
        match action {
            Action::AddNode(peer) => {
                self.active_nodes.insert(peer);
            }
            Action::RemoveNode(peer) => {
                self.active_nodes.remove(&peer);
            }
            _ => {}
        }
    }
}
