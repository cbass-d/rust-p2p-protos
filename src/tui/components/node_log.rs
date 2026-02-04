use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, List, ListState, Paragraph, StatefulWidget, Widget},
};
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
};
use tracing::debug;

use crate::{node::history::MessageHistory, tui::app::Action};

#[derive(Debug, Clone)]
pub struct NodeLog {
    active_nodes: HashSet<PeerId>,
    selected: Option<Arc<RwLock<MessageHistory>>>,
    focus: bool,
}

impl NodeLog {
    pub fn new() -> Self {
        Self {
            active_nodes: HashSet::new(),
            selected: None,
            focus: false,
        }
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let block = if self.focus {
            Block::new()
                .title("Node Log")
                .borders(Borders::ALL)
                .border_style(Color::LightRed)
        } else {
            Block::new().title("Node Log").borders(Borders::ALL)
        };

        if self.selected.is_none() {
            Paragraph::new("-- No Node Selected -- ")
                .centered()
                .block(block)
                .render(area, frame.buffer_mut());

            return;
        }

        let selected = self.selected.clone().unwrap();

        let message_history = selected.read().unwrap();

        let all_messages = message_history.all_messages();

        let list = List::new(all_messages).block(block);

        Widget::render(list, area, frame.buffer_mut());
    }

    pub fn display_logs(&mut self, message_history: Arc<RwLock<MessageHistory>>) {
        self.selected = Some(message_history);
    }
}
