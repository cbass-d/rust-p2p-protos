use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Margin, Rect},
    style::{Color, Style},
    widgets::{
        Block, Borders, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Widget,
    },
};
use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use crate::{
    node::{NodeStats, history::MessageHistory},
    tui::app::Action,
};

#[derive(Debug, Clone)]
pub struct NodeLog {
    selected: Option<Arc<RwLock<(MessageHistory, NodeStats)>>>,
    pub list_state: ListState,
    pub scrollbar_state: ScrollbarState,

    len: usize,
    focus: bool,
}

impl NodeLog {
    pub fn new() -> Self {
        Self {
            selected: None,
            list_state: ListState::default(),
            scrollbar_state: ScrollbarState::default(),
            len: 0,
            focus: false,
        }
    }

    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    pub fn update(&mut self, _action: Action, _actions: &mut VecDeque<Action>) {}

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        _actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();
                self.scrollbar_state = self.scrollbar_state.position(
                    self.clap_scrollbar_pos(self.scrollbar_state.get_position().saturating_sub(1)),
                );
            }
            KeyCode::Down => {
                self.scrollbar_state = self
                    .scrollbar_state
                    .position(self.clap_scrollbar_pos(self.scrollbar_state.get_position() + 1));
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

    fn clap_scrollbar_pos(&self, new_pos: usize) -> usize {
        let curr_length = self.len;
        if new_pos > curr_length {
            curr_length
        } else {
            new_pos
        }
    }

    /// Moving up and down the listcan move past the bounds of the list,
    /// we must make sure it does not
    fn clamp(&mut self, idx: usize) -> usize {
        if idx >= self.len { self.len - 1 } else { idx }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if self.selected.is_none() {
            let block = if self.focus {
                Block::new()
                    .title("Node Log")
                    .borders(Borders::ALL)
                    .border_style(Color::LightRed)
            } else {
                Block::new().title("Node Log").borders(Borders::ALL)
            };
            Paragraph::new("-- No Node Selected -- ")
                .centered()
                .block(block)
                .render(area, frame.buffer_mut());

            return;
        }

        let block = if self.focus {
            Block::new()
                .title("Node Log")
                .borders(Borders::ALL)
                .border_style(Color::LightRed)
        } else {
            Block::new().title("Node Log").borders(Borders::ALL)
        };

        let selected = self.selected.clone().unwrap();

        let messages_and_stats = selected.read().unwrap();

        let messages = &messages_and_stats.0;
        let stats = &messages_and_stats.1;

        let block = block
            .title_bottom(format!("Total swarm events: {}", stats.recvd_count))
            .title_alignment(Alignment::Center);

        let all_messages = messages.all_messages_formmatted();

        self.len = all_messages.len();

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);

        self.scrollbar_state = self.scrollbar_state.content_length(self.len);
        self.scrollbar_state = self
            .scrollbar_state
            .viewport_content_length(area.height as usize);

        let list = List::new(all_messages)
            .block(block)
            .highlight_style(Style::new().underlined())
            .highlight_symbol("");

        frame.render_stateful_widget(list, area, &mut self.list_state);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut self.scrollbar_state,
        );
    }

    pub fn display_logs(&mut self, message_and_stats: Arc<RwLock<(MessageHistory, NodeStats)>>) {
        self.selected = Some(message_and_stats);
    }
}
