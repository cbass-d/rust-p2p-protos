use std::collections::VecDeque;

use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Widget, Wrap},
};

use crate::{
    node::info::KademliaInfo as NodeKademliaInfo,
    tui::{app::Action, components::popup::PopUpContent},
};

/// Component for displaying info related to the identify protocol
#[derive(Debug)]
pub struct KademliaInfo {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    /// The local kademlia info for the node
    info: Option<NodeKademliaInfo>,
}

impl KademliaInfo {
    /// Build a fresh IdentifyInfo component
    pub fn new() -> Self {
        Self {
            node: None,
            info: None,
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    /// Set the info for the component context
    pub fn set_info(&mut self, info: NodeKademliaInfo) {
        self.info = Some(info);
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
            let mut lines = Text::from(vec![
                Line::raw("Node Mode:").style(Style::new().underlined()),
                Line::from(format!("{}", info.mode)),
                Line::raw("Bootstrapped:").style(Style::new().underlined()),
                Line::from(format!("{}", info.bootstrapped)),
                Line::raw("Closest Local Peers:").style(Style::new().underlined()),
            ]);
            if info.closest_peers.is_empty() {
                lines.push_line(Line::from("None"));
            } else {
                info.closest_peers.iter().for_each(|p| {
                    lines.push_line(Line::from(format!("- {}", p)));
                });
            }
            lines.push_line(Line::raw("Buckets:").style(Style::new().underlined()));

            if info.bucket_info.is_empty() {
                lines.push_line(Line::from("None"));
            } else {
                info.bucket_info.iter().enumerate().for_each(|(i, kb)| {
                    lines.push_line(Line::from(format!(
                        "Bucket {i} - {} total entries",
                        kb.num_entries
                    )));
                });
            }

            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, frame.buffer_mut());
        }
    }

    /// Update IdentifyInfo with the provided Action component as needed
    pub fn update(&mut self, action: Action, _actions: &mut VecDeque<Action>) {
        match action {
            Action::DisplayNodeCommands { peer_id } => {
                self.node = Some(peer_id);
            }
            _ => {}
        }
    }
}
