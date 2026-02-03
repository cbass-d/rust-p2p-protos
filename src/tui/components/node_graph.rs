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
pub struct NodeGraph {
    active_nodes: HashSet<PeerId>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            active_nodes: HashSet::new(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::new().title("Node Graph").borders(Borders::ALL);

        let list = List::new(
            self.active_nodes
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<String>>(),
        )
        .highlight_style(Style::new().reversed())
        .highlight_symbol(">")
        .block(block);

        Widget::render(list, area, frame.buffer_mut());
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
