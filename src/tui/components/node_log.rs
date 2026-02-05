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
    pub list_state: ListState,

    len: usize,
    focus: bool,
}

impl NodeLog {
    pub fn new() -> Self {
        Self {
            active_nodes: HashSet::new(),
            selected: None,
            list_state: ListState::default(),
            len: 0,
            focus: false,
        }
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn update(&mut self, action: Action) -> Option<Action> {
        None
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();
            }
            KeyCode::Down => {
                self.select_next();
            }
            _ => {}
        }

        None
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

        let all_messages = message_history.all_messages_formmatted();

        self.len = all_messages.len();

        let list = List::new(all_messages)
            .block(block)
            .highlight_style(Style::new().reversed())
            .highlight_symbol("");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    pub fn display_logs(&mut self, message_history: Arc<RwLock<MessageHistory>>) {
        self.selected = Some(message_history);
    }
}
