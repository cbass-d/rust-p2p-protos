use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent};
use libp2p::PeerId;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, Padding, Paragraph, Widget, Wrap},
};

use tracing::{debug, warn};

use crate::{
    messages::NetworkEvent,
    node::info::IdentifyInfo as NodeIdentifyInfo,
    tui::{
        action_queue::ActionQueue,
        app::Action,
        components::popup::PopUpContent,
        event_handler::{KeyResult, TuiEventHandler, TuiKeyCtx},
    },
};

/// Component for displaying list of local KAD stored records
#[derive(Debug)]
pub(crate) struct RecordsList {
    /// The node for which we are performing commands for
    node: Option<PeerId>,

    records: Vec<(String, String)>,
}

impl RecordsList {
    /// Build a fresh `IdentifyInfo` component
    pub fn new() -> Self {
        Self {
            node: None,
            records: vec![],
        }
    }

    /// Set the node for the component context
    pub fn set_node(&mut self, node: PeerId) {
        self.node = Some(node);
    }

    /// Set the info for the component context
    pub fn set_records(&mut self, records: Vec<(String, String)>) {
        self.records = records;
    }

    /// Handle a key event comming from the TUI
    pub fn handle_key_event(
        &mut self,
        key_event: KeyEvent,
        actions: &mut ActionQueue,
    ) -> Result<()> {
        if let Some(peer_id) = self.node
            && key_event.code == KeyCode::Esc
        {
            actions.push(Action::Popup {
                content: PopUpContent::NodeCommands,
                peer_id,
            });
        }

        Ok(())
    }

    /// Render the `IdentifyInfo` component with the current context
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        Clear.render(area, frame.buffer_mut());

        let footer_text =
            Line::from(vec![Span::raw("<Esc> exit")]).style(Style::new().fg(Color::White));

        let block = Block::new()
            .title("Local Kademlia Stored Records")
            .title_alignment(Alignment::Center)
            .title_bottom(footer_text)
            .borders(Borders::ALL)
            .border_style(Color::LightRed)
            .padding(Padding::uniform(1));

        if self.node.is_none() {
            return;
        }

        // Render the info is present as a Paragraph
        if !self.records.is_empty() {
            let mut header = vec![
                Line::from_iter([
                    Span::from("Key").style(Style::new()).bold().underlined(),
                    Span::from(" - "),
                    Span::from("Value").style(Style::new()).bold().underlined(),
                ]),
                Line::from(""),
            ];
            let mut records = self
                .records
                .iter()
                .map(|r| Line::from(format!("{} - {}", r.0, r.1)))
                .collect();

            header.append(&mut records);
            let list = List::new(header).block(block);

            list.render(area, frame.buffer_mut());
        } else {
            Paragraph::new("--- No records to display ---")
                .block(block)
                .alignment(Alignment::Center)
                .render(area, frame.buffer_mut());
        }
    }
}

impl TuiEventHandler for RecordsList {
    fn on_network_event(&mut self, event: &NetworkEvent, _actions: &mut ActionQueue) {
        if let NetworkEvent::RecordsList { records } = event {
            self.set_records(records.to_owned());
            debug!(target: "tui::records_list", "setting records list component");
        }
    }

    fn on_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        actions: &mut ActionQueue,
        _ctx: &TuiKeyCtx<'_>,
    ) -> KeyResult {
        if let Err(e) = self.handle_key_event(key, actions) {
            warn!(target: "tui::records_list", error = %e, "key handler error");
        }
        KeyResult::Handled
    }
}
