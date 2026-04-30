use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Margin, Rect},
    style::{Color, Style},
    widgets::{
        Block, Borders, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Widget,
    },
};

use tracing::warn;

use crate::{
    node::NodeLogsHandle,
    tui::{
        action_queue::ActionQueue,
        app::Action,
        event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
    },
};

/// Scrollable log panel for the selected node.
#[derive(Debug, Clone)]
pub(crate) struct NodeLog {
    selected: Option<NodeLogsHandle>,
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

    pub fn update(&mut self, _action: &Action, _actions: &mut ActionQueue) {}

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        _actions: &mut ActionQueue,
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

    pub fn render(&mut self, frame: &mut Frame, area: Rect, _active_nodes: &IndexSet<PeerId>) {
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

        if let Some(selected) = &self.selected {
            let (messages, stats) = {
                let (messages_lock, stats_lock) = selected;

                (messages_lock.read(), stats_lock.read())
            };

            let block = block
                .title_bottom(format!("Total swarm events: {}", stats.total_recvd()))
                .title_alignment(Alignment::Center);

            let all_messages = messages.all_messages_formatted();

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
    }

    pub fn display_logs(&mut self, message_and_stats: NodeLogsHandle) {
        self.selected = Some(message_and_stats);
    }
}

impl TuiEventHandler for NodeLog {
    fn on_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        actions: &mut ActionQueue,
        _ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(key, actions) {
            warn!(target: "tui::node_log", error = %e, "key handler error");
        }
        KeyResult::Handled
    }

    fn on_action(
        &mut self,
        action: &Action,
        actions: &mut ActionQueue,
        _active: &IndexSet<PeerId>,
    ) {
        self.update(action, actions);
    }
}
