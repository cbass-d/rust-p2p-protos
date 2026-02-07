use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    buffer::Buffer,
    crossterm::ExecutableCommand,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::tui::app::Action;

#[derive(Debug)]
pub struct NodeCommands {
    node: Option<PeerId>,
}

impl NodeCommands {
    pub fn new() -> Self {
        Self { node: None }
    }

    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<Action> {
        match key_event.code {
            //KeyCode::Up => {
            //    self.select_previous();

            //    // Get the index of the newly selected node
            //    let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
            //    Some(Action::DisplayLogs {
            //        peer_id: self.active_nodes[node_idx],
            //    })
            //}
            //KeyCode::Down => {
            //    self.select_next();

            //    // Get the index of the newly selected node
            //    let node_idx = self.clamp(self.list_state.selected().unwrap_or(0));
            //    Some(Action::DisplayLogs {
            //        peer_id: self.active_nodes[node_idx],
            //    })
            //}
            KeyCode::Esc => Some(Action::CloseNodeCommands),
            _ => None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        Clear.render(area, frame.buffer_mut());

        let block = Block::new()
            .title("Node Commands")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Color::LightRed);

        if self.node.is_none() {
            return;
        }

        let node = self.node.unwrap();

        let line = Line::from(vec![
            Span::raw("Connect node"),
            Span::raw(" "),
            Span::styled(
                node.to_string(),
                Style::new().add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw(" "),
            Span::raw("to:"),
        ]);

        let paragraph = Paragraph::new(line).block(block);

        paragraph.render(area, frame.buffer_mut());
    }

    pub fn update(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::DisplayNodeCommands { peer_id } => {
                self.node = Some(peer_id);
                None
            }
            _ => None,
        }
    }
}
