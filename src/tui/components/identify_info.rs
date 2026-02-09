use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use crossterm::event::{KeyCode, KeyEvent};
use indexmap::IndexSet;
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListState, Padding, Paragraph, Widget, Wrap},
};
use tracing::debug;

use crate::tui::app::Action;

#[derive(Debug)]
pub struct IdentifyInfo {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// The length of the List
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    /// If the component is currenlty in focus in the TUI
    focus: bool,
}

impl IdentifyInfo {
    pub fn new() -> Self {
        Self {
            node: None,
            len: 2,
            list_state: ListState::default().with_selected(Some(0)),
            focus: false,
        }
    }

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

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();
                None
            }
            KeyCode::Down => {
                self.select_next();
                None
            }
            KeyCode::Enter => {
                let selection = self.clamp(self.list_state.selected().unwrap_or(0));

                debug!(target: "node_commands", "node commands option {} selected", selection);

                match selection {
                    0 => Some(Action::DisplayManageConnections {
                        peer_id: self.node.unwrap(),
                    }),
                    1 => Some(Action::StopNode {
                        peer_id: self.node.unwrap(),
                    }),
                    _ => None,
                }
            }
            KeyCode::Esc => Some(Action::CloseNodeCommands),
            _ => None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        Clear.render(area, frame.buffer_mut());

        let footer_text = Line::from(vec![
            Span::raw("<Esc> exit"),
            Span::raw(", "),
            Span::raw("<Up> <Down> select options"),
        ])
        .style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("Identify Commands")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        if self.node.is_none() {
            return;
        }

        let list_items = vec!["", "Remove Node (stop node)"];

        let list = List::new(list_items)
            .highlight_style(Style::new().reversed())
            .highlight_symbol("*")
            .block(block);

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    pub fn update(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::DisplayNodeCommands { peer_id } => {
                self.node = Some(peer_id);
                None
            }
            Action::CloseNodeCommands => None,
            _ => None,
        }
    }
}
