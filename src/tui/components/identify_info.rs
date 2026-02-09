use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, RwLock},
};

use color_eyre::eyre::Result;
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

use crate::{
    node::info::IdentifyInfo as NodeIdentifyInfo,
    tui::{app::Action, components::popup::PopUpContent},
};

#[derive(Debug)]
pub struct IdentifyInfo {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// The length of the List
    len: usize,

    /// The state of the list (currently selected, next, etc.)
    pub list_state: ListState,

    info: Option<NodeIdentifyInfo>,

    /// If the component is currenlty in focus in the TUI
    focus: bool,
}

impl IdentifyInfo {
    pub fn new() -> Self {
        Self {
            node: None,
            len: 2,
            list_state: ListState::default().with_selected(Some(0)),
            info: None,
            focus: false,
        }
    }

    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    pub fn set_info(&mut self, info: NodeIdentifyInfo) {
        self.info = Some(info);
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

    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
            KeyCode::Up => {
                self.select_previous();
            }
            KeyCode::Down => {
                self.select_next();
            }
            KeyCode::Enter => {
                let selection = self.clamp(self.list_state.selected().unwrap_or(0));

                debug!(target: "node_commands", "node commands option {} selected", selection);
            }
            // We return back to the node commands when pressing esc (exit)
            KeyCode::Esc => {
                actions.push_back(Action::Popup {
                    content: PopUpContent::NodeInfo,
                    peer_id: self.node.unwrap(),
                });
            }
            _ => {}
        }

        Ok(())
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
            .title("Identify Info")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        if self.node.is_none() {
            return;
        }

        if self.info.is_none() {
            Paragraph::new("--- No info to display ---")
                .block(block)
                .alignment(Alignment::Center)
                .render(area, frame.buffer_mut());
        } else {
            let info = self.info.clone().unwrap();
            let lines = Text::from(vec![
                Line::from(format!("{:?}", info.public_key)),
                Line::from(format!("Protocol Version: {}", info.protocol_version)),
                Line::from(format!("Agent String: {}", info.agent_string)),
                Line::from(format!("Listen Address: {}", info.listen_addr.to_string())),
            ]);
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, frame.buffer_mut());
        }
    }

    pub fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::DisplayNodeCommands { peer_id } => {
                self.node = Some(peer_id);
            }
            _ => {}
        }
    }
}
