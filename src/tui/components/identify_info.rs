use std::collections::VecDeque;

use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, ListState, Padding, Paragraph, Widget, Wrap},
};

use crate::{
    node::info::IdentifyInfo as NodeIdentifyInfo,
    tui::{app::Action, components::popup::PopUpContent},
};

/// Component for displaying info related to the identify protocol
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
    /// Build a fresh IdentifyInfo component
    pub fn new() -> Self {
        Self {
            node: None,
            len: 2,
            list_state: ListState::default().with_selected(Some(0)),
            info: None,
            focus: false,
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    /// Set the info for the component context
    pub fn set_info(&mut self, info: NodeIdentifyInfo) {
        self.info = Some(info);
    }

    /// Set the focus field for the component
    pub fn focus(&mut self, focus: bool) {
        self.focus = focus;
    }

    /// Handle a key event comming from the TUI
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut VecDeque<Action>,
    ) -> Result<()> {
        match key_event.code {
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

    /// Render the IdentifyInfo component with the current context
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

        // Redner the info is present as a Paragraph
        if self.info.is_none() {
            Paragraph::new("--- No info to display ---")
                .block(block)
                .alignment(Alignment::Center)
                .render(area, frame.buffer_mut());
        } else {
            let info = self.info.clone().unwrap();
            let lines = Text::from(vec![
                Line::raw("Peer Id:").style(Style::new().underlined()),
                Line::from(format!("    {}", info.public_key.to_peer_id())),
                Line::raw("Protocol Version:").style(Style::new().underlined().bold()),
                Line::from(format!("    {}", info.protocol_version)),
                Line::raw("Agent String:").style(Style::new().underlined()),
                Line::from(format!("    {}", info.agent_string)),
                Line::raw("Listen Address:").style(Style::new().underlined()),
                Line::from(format!("    {}", info.listen_addr.to_string())),
            ]);
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, frame.buffer_mut());
        }
    }

    /// Update IdentifyInfo with the provided Action component as needed
    pub fn update(&mut self, action: Action, actions: &mut VecDeque<Action>) {
        match action {
            Action::DisplayNodeCommands { peer_id } => {
                self.node = Some(peer_id);
            }
            _ => {}
        }
    }
}
